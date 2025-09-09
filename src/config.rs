use std::{env, net::SocketAddr};

use config::{Config, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CncConfig {
    pub logs: LogsConfig,
    pub connection: TcpConfig,
    pub grbl: GrblConfig,
    pub inputs: InputsConfig,
    pub steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
pub struct LogsConfig {
    pub verbose: bool,
    pub save: bool,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct TcpConfig {
    pub address: SocketAddr,
    pub timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct GrblConfig {
    pub rx_buffer_size_bytes: usize,
}

#[derive(Debug, Deserialize)]
pub struct InputsConfig {
    pub signal: InputPin,
}

#[derive(Debug, Deserialize)]
pub struct InputPin {
    pub pin: u8,
    pub debounce_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Step {
    #[serde(rename = "gcode")]
    Gcode(GcodeStepConfig),
    #[serde(rename = "bash")]
    Bash(BashStepConfig),
}

#[derive(Debug, Deserialize)]
pub struct GcodeStepConfig {
    pub path: String,
    pub probe: Option<ProbeConfig>,
    #[serde(default = "default_wait_for_signal")]
    pub wait_for_signal: bool,
    #[serde(default = "default_check")]
    pub check: bool,
}

#[derive(Debug, Deserialize)]
pub struct ProbeConfig {
    pub save_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BashStepConfig {
    pub command: String,
    #[serde(default)]
    pub wait_for_signal: bool,
}

fn default_wait_for_signal() -> bool {
    true
}

fn default_check() -> bool {
    true
}

impl CncConfig {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path()?;
        let settings = Config::builder()
            .add_source(File::with_name(&config_path))
            .build()?;

        let config: CncConfig = settings.try_deserialize()?;

        Ok(config)
    }

    fn get_config_path() -> Result<String, Box<dyn std::error::Error>> {
        let home_dir = env::home_dir().ok_or("Failed to get home directory")?;
        let config_path = home_dir.join(".config").join("cnc-ctrl").join("config.yml");

        Ok(config_path.to_string_lossy().to_string())
    }
}

pub fn expand_path(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home_dir) = env::home_dir() {
            let home_str = home_dir.to_string_lossy();
            return path.replacen('~', &home_str, 1);
        }
    }
    path.to_string()
}

pub fn apply_template(text: &str, timestamp: &str) -> String {
    text.replace("{%t}", timestamp)
}
