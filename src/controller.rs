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
    GcodeError(Vec<(i32, Response)>),
    SerialError(String),
}

impl std::error::Error for ControllerError {}

impl fmt::Display for ControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControllerError::ParseError { message, input } => {
                write!(f, "Failed to parse '{}': {}", input, message)
            }
            ControllerError::GcodeError(errors) => {
                for error in errors {
                    writeln!(f, "Line {}: {}", error.0, error.1)?;
                }

                Ok(())
            }
            ControllerError::SerialError(message) => {
                write!(f, "Serial error: {}", message)
            }
        }
    }
}

pub struct Controller {
    pub prio_serial: (channel::Sender<Command>, channel::Receiver<Push>),
    prio_serial_internal: (channel::Sender<Push>, channel::Receiver<Command>),
    pub serial: (channel::Sender<Command>, channel::Receiver<Response>),
    serial_internal: (channel::Sender<Response>, channel::Receiver<Command>),
    serial_handles: Option<(thread::JoinHandle<()>, thread::JoinHandle<()>)>,
    connected: Arc<AtomicBool>,
}

impl Controller {
    pub fn new() -> Self {
        let (prio_send_tx, prio_send_rx) = channel::bounded(0);
        let (prio_recv_tx, prio_recv_rx) = channel::bounded(0);

        let (send_tx, send_rx) = channel::bounded(0);
        let (recv_tx, recv_rx) = channel::unbounded();

        Self {
            prio_serial: (prio_send_tx, prio_recv_rx),
            prio_serial_internal: (prio_recv_tx, prio_send_rx),
            serial: (send_tx, recv_rx),
            serial_internal: (recv_tx, send_rx),
            serial_handles: None,
            connected: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn connect(&mut self, serial: Box<dyn serialport::SerialPort>, verbose_logging: bool) {
        let mut writer = io::BufWriter::new(serial.try_clone().unwrap());
        let mut reader = io::BufReader::new(serial.try_clone().unwrap());

        let (prio_recv_tx, prio_send_rx) = self.prio_serial_internal.clone();
        let (recv_tx, send_rx) = self.serial_internal.clone();

        let send_connected = self.connected.clone();
        let recv_connected = self.connected.clone();

        self.connected.store(true, Ordering::Relaxed);

        fn log_err<R, T: std::error::Error>(err: T) -> Result<R, T> {
            error!("{}", ControllerError::SerialError(err.to_string()));
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
                            .write_all(format!("{}\n", gcode).as_bytes())
                            .or_else(log_err);
                    }
                    Command::Realtime(byte) => {
                        let _ = writer.write_all(&[byte]).or_else(log_err);
                    }
                }

                let _ = writer.flush().or_else(log_err);
            }

            while send_connected.load(Ordering::Relaxed) {
                if let Ok(command) = prio_send_rx.try_recv() {
                    send(&mut writer, command, verbose_logging);
                }

                if let Ok(command) = send_rx.try_recv() {
                    send(&mut writer, command, verbose_logging);
                }
            }
        });

        let recv_handle = thread::spawn(move || {
            while recv_connected.load(Ordering::Relaxed) {
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

        self.serial_handles = Some((send_handle, recv_handle));
    }

    pub fn disconnect(&mut self) {
        if let Some((send_handle, recv_handle)) = self.serial_handles.take() {
            self.connected.store(false, Ordering::Relaxed);

            let _ = send_handle.join();
            let _ = recv_handle.join();
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.disconnect();
    }
}
