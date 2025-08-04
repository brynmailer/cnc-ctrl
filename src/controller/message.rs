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
        if value.contains("ok") {
            Message::Response(Response::Ok)
        } else if let Some(code) = value.strip_prefix("error:") {
            Message::Response(Response::Error(code.parse().unwrap()))
        } else if let Ok(report) = Report::try_from(value) {
            Message::Push(Push::Report(report))
        } else {
            Message::Unknown(value.to_string())
        }
    }
}

pub enum Response {
    Ok,
    Error(u8),
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Response::Ok => write!(f, "ok"),
            Response::Error(code) => write!(f, "error:{}", code),
        }
    }
}

pub enum Push {
    Report(Report),
}

impl fmt::Display for Push {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Push::Report(report) => write!(f, "{}", report.string),
        }
    }
}

pub struct Report {
    pub string: String,
    pub status: Option<Status>,
    pub mpos: Option<(f32, f32, f32)>,
    pub bf: Option<(usize, usize)>,
}

pub enum Status {
    Idle,
    Home,
    Jog,
    Unknown(String),
}

impl From<&str> for Status {
    fn from(value: &str) -> Self {
        match value {
            "Idle" => Status::Idle,
            "Home" => Status::Home,
            "Jog" => Status::Jog,
            _ => Status::Unknown(value.to_string()),
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
            string: value.to_string(),
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
