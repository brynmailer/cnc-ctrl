mod bash;
mod gcode;

use serde::Deserialize;

use super::config::{CncConfig, PointsConfig};
use super::controller::Controller;

use bash::execute_bash_step;
use gcode::execute_gcode_step;

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
    pub points: Option<PointsConfig>,
    #[serde(default = "default_wait_for_signal")]
    pub wait_for_signal: bool,
    #[serde(default = "default_check")]
    pub check: bool,
}

#[derive(Debug, Deserialize)]
pub struct BashStep {
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

impl Step {
    pub fn should_wait(&self) -> bool {
        match self {
            Step::Gcode(step) => step.wait_for_signal,
            Step::Bash(step) => step.wait_for_signal,
        }
    }

    pub fn execute(
        &self,
        controller: &Controller,
        timestamp: &str,
        config: &CncConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Step::Gcode(step) => execute_gcode_step(
                step,
                controller,
                timestamp,
                config.grbl.rx_buffer_size_bytes,
            ),
            Step::Bash(step) => execute_bash_step(step, timestamp),
        }
    }
}
