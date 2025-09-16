use std::fmt;

#[derive(Clone)]
pub enum Command {
    Realtime(Realtime),
    Block(String),
}

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum Realtime {
    Reset = 0x18,
    Stop = 0x19,
    Report = 0x80,
    CycleStart = 0x81,
    FeedHold = 0x82,
    ParserStateReport = 0x83,
    FullReport = 0x87,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Realtime(cmd) => write!(f, "RT {:#x}", *cmd as u8),
            Command::Block(cmd) => write!(f, "{}", cmd),
        }
    }
}

// TODO: Implement better parsing
impl From<String> for Command {
    fn from(value: String) -> Self {
        Command::Block(value)
    }
}
