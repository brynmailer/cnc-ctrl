use std::fmt;

use regex::Regex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Invalid error code '{0}'")]
    InvalidErrorCode(String),

    #[error("Invalid alarm code '{0}'")]
    InvalidAlarmCode(String),

    #[error("Unknown message format '{0}'")]
    UnknownFormat(String),

    #[error("Failed to initialise RegExp")]
    RegExp,
}

pub enum Message {
    Response(Response),
    Push(Push),
    Unknown(String),
}

#[derive(Debug, Clone)]
pub enum Response {
    Ok,
    Error(u8),
}

pub enum Push {
    Alarm(u8),
    Report(Report, String),
    Feedback(Feedback, String),
}

pub struct Report {
    pub status: String,
}

pub struct Feedback {
    pub kind: String,
    pub data: String,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::Response(response) => match response {
                Response::Ok => write!(f, "ok"),
                Response::Error(code) => write!(f, "error:{}", code),
            },
            Message::Push(push) => match push {
                Push::Alarm(code) => write!(f, "ALARM:{}", code),
                Push::Report(_, raw) => write!(f, "{}", raw),
                Push::Feedback(_, raw) => write!(f, "{}", raw),
            },
            Message::Unknown(value) => write!(f, "{}", value),
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

impl TryFrom<&str> for Response {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, ParseError> {
        let error_regex = Regex::new(r"^error:(\d+)$").map_err(|_| ParseError::RegExp)?;

        match value {
            "ok" => Ok(Response::Ok),
            value if error_regex.is_match(value) => {
                let code = value
                    .strip_prefix("error:")
                    .unwrap()
                    .parse()
                    .map_err(|_| ParseError::InvalidErrorCode(value.to_string()))?;

                Ok(Response::Error(code))
            }
            _ => Err(ParseError::UnknownFormat(value.to_string())),
        }
    }
}

impl TryFrom<&str> for Push {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, ParseError> {
        let alarm_regex = Regex::new(r"^ALARM:(\d+)$").map_err(|_| ParseError::RegExp)?;
        let report_regex =
            Regex::new(r"^<([^,|>]+)(?:\|([^>]*))?>$").map_err(|_| ParseError::RegExp)?;
        let feedback_regex =
            Regex::new(r"^\[(MSG|GC|PRB|TLO|G\d+):[^\]]*\]$").map_err(|_| ParseError::RegExp)?;

        match value {
            value if alarm_regex.is_match(value) => {
                let code = value
                    .strip_prefix("ALARM:")
                    .unwrap()
                    .parse()
                    .map_err(|_| ParseError::InvalidErrorCode(value.to_string()))?;

                Ok(Push::Alarm(code))
            }
            value if report_regex.is_match(value) => {
                let content = value.strip_prefix("<").unwrap().strip_suffix(">").unwrap();
                let parts: Vec<&str> = content.split("|").collect();

                let report = Report {
                    status: parts[0].to_string(),
                };

                Ok(Push::Report(report, value.to_string()))
            }
            value if feedback_regex.is_match(value) => {
                let content = value.strip_prefix("[").unwrap().strip_suffix("]").unwrap();
                let parts: Vec<&str> = content.split(":").collect();

                let feedback = Feedback {
                    kind: parts[0].to_string(),
                    data: parts[1].to_string(),
                };

                Ok(Push::Feedback(feedback, value.to_string()))
            }
            value => Err(ParseError::UnknownFormat(value.to_string())),
        }
    }
}
