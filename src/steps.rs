mod bash;
mod gcode;

use super::config::{JobConfig, Step};
use super::controller::Machine;

use bash::execute_bash_step;
use gcode::execute_gcode_step;

impl Step {
    pub fn should_wait(&self) -> bool {
        match self {
            Step::Gcode(step) => step.wait_for_signal,
            Step::Bash(step) => step.wait_for_signal,
        }
    }

    pub fn execute(
        &self,
        controller: &Machine,
        timestamp: &str,
        config: &JobConfig,
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
