mod bash;
mod gcode;

use anyhow::Result;

use crate::config::{JobConfig, Step};
use crate::connection::{ActiveConnection, Device};

use bash::execute_bash_step;
use gcode::execute_gcode_step;

impl Step {
    pub fn should_wait(&self) -> bool {
        match self {
            Step::Gcode(step) | Step::Bash(step) => step.wait_for_signal,
        }
    }

    pub fn execute<T: Device>(
        &self,
        timestamp: &str,
        connection: &ActiveConnection<T>,
        config: &JobConfig,
    ) -> Result<()> {
        match self {
            Step::Gcode(step) => execute_gcode_step(step, connection, timestamp),
            Step::Bash(step) => execute_bash_step(step, timestamp),
        }
    }
}
