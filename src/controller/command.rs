use std::fmt;

pub enum Command {
    Gcode(String),
    Realtime(u8),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Gcode(gcode) => write!(f, "GCODE '{}'", gcode),
            Command::Realtime(byte) => write!(f, "REALTIME '{}'", byte),
        }
    }
}
