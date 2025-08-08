use std::env;

use config::{Config, File};
use serde::Deserialize;

use super::steps::Step;

#[derive(Debug, Deserialize)]
pub struct CncConfig {
    pub logs: LogsConfig,
    pub serial: SerialConfig,
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
pub struct SerialConfig {
    pub port: String,
    pub baudrate: u32,
}

#[derive(Debug, Deserialize)]
pub struct GrblConfig {
    pub rx_buffer_size_bytes: usize,
}

#[derive(Debug, Deserialize)]
pub struct InputsConfig {
    pub signal: InputPin,
    pub probe_xy: InputPin,
    pub probe_z: InputPin,
}

#[derive(Debug, Deserialize)]
pub struct InputPin {
    pub pin: u8,
    pub debounce_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct PointsConfig {
    #[serde(default)]
    pub save: bool,
    pub path: String,
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
