mod bash;
mod gcode;

use anyhow::Result;

use crate::config::{JobConfig, Step};
use crate::machine::Machine;
use bash::execute_bash_step;
use gcode::execute_gcode_step;

impl Step {
    pub fn should_wait(&self) -> bool {
        match self {
            Step::Gcode(step) => step.wait_for_signal,
            Step::Bash(step) => step.wait_for_signal,
        }
    }

    pub fn execute(&self, timestamp: &str, machine: &Machine, config: &JobConfig) -> Result<()> {
        match self {
            Step::Gcode(step) => execute_gcode_step(step, machine, timestamp),
            Step::Bash(step) => execute_bash_step(step, timestamp),
        }
    }
}
