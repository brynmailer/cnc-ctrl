use std::fmt;

pub enum Command {
    Gcode(String, Option<usize>),
    System(String, Option<usize>),
    Realtime(u8),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Gcode(block, None) | Command::System(block, None) => write!(f, "{}", block),
            Command::Gcode(block, Some(line)) | Command::System(block, Some(line)) => {
                write!(f, "{}: {}", line, block)
            }
            Command::Realtime(byte) => write!(f, "{:#x}", byte),
        }
    }
}
