pub mod command;
pub mod message;

use std::collections::VecDeque;
use std::io::{self, BufRead, Write};
use std::sync::atomic;
use std::{net, thread, time};

use anyhow::{Context, Result, bail};
use crossbeam::channel;
use log::{debug, error, info};

use crate::config::TcpConfig;

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
    worker_handle: Option<thread::JoinHandle<()>>,
    pub tx: channel::Sender<(Command, Option<channel::Sender<Message>>)>,
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

        let handle = thread::spawn(move || {
            let mut queued: VecDeque<(Command, Option<channel::Sender<Message>>)> = VecDeque::new();
            let mut sent = queued.clone();

            loop {
                #[allow(unused_assignments)]
                let mut sleeping = true;

                cmd_rx.try_iter().for_each(|cmd| match cmd {
                    (Command::Block(_), _) => queued.push_back(cmd),
                    (Command::Realtime(_), _) => queued.push_front(cmd),
                });

                match queued.front() {
                    Some((cmd @ Command::Realtime(byte), _)) => {
                        sleeping = false;
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
                    // Have to do this here so that the match case falls through to receiving
                    // if the grbl rx buffer is full, without having to calculate the currently
                    // occupied space in the buffer every worker cycle
                        if sent
                            .iter()
                            .fold(block.len() + 1, |sum, (cmd, _)| match cmd {
                                Command::Block(block) => sum + block.len() + 1,
                                Command::Realtime(_) => sum,
                            })
                            < GRBL_RX_SIZE =>
                    {
                        sleeping = false;
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
                        let mut received = String::new();
                        match reader.read_line(&mut received) {
                            Ok(0) => break,
                            Ok(_) => {
                                sleeping = false;
                                let trimmed = received.trim();
                                info!("    <RECV {:?}", trimmed);

                                if let Some((_, Some(msg_tx))) = sent.front() {
                                    if let Err(err) = msg_tx.send(Message::from(trimmed)) {
                                        debug!("Failed to send message to command issuer: {}", err);
                                    }
                                }

                                if let Message::Response(_) = Message::from(trimmed) {
                                    sent.pop_front();
                                }
                            }
                            Err(err) if err.kind() == io::ErrorKind::WouldBlock => (),
                            Err(err) => {
                                error!("Failed to read data from connection: {}", err);
                                break;
                            }
                        }
                    }
                }

                // Sleep during periods of low/no activity to avoid busy waiting unnecessarily
                if sleeping {
                    thread::sleep(time::Duration::from_millis(1));
                }
            }

            info!("Connection worker exited");
        });

        Ok(ActiveConnection {
            device: self.device,
            worker_handle: Some(handle),
            tx: cmd_tx,
        })
    }
}

impl ActiveConnection {
    pub fn send(
        &self,
        cmd: Command,
        running: Option<&atomic::AtomicBool>,
    ) -> Result<channel::Receiver<Message>> {
        let (tx, rx) = channel::unbounded();

        if let Some(running) = running {
            while running.load(atomic::Ordering::Relaxed) {
                match self.tx.try_send((cmd.clone(), Some(tx.clone()))) {
                    Ok(_) => (),
                    Err(channel::TrySendError::Full(_)) => {
                        // Sleep to avoid busy loop
                        thread::sleep(time::Duration::from_millis(1));
                    }
                    Err(channel::TrySendError::Disconnected((val, _))) => {
                        bail!("Failed to send command '{}'", val);
                    }
                }
            }
        } else {
            self.tx.send((cmd, Some(tx)))?;
        }

        Ok(rx)
    }
}

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        if let Err(err) = self.send(Command::Realtime(Realtime::Stop), None) {
            error!("Failed to stop Grbl: {}", err);
        }

        if let Err(err) = self.device.shutdown(net::Shutdown::Both) {
            error!("Failed to shut down connection: {}", err);
        }

        if let Some(handle) = self.worker_handle.take() {
            if let Err(_) = handle.join() {
                error!("Failed to wait for connection worker to exit");
            }
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
