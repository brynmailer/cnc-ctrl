use std::fmt;

use regex::Regex;

use super::ControllerError;

pub enum Message {
    Response(Response),
    Push(Push),
    Unknown(String),
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::Response(response) => write!(f, "RES '{}'", response),
            Message::Push(push) => write!(f, "PUSH '{}'", push),
            Message::Unknown(message) => write!(f, "UNKNOWN '{}'", message),
        }
    }
}

impl From<&str> for Message {
    fn from(value: &str) -> Self {
        if let Ok(response) = Response::try_from(value) {
            Message::Response(response)
        } else if let Ok(push) = Push::try_from(value) {
            Message::Push(push)
        } else {
            Message::Unknown(value.to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub enum Response {
    Ok,
    Error(u8),
    Probe {
        raw: String,
        coords: (f64, f64, f64),
    },
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Response::Ok => write!(f, "ok"),
            Response::Error(code) => write!(f, "error:{}", code),
            Response::Probe { raw, .. } => write!(f, "{}", raw),
        }
    }
}

impl TryFrom<&str> for Response {
    type Error = ControllerError;

    fn try_from(value: &str) -> Result<Self, ControllerError> {
        if value.contains("ok") {
            Ok(Response::Ok)
        } else if let Some(code) = value.strip_prefix("error:") {
            let error_code = code.parse().map_err(|_| ControllerError::ParseError {
                message: "Invalid error code".to_string(),
                input: value.to_string(),
            })?;
            Ok(Response::Error(error_code))
        } else if value.starts_with("[PRB:") {
            let regex = Regex::new(r"^\[PRB:([+-]?\d+\.\d+),([+-]?\d+\.\d+),([+-]?\d+\.\d+),([+-]?\d+\.\d+),([+-]?\d+\.\d+):([01])\]$").unwrap();

            if let Some(captures) = regex.captures(value) {
                let x = captures[1]
                    .parse::<f64>()
                    .map_err(|_| ControllerError::ParseError {
                        message: "Invalid X coordinate".to_string(),
                        input: value.to_string(),
                    })?;
                let y = captures[2]
                    .parse::<f64>()
                    .map_err(|_| ControllerError::ParseError {
                        message: "Invalid Y coordinate".to_string(),
                        input: value.to_string(),
                    })?;
                let z = captures[3]
                    .parse::<f64>()
                    .map_err(|_| ControllerError::ParseError {
                        message: "Invalid Z coordinate".to_string(),
                        input: value.to_string(),
                    })?;

                Ok(Response::Probe {
                    raw: value.to_string(),
                    coords: (x, y, z),
                })
            } else {
                Err(ControllerError::ParseError {
                    message: "Invalid probe response format".to_string(),
                    input: value.to_string(),
                })
            }
        } else {
            Err(ControllerError::ParseError {
                message: "Not a valid response".to_string(),
                input: value.to_string(),
            })
        }
    }
}

pub enum Push {
    Report(Report),
}

impl fmt::Display for Push {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Push::Report(report) => write!(f, "{}", report.raw),
        }
    }
}

impl TryFrom<&str> for Push {
    type Error = ControllerError;

    fn try_from(value: &str) -> Result<Self, ControllerError> {
        let report = Report::try_from(value)?;
        Ok(Push::Report(report))
    }
}

pub struct Report {
    pub raw: String,
    pub status: Option<Status>,
    pub mpos: Option<(f32, f32, f32)>,
    pub bf: Option<(usize, usize)>,
}

pub enum Status {
    Idle,
    Home,
    Jog,
    Unknown,
}

impl From<&str> for Status {
    fn from(value: &str) -> Self {
        match value {
            "Idle" => Status::Idle,
            "Home" => Status::Home,
            "Jog" => Status::Jog,
            _ => Status::Unknown,
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

        let mut report = Report {
            raw: value.to_string(),
            status: Some(Status::from(parts[0])),
            mpos: None,
            bf: None,
        };

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
