mod command;
mod message;

use std::collections::VecDeque;
use std::io::{self, BufRead, Read, Write};
use std::{net, thread};

use anyhow::{Context, Result};
use crossbeam::channel;
use log::{error, info, warn};

use self::command::Command;
use self::message::Message;

pub enum Connection<T: Device> {
    Inactive(InactiveConnection<T>),
    Active(ActiveConnection<T>),
}

impl<T: Device> Connection<T> {
    pub fn new(device: T) -> InactiveConnection<T> {
        InactiveConnection {
            device: Some(device),
        }
    }
}

pub struct InactiveConnection<T: Device> {
    device: Option<T>,
}

pub struct ActiveConnection<T: Device> {
    device: T,
    channel: (channel::Sender<Command>, channel::Receiver<Message>),
    handle: thread::JoinHandle<()>,
}

impl<T: Device> InactiveConnection<T> {
    pub fn open(mut self) -> Result<ActiveConnection<T>> {
        let device = self
            .device
            .take()
            .context("Attempted to open connection without a device")?;

        let mut writer = io::BufWriter::new(device.try_clone()?);
        let mut reader = io::BufReader::new(device.try_clone()?);

        let (cmd_tx, cmd_rx) = channel::bounded(0);
        let (msg_tx, msg_rx) = channel::unbounded();

        let handle = thread::spawn(move || {
            let mut received = String::new();
            let mut queued: VecDeque<Command> = VecDeque::new();
            let mut sent: VecDeque<Command> = VecDeque::new();

            'outer: loop {
                match reader.read_line(&mut received) {
                    Ok(0) => {
                        warn!("EOF reached");
                        break;
                    }
                    Ok(_) => {
                        let msg = Message::from(received.trim());

                        match msg {
                            Message::Response(_) => match sent.pop_front() {
                                Some(Command::Block(_, Some(sequence))) => {
                                    info!("    RECV:{} < {}", sequence, msg)
                                }
                                _ => (),
                            },
                            _ => info!("    RECV < {}", msg),
                        }

                        if let Err(channel::SendError(_)) = msg_tx.send(msg) {
                            warn!("Channel disconnected");
                            break;
                        };
                    }
                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => (),
                    Err(err) => {
                        error!("Failed to read data from connection: {}", err);
                        break;
                    }
                }

                loop {
                    match cmd_rx.try_recv() {
                        Ok(cmd) => match cmd {
                            Command::Realtime(_) => queued.push_front(cmd),
                            Command::Block(..) => queued.push_back(cmd),
                        },
                        Err(channel::TryRecvError::Empty) => break,
                        Err(channel::TryRecvError::Disconnected) => {
                            warn!("Channel disconnected");
                            break 'outer;
                        }
                    }
                }

                let buffered_bytes = sent.iter().fold(0, |sum, cmd| match cmd {
                    Command::Block(block, _) => sum + (block.len() + 1),
                    Command::Realtime(..) => sum,
                });

                match queued.pop_front() {
                    Some(cmd) => {
                        match cmd {
                            Command::Block(ref block, ref sequence) => {
                                if buffered_bytes + (block.len() + 1) < 1023 {
                                    if let Err(err) = write!(writer, "{}\n", block) {
                                        error!("Failed to send command '{}': {}", cmd, err);
                                        queued.push_front(cmd);
                                    } else {
                                        if let Some(number) = sequence {
                                            info!("SEND:{} > {}", number, cmd);
                                        } else {
                                            info!("SEND: > {}", cmd);
                                        }

                                        sent.push_back(cmd);
                                    }
                                } else {
                                    queued.push_front(cmd);
                                }
                            }
                            Command::Realtime(_) => {
                                if let Err(err) = write!(writer, "{}", cmd) {
                                    error!("Failed to send command '{}': {}", cmd, err);
                                    queued.push_front(cmd);
                                } else {
                                    info!("SEND > {}", cmd);
                                }
                            }
                        }

                        if let Err(err) = writer.flush() {
                            error!("Failed to write to connection: {}", err);
                            break;
                        }
                    }
                    None => (),
                }
            }

            warn!("Connection closed");
        });

        Ok(ActiveConnection {
            device,
            channel: (cmd_tx, msg_rx),
            handle,
        })
    }
}

impl<T: Device> ActiveConnection<T> {
    pub fn close(self) -> InactiveConnection<T> {
        InactiveConnection {
            device: Some(self.device),
        }
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
