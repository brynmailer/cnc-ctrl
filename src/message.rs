use regex::Regex;

use crate::controller::ControllerError;

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

pub struct Report {
    pub status: Option<String>,
    pub mpos: Option<(f32, f32, f32)>,
    pub bf: Option<(usize, usize)>,
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
