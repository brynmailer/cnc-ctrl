use std::process::Command;

use log::info;

use crate::config::{apply_template, expand_path};

use super::BashStep;

pub fn execute_bash_step(
    step: &BashStep,
    timestamp: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let expanded_command = expand_path(&step.command);
    let templated_command = apply_template(&expanded_command, timestamp);

    let output = Command::new("sh")
        .arg("-c")
        .arg(&templated_command)
        .output()
        .map_err(|error| {
            format!(
                "Failed to execute command '{}': {}",
                templated_command, error
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Command failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        info!("Command output: {}", stdout.trim());
    }

    Ok(())
}
