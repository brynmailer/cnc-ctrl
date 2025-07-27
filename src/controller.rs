use std::fmt;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    sync::{Arc, mpsc},
    thread,
};

use regex::Regex;

#[derive(Debug)]
pub enum ControllerError {
    SerialError(std::io::Error),
    ParseError { message: String, input: String },
}

impl std::error::Error for ControllerError {}

impl fmt::Display for ControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControllerError::SerialError(err) => write!(f, "Serial communication error: {}", err),
            ControllerError::ParseError { message, input } => {
                write!(f, "Failed to parse '{}'. {}", input, message)
            }
        }
    }
}

struct Report {
    status: Option<String>,
    mpos: Option<(f32, f32, f32)>,
    bf: Option<(usize, usize)>,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            status: None,
            mpos: None,
            bf: None,
        }
    }
}

impl TryFrom<&str> for Report {
    type Error = ControllerError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let regex = Regex::new(r"^<([A-Za-z]+)(\|[^>]*)*>$").unwrap();
        if !regex.is_match(&value) {
            return Err(ControllerError::ParseError {
                message: "Not a valid realtime report".to_string(),
                input: value.to_string(),
            });
        }

        let content = value.strip_prefix("<").unwrap().strip_suffix(">").unwrap();
        let parts: Vec<&str> = content.split("|").collect();

        let mut report = Report::default();
        report.status = Some(parts[0].to_string());

        for part in &parts[1..] {
            if let Some(pos_str) = part.strip_prefix("MPos:") {
                // Machine position: MPos:0.000,0.000,0.000
                let coords: Vec<&str> = pos_str.split(",").collect();
                if coords.len() >= 3 {
                    report.mpos = Some((
                        coords[0].parse().unwrap_or(0.0),
                        coords[1].parse().unwrap_or(0.0),
                        coords[2].parse().unwrap_or(0.0),
                    ));
                }
            } else if let Some(buf_str) = part.strip_prefix("Bf:") {
                // Buffer state: Bf:15,128
                let buf_parts: Vec<&str> = buf_str.split(",").collect();
                if buf_parts.len() >= 2 {
                    report.bf = Some((
                        buf_parts[0].parse().unwrap_or(0),
                        buf_parts[1].parse().unwrap_or(0),
                    ));
                }
            }
        }

        Ok(report)
    }
}

pub enum Command {
    Gcode(String),
    Realtime(u8),
}

pub enum Message {
    Response(Response),
    Push(Push),
    Unknown(String),
}

pub enum Response {
    Ok,
    Err(u8),
}

pub enum Push {
    Report(Report),
}

impl From<&str> for Message {
    fn from(value: &str) -> Self {
        if value.contains("ok") {
            Message::Response(Response::Ok)
        } else if let Some(code) = value.strip_prefix("error:") {
            Message::Response(Response::Err(code.parse().unwrap()))
        } else if let Ok(report) = Report::try_from(value) {
            Message::Push(Push::Report(report))
        } else {
            Message::Unknown(value.to_string())
        }
    }
}

pub struct Controller {
    prio_serial_channel: Option<(mpsc::SyncSender<Command>, mpsc::Receiver<Push>)>,
    serial_channel: Option<(mpsc::SyncSender<Command>, mpsc::Receiver<Response>)>,
    serial_handles: Option<(thread::JoinHandle<()>, thread::JoinHandle<()>)>,
    running: Arc<AtomicBool>,
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

    pub fn start(&mut self, serial: Box<dyn serialport::SerialPort>) {
        let mut writer = io::BufWriter::new(serial.try_clone().unwrap());
        let mut reader = io::BufReader::new(serial.try_clone().unwrap());

        let (prio_send_tx, prio_send_rx) = mpsc::sync_channel(0);
        let (send_tx, send_rx) = mpsc::sync_channel(0);

        let (prio_recv_tx, prio_recv_rx) = mpsc::sync_channel(0);
        let (recv_tx, recv_rx) = mpsc::channel();

        let send_running = self.running.clone();
        let recv_running = self.running.clone();
        self.running.store(true, Ordering::Relaxed);

        fn log_err<R, T: std::error::Error>(err: T) -> Result<R, T> {
            eprintln!("{}", err);
            Err(err)
        }

        let send_handle = thread::spawn(move || {
            fn send(writer: &mut io::BufWriter<Box<dyn serialport::SerialPort>>, command: Command) {
                match command {
                    Command::Gcode(gcode) => {
                        writer
                            .write_all(format!("{}\n", gcode).as_bytes())
                            .or_else(log_err);
                    }
                    Command::Realtime(byte) => {
                        writer.write_all(&[byte]).or_else(log_err);
                    }
                }

                writer.flush().or_else(log_err);
            }

            while send_running.load(Ordering::Relaxed) {
                if let Ok(command) = prio_send_rx.try_recv() {
                    send(&mut writer, command);
                }

                if let Ok(command) = send_rx.try_recv() {
                    send(&mut writer, command);
                }
            }
        });

        let recv_handle = thread::spawn(move || {
            while recv_running.load(Ordering::Relaxed) {
                let mut response = String::new();
                reader.read_line(&mut response).or_else(log_err);
                response = response.trim().to_string();

                match Message::from(response.as_str()) {
                    Message::Push(push) => {
                        println!("RECV: PUSH < {}", response);
                        let _ = prio_recv_tx.try_send(push);
                    }
                    Message::Response(res) => {
                        println!("RECV: RESPONSE < {}", response);
                        recv_tx.send(res).unwrap();
                    }
                    Message::Unknown(msg) => println!("RECV: UNKNOWN < {}", msg),
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

            send_handle.join();
            recv_handle.join();

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
