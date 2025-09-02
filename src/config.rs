use std::env;

use anyhow::Result;
use config::{Config, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct JobConfig {
    pub machine: MachineConfig,
    pub logs: LogsConfig,
    pub inputs: InputsConfig,
    pub steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
pub struct MachineConfig {
    pub connection: ConnectionConfig,
    pub rx_capacity: usize,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ConnectionConfig {
    #[serde(rename = "tcp")]
    TCP {
        address: String,
        port: u16,
        timeout: u64,
    },
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

impl JobConfig {
    pub fn load(config_path: &str) -> Result<Self> {
        let settings = Config::builder()
            .add_source(File::with_name(config_path))
            .build()?;

        let config: JobConfig = settings.try_deserialize()?;

        Ok(config)
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
