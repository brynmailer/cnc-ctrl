use std::fmt;
use std::io::{self, BufRead, Write};
use std::{sync::mpsc, thread, time};

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
        if !regex.is_match(value) {
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

pub struct Controller {
    report_rx: Option<mpsc::Receiver<Report>>,
    monitor_handle: Option<thread::JoinHandle<()>>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            report_rx: None,
            monitor_handle: None,
        }
    }

    pub fn start_monitoring(&mut self, serial: Box<dyn serialport::SerialPort>) {
        let (report_tx, report_rx) = mpsc::sync_channel(0);

        let handle = thread::spawn(move || {
            let mut writer = io::BufWriter::new(serial.try_clone().unwrap());
            let mut reader = io::BufReader::new(serial.try_clone().unwrap());

            fn log_err<R, T: std::error::Error>(err: T) -> Result<R, T> {
                eprintln!("MONITOR: {}", err);
                Err(err)
            }

            loop {
                writer.write_all("?".as_bytes()).or_else(log_err);
                writer.flush().or_else(log_err);

                let mut response = String::new();
                reader.read_line(&mut response).or_else(log_err);
                match Report::try_from(response.trim()) {
                    Ok(report) => {
                        report_tx.try_send(report);
                    }
                    Err(err) => {
                        let _: Result<Report, ControllerError> = log_err(err);
                    }
                }

                thread::sleep(time::Duration::from_millis(200));
            }
        });

        self.report_rx = Some(report_rx);
        self.monitor_handle = Some(handle);
    }

    pub fn stop_monitoring(&mut self) {
        if let Some(handle) = self.monitor_handle.take() {}
    }
}
