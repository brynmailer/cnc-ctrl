use std::env;

use config::{Config, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CncConfig {
    pub serial: SerialConfig,
    pub grbl: GrblConfig,
    pub inputs: InputsConfig,
    pub steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
pub struct SerialConfig {
    pub port: String,
    pub baudrate: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct GrblConfig {
    pub rx_buffer_size_bytes: usize,
}

#[derive(Debug, Deserialize)]
pub struct InputsConfig {
    pub signal: InputPin,
    pub probe_xy_limit: InputPin,
    pub probe_z_limit: InputPin,
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
    Gcode(GcodeStep),
    #[serde(rename = "bash")]
    Bash(BashStep),
}

#[derive(Debug, Deserialize)]
pub struct GcodeStep {
    pub path: String,
    #[serde(default)]
    pub probe: bool,
    pub points: Option<PointsConfig>,
    #[serde(default = "default_wait_for_signal")]
    pub wait_for_signal: bool,
}

#[derive(Debug, Deserialize)]
pub struct BashStep {
    pub command: String,
    #[serde(default)]
    pub wait_for_signal: bool,
}

#[derive(Debug, Deserialize)]
pub struct PointsConfig {
    #[serde(default)]
    pub save: bool,
    pub path: String,
}

fn default_wait_for_signal() -> bool {
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

impl Step {
    pub fn wait_for_signal(&self, is_first: bool) -> bool {
        match self {
            Step::Gcode(step) => {
                if is_first {
                    true
                } else {
                    step.wait_for_signal
                }
            }
            Step::Bash(step) => {
                if is_first {
                    true
                } else {
                    step.wait_for_signal
                }
            }
        }
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
