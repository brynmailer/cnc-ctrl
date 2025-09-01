mod command;
mod message;

use std::collections::VecDeque;
use std::io::{self, BufRead, Read, Write};
use std::{net, thread};

use anyhow::{Context, Result};
use crossbeam::channel;
use log::{error, info, warn};
use serde::Deserialize;

use self::command::Command;
use self::message::Message;

#[derive(Debug, Deserialize)]
pub enum ConnectionConfig {
    TCP {
        address: String,
        port: u16,
        timeout: u64,
    },
}

pub enum Connection<T: Device> {
    Inactive(InactiveConnection<T>),
    Active(ActiveConnection<T>),
}

pub struct InactiveConnection<T: Device> {
    device: Option<T>,
}

pub struct ActiveConnection<T: Device> {
    device: T,
    channel: (channel::Sender<Command>, channel::Receiver<Message>),
    handle: thread::JoinHandle<()>,
}

impl<T: Device> Connection<T> {
    pub fn new(device: T) -> InactiveConnection<T> {
        InactiveConnection {
            device: Some(device),
        }
    }
}

impl<T: Device> InactiveConnection<T> {
    pub fn open(&mut self) -> Result<ActiveConnection<T>> {
        let device = self
            .device
            .take()
            .context("Attempted to open connection without a device")?;

        let mut reader = io::BufReader::new(device.try_clone()?);
        let writer = io::BufWriter::new(device.try_clone()?);

        let (cmd_tx, cmd_rx) = channel::unbounded();
        let (msg_tx, msg_rx) = channel::unbounded();

        let handle = thread::spawn(move || {
            let mut received = String::new();

            let mut grbl_queue: VecDeque<Command> = VecDeque::new();

            loop {
                match reader.read_line(&mut received) {
                    Ok(0) => {
                        warn!("Connection closed");
                        break;
                    }
                    Ok(_) => {
                        let message = Message::from(received.trim());

                        match message {
                            Message::Response(_) => match grbl_queue.pop_front() {
                                Some(
                                    Command::Gcode(_, Some(line)) | Command::System(_, Some(line)),
                                ) => info!("    RECV:{} < {}", line, message),
                                _ => (),
                            },
                            _ => info!("    RECV < {}", message),
                        }

                        msg_tx.send(message);
                    }
                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => (),
                    Err(err) => {
                        error!("Failed to read data from connection: {}", err);
                        break;
                    }
                }

                // TODO: Write next command in channel, prioritising realtime commands
                while let Ok(command) = cmd_rx.recv() {}
            }
        });

        Ok(ActiveConnection {
            device,
            channel: (cmd_tx, msg_rx),
            handle,
        })
    }
}

pub trait Device: Read + Write + Send + Sync + 'static {
    fn try_clone(&self) -> Result<Self>
    where
        Self: Sized;
}

impl Device for net::TcpStream {
    fn try_clone(&self) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(self.try_clone()?)
    }
}
