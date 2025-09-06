pub mod command;
pub mod message;

use std::collections::VecDeque;
use std::io::{self, BufRead, Read, Write};
use std::{net, thread, time};

use anyhow::{Context, Result, anyhow, bail, ensure};
use crossbeam::channel::{self};
use log::{error, info, warn};

use crate::config::{SerialConfig, TcpConfig};

pub use self::command::Command;
pub use self::message::Message;

pub enum Connection<T: Device> {
    Inactive(InactiveConnection<T>),
    Active(ActiveConnection<T>),
}

pub struct InactiveConnection<T: Device> {
    device: T,
}

pub struct ActiveConnection<T: Device> {
    device: T,
    pub sender: channel::Sender<(Command, Option<channel::Sender<Message>>)>,
    pub receiver: channel::Receiver<Message>,
}

impl Connection<net::TcpStream> {
    pub fn tcp(config: &TcpConfig) -> Result<InactiveConnection<net::TcpStream>> {
        let device = (|| -> Result<net::TcpStream> {
            let stream = net::TcpStream::connect_timeout(
                &(format!("{}:{}", config.address, config.port).parse()?),
                time::Duration::from_secs(config.timeout),
            )?;

            stream.set_nonblocking(true)?;

            Ok(stream)
        })()
        .with_context(|| {
            format!(
                "Failed to create TCP connection to {}:{}",
                config.address, config.port
            )
        })?;

        Ok(InactiveConnection { device })
    }
}

impl Connection<Box<dyn serialport::SerialPort>> {
    pub fn serial(
        config: &SerialConfig,
    ) -> Result<InactiveConnection<Box<dyn serialport::SerialPort>>> {
        let device = serialport::new(config.port.clone(), config.baud_rate)
            .timeout(time::Duration::from_secs(config.timeout))
            .open()
            .with_context(|| format!("Failed to open serial port {}", config.port))?;

        Ok(InactiveConnection { device })
    }
}

impl<T: Device> InactiveConnection<T> {
    pub fn open(self) -> Result<ActiveConnection<T>> {
        let mut writer = io::BufWriter::new(self.device.try_clone()?);
        let mut reader = io::BufReader::new(self.device.try_clone()?);

        let (cmd_tx, cmd_rx) = channel::bounded(0);
        let (msg_tx, msg_rx) = channel::unbounded();
        let (res_tx, res_rx) = channel::unbounded::<Option<channel::Sender<Message>>>();

        let send_handle = thread::spawn(move || {
            let mut queued: VecDeque<Command> = VecDeque::new();
            let mut sent: VecDeque<Command> = VecDeque::new();

            loop {
                match cmd_rx.recv() {
                    Ok(cmd) => match cmd {
                        Command::Realtime(_) => queued.push_front(cmd),
                        Command::Block(..) => queued.push_back(cmd),
                    },
                    Err(channel::RecvError) => {
                        break;
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
                                    error!(
                                        "Failed to write command '{}' to connection: {}",
                                        cmd, err
                                    );
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
                        error!("Failed to flush command buffer: {}", err);
                        break;
                    }
                }
                None => (),
            }

            warn!("Closed send worker");
        });

        thread::spawn(move || {
            let mut received = String::new();

            loop {
                match reader.read_line(&mut received) {
                    Ok(0) => {
                        warn!("EOF reached");
                        break;
                    }
                    Ok(_) => {
                        match Message::from(received.trim()) {
                            Message::Response(_) => {}
                            _ => result = (msg, None),
                        }

                        info!("{}", info);
                        if let Err(channel::SendError(_)) = msg_tx.send(result) {
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
            }

            warn!("Closed read worker");
        });

        Ok(ActiveConnection {
            device: self.device,
            sender: cmd_tx,
            receiver: msg_rx,
        })
    }
}

impl<T: Device> ActiveConnection<T> {
    pub fn stream(&self, cmds: Vec<Command>) -> Result<Vec<(usize, Message)>> {
        let mut receivers = Vec::new();

        for cmd in cmds {
            if let Command::Block(block) = cmd {
                let (tx, rx) = channel::unbounded();
                receivers.push(rx);
                self.sender
                    .send((Command::Block(block.clone()), Some(tx)))?;
            }
        }

        let mut responses = Vec::new();
        for (index, rx) in receivers.iter().enumerate() {
            responses.push((index, rx.recv()?));
        }

        Ok(responses)
    }

    pub fn close(self) -> InactiveConnection<T> {
        InactiveConnection {
            device: self.device,
        }
    }
}

pub trait Device: Read + Write + Send + 'static {
    fn id(&self) -> Result<String>;

    fn try_clone(&self) -> Result<Self>
    where
        Self: Sized;
}

impl Device for net::TcpStream {
    fn id(&self) -> Result<String> {
        Ok(self.peer_addr()?.to_string())
    }

    fn try_clone(&self) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(self.try_clone()?)
    }
}

impl Device for Box<dyn serialport::SerialPort> {
    fn id(&self) -> Result<String> {
        self.name()
            .ok_or(anyhow!("Failed to get name of serial port"))
    }

    fn try_clone(&self) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(self.as_ref().try_clone()?)
    }
}
