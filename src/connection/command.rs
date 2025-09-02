use std::fmt;

pub enum Command {
    Realtime(Realtime),
    Block(String, Option<usize>),
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
            Command::Realtime(command) => write!(f, "{:#x}", *command as u8),
            Command::Block(command, _) => write!(f, "{}", command),
        }
    }
}
