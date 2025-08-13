pub mod command;
pub mod message;
pub mod serial;

use log::{debug, error};
use std::fmt;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::Arc, thread};

use crossbeam::channel;

use command::Command;
use message::{Message, Push, Response};

#[derive(Debug)]
pub enum ControllerError {
    ParseError { message: String, input: String },
    GcodeError(i32, Response),
    SerialError(String),
}

impl std::error::Error for ControllerError {}

impl fmt::Display for ControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControllerError::ParseError { message, input } => {
                write!(f, "Failed to parse '{}': {}", input, message)
            }
            ControllerError::GcodeError(line_number, error) => {
                write!(f, "Line {}: {}", line_number, error)
            }
            ControllerError::SerialError(message) => {
                write!(f, "Serial error: {}", message)
            }
        }
    }
}

pub struct Controller {
    pub prio_serial_channel: Option<(channel::Sender<Command>, channel::Receiver<Push>)>,
    pub serial_channel: Option<(channel::Sender<Command>, channel::Receiver<Response>)>,
    pub running: Arc<AtomicBool>,

    serial_handles: Option<(thread::JoinHandle<()>, thread::JoinHandle<()>)>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            prio_serial_channel: None,
            serial_channel: None,
            serial_handles: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&mut self, serial: Box<dyn serialport::SerialPort>, verbose_logging: bool) {
        let mut writer = io::BufWriter::new(serial.try_clone().unwrap());
        let mut reader = io::BufReader::new(serial.try_clone().unwrap());

        let (prio_send_tx, prio_send_rx) = channel::bounded(0);
        let (send_tx, send_rx) = channel::bounded(0);

        let (prio_recv_tx, prio_recv_rx) = channel::bounded(0);
        let (recv_tx, recv_rx) = channel::unbounded();

        let send_running = self.running.clone();
        let recv_running = self.running.clone();

        self.running.store(true, Ordering::Relaxed);

        fn log_err<R, T: std::error::Error>(err: T) -> Result<R, T> {
            error!("{}", err);
            Err(err)
        }

        let send_handle = thread::spawn(move || {
            fn send(
                writer: &mut io::BufWriter<Box<dyn serialport::SerialPort>>,
                command: Command,
                verbose: bool,
            ) {
                if verbose {
                    debug!("Serial (SND) > {}", command);
                }

                match command {
                    Command::Gcode(gcode) => {
                        let _ = writer
                            .write_all(format!("{}\r\n", gcode).as_bytes())
                            .or_else(log_err);
                    }
                    Command::Realtime(byte) => {
                        let _ = writer.write_all(&[byte]).or_else(log_err);
                    }
                }

                let _ = writer.flush().or_else(log_err);
            }

            while send_running.load(Ordering::Relaxed) {
                if let Ok(command) = prio_send_rx.try_recv() {
                    send(&mut writer, command, verbose_logging);
                }

                if let Ok(command) = send_rx.try_recv() {
                    send(&mut writer, command, verbose_logging);
                }
            }
        });

        let recv_handle = thread::spawn(move || {
            while recv_running.load(Ordering::Relaxed) {
                let mut response = String::new();
                let _ = reader.read_line(&mut response).or_else(log_err);
                let message = Message::from(response.trim());

                if verbose_logging {
                    debug!("Serial (RECV) < {}", message);
                }

                match message {
                    Message::Push(push) => {
                        let _ = prio_recv_tx.try_send(push);
                    }
                    Message::Response(res) => {
                        recv_tx.send(res).unwrap();
                    }
                    _ => continue,
                }
            }
        });

        self.prio_serial_channel = Some((prio_send_tx, prio_recv_rx));
        self.serial_channel = Some((send_tx, recv_rx));
        self.serial_handles = Some((send_handle, recv_handle));
    }

    pub fn stop(&mut self) {
        if let Some((send_handle, recv_handle)) = self.serial_handles.take() {
            self.running.store(false, Ordering::Relaxed);

            let _ = send_handle.join();
            let _ = recv_handle.join();

            self.prio_serial_channel.take();
            self.serial_channel.take();
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            self.stop();
        }
    }
}
