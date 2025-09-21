pub mod command;
pub mod message;

use std::collections::VecDeque;
use std::io::{self, BufRead, Read, Write};
use std::time::Duration;
use std::{net, thread, time};

use anyhow::{Context, Result, anyhow, bail};
use crossbeam::channel;
use log::{debug, error, info, warn};

use crate::config::{SerialConfig, TcpConfig};

pub use self::command::{Command, Realtime};
pub use self::message::{Message, Response};

const TIMEOUT_MS: u64 = 60000;
const GRBL_RX_SIZE: usize = 1024;

pub struct Connection;

pub struct InactiveConnection {
    device: net::TcpStream,
}

pub struct ActiveConnection {
    device: net::TcpStream,
    pub sender: channel::Sender<(Command, Option<channel::Sender<Message>>)>,
}

impl Connection {
    pub fn new(config: &TcpConfig) -> Result<InactiveConnection> {
        let device = net::TcpStream::connect_timeout(
            &(format!("{}:{}", config.address, config.port).parse()?),
            time::Duration::from_millis(TIMEOUT_MS),
        )
        .with_context(|| {
            format!(
                "Failed to create TCP connection to {}:{}",
                config.address, config.port
            )
        })?;

        device.set_nonblocking(true)?;

        Ok(InactiveConnection { device })
    }
}

impl InactiveConnection {
    pub fn open(self) -> Result<ActiveConnection> {
        let mut writer = io::BufWriter::new(self.device.try_clone()?);
        let mut reader = io::BufReader::new(self.device.try_clone()?);

        let (cmd_tx, cmd_rx) = channel::bounded::<(Command, Option<channel::Sender<Message>>)>(0);

        thread::spawn(move || {
            let mut queued: VecDeque<(Command, Option<channel::Sender<Message>>)> = VecDeque::new();
            let mut sent = queued.clone();

            let mut receive =
                |sent: &mut VecDeque<(Command, Option<channel::Sender<Message>>)>| -> Result<()> {
                    let mut received = String::new();

                    match reader.read_line(&mut received) {
                        Ok(0) => {
                            bail!("EOF reached");
                        }
                        Ok(_) => {
                            let trimmed = received.trim();
                            info!("    <RECV {}", Message::from(trimmed));

                            if let Some((_, Some(msg_tx))) = sent.front() {
                                if let Err(err) = msg_tx.send(Message::from(trimmed)) {
                                    debug!("Failed to send message: {}", err);
                                }
                            }

                            if let Message::Response(_) = Message::from(trimmed) {
                                sent.pop_front();
                            }

                            Ok(())
                        }
                        Err(err) if err.kind() == io::ErrorKind::WouldBlock => Ok(()),
                        Err(err) => {
                            bail!("Failed to read data from connection: {}", err);
                        }
                    }
                };

            loop {
                let buffered_bytes = sent.iter().fold(0, |sum, (cmd, _)| match cmd {
                    Command::Block(block) => sum + block.len() + 1,
                    Command::Realtime(_) => sum,
                });

                cmd_rx.try_iter().for_each(|cmd| match cmd {
                    (Command::Block(_), _) => queued.push_back(cmd),
                    (Command::Realtime(_), _) => queued.push_front(cmd),
                });

                match queued.front() {
                    Some((cmd @ Command::Realtime(byte), _)) => {
                        info!("SND> {}", cmd);

                        if let Err(err) = writer.write(&[*byte as u8]) {
                            error!("{}", err);
                            break;
                        }

                        if let Err(err) = writer.flush() {
                            error!("{}", err);
                            break;
                        }

                        queued.pop_front();
                    }
                    Some((cmd @ Command::Block(block), _))
                        if buffered_bytes + block.len() + 1 < GRBL_RX_SIZE =>
                    {
                        info!("SND> {}", cmd);

                        if let Err(err) = write!(writer, "{}\n", block) {
                            error!("{}", err);
                            break;
                        }

                        if let Err(err) = writer.flush() {
                            error!("{}", err);
                            break;
                        }

                        sent.push_back(queued.pop_front().unwrap());
                    }
                    _ => {
                        if let Err(err) = receive(&mut sent) {
                            error!("{}", err);
                            break;
                        }
                    }
                }
            }

            info!("Connection worker exited");
        });

        Ok(ActiveConnection {
            device: self.device,
            sender: cmd_tx,
        })
    }
}

impl ActiveConnection {
    pub fn send(&self, cmd: Command) -> Result<channel::Receiver<Message>> {
        let (tx, rx) = channel::unbounded();

        self.sender.send((cmd, Some(tx)))?;

        Ok(rx)
    }
}

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        warn!("Sending stop signal to Grbl");
        if let Err(err) = self.send(Command::Realtime(Realtime::Stop)) {
            error!("Failed to stop Grbl: {}", err);
        }

        thread::sleep(Duration::from_millis(500));
        if let Err(err) = self.device.shutdown(net::Shutdown::Both) {
            error!("Failed to shut down device: {}", err);
        }
    }
}

/*
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
*/
