use std::path;

use anyhow::{Context, Result};
use config::{Config, File};
use serde::Deserialize;

/* General */

#[derive(Debug, Deserialize)]
pub struct GeneralConfig {
    pub logs: LogsConfig,
    pub gpio: GpioConfig,
}

#[derive(Debug, Deserialize)]
pub struct LogsConfig {
    pub path: Option<path::PathBuf>,
    pub level: log::LevelFilter,
}

#[derive(Debug, Deserialize)]
pub struct GpioConfig {
    pub signal: PinConfig,
}

#[derive(Debug, Deserialize)]
pub struct PinConfig {
    pub pin: u8,
    pub debounce_ms: u64,
}

impl GeneralConfig {
    pub fn load() -> Result<Self> {
        let path = dirs::config_dir()
            .map(|dir| dir.join("cnc-ctrl").join("config.yml"))
            .context("Failed to determine config directory")?;

        if !path.exists() {
            return Ok(GeneralConfig::default());
        }

        let file = Config::builder().add_source(File::from(path)).build()?;

        Ok(file.try_deserialize()?)
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        let log_path = dirs::data_dir()
            .map(|dir| dir.join("cnc-ctrl").join("config.yml"))
            .or_else(|| dirs::home_dir());

        Self {
            logs: LogsConfig {
                path: log_path,
                level: log::LevelFilter::Info,
            },
            gpio: GpioConfig {
                signal: PinConfig {
                    pin: 17,
                    debounce_ms: 30,
                },
            },
        }
    }
}

/* Per job */

#[derive(Debug, Deserialize)]
pub struct JobConfig {
    pub connection: ConnectionConfig,
    pub tasks: Vec<TaskConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ConnectionConfig {
    #[serde(flatten)]
    pub kind: ConnectionKind,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ConnectionKind {
    Tcp(TcpConfig),
    Serial(SerialConfig),
}

#[derive(Debug, Deserialize)]
pub struct TcpConfig {
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct SerialConfig {
    pub port: String,
    pub baud_rate: u32,
}

#[derive(Debug, Deserialize)]
pub struct TaskConfig {
    #[serde(flatten)]
    pub kind: TaskKind,
    pub wait: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TaskKind {
    Stream(StreamConfig),
    Process(ProcessConfig),
}

#[derive(Debug, Deserialize)]
pub struct StreamConfig {
    pub path: path::PathBuf,
    pub check: bool,
    pub output: Option<OutputConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ProcessConfig {
    pub command: String,
}

#[derive(Debug, Deserialize)]
pub struct OutputConfig {
    pub kind: OutputKind,
    pub path: path::PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum OutputKind {
    #[serde(rename = "points")]
    ProbedPoints,
}

impl JobConfig {
    pub fn load(path: &str) -> Result<Self> {
        let file = Config::builder()
            .add_source(File::with_name(path))
            .build()?;

        let config: JobConfig = file.try_deserialize()?;

        Ok(config)
    }
}

pub fn expand_path(path: path::PathBuf) -> path::PathBuf {
    if path.starts_with("~/") {
        if let Some(expanded) = dirs::home_dir()
            .map(|home| home.join(path.components().skip(1).collect::<path::PathBuf>()))
        {
            return expanded;
        }
    }

    path
}

pub fn apply_template(text: &str, timestamp: &str) -> String {
    text.replace("{%t}", timestamp)
}
