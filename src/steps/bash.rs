use std::process::Command;

use anyhow::{Context, Result, bail};
use log::info;

use crate::config::{BashStepConfig, apply_template, expand_path};

pub fn execute_bash_step(step: &BashStepConfig, timestamp: &str) -> Result<()> {
    let expanded_command = expand_path(&step.command);
    let templated_command = apply_template(&expanded_command, timestamp);

    let output = Command::new("sh")
        .arg("-c")
        .arg(&templated_command)
        .output()
        .with_context(|| format!("Failed to execute command '{}'", templated_command))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Command failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        info!("Command output: {}", stdout.trim());
    }

    Ok(())
}
