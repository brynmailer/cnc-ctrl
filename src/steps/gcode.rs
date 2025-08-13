use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};

use log::{error, info};

use crate::config::{GcodeStepConfig, ProbeConfig, apply_template, expand_path};
use crate::controller::command::Command;
use crate::controller::message::{Report, Response, Status};
use crate::controller::serial::{buffered_stream, wait_for_report};
use crate::controller::{Controller, ControllerError};

pub fn execute_gcode_step(
    step: &GcodeStepConfig,
    controller: &Controller,
    timestamp: &str,
    rx_buffer_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let expanded_path = expand_path(&step.path);
    let templated_path = apply_template(&expanded_path, timestamp);

    let file = File::open(&templated_path)
        .map_err(|error| format!("Failed to open G-code file '{}': {}", templated_path, error))?;
    let reader = BufReader::new(file);

    let gcode_lines: Vec<String> = reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to read G-code file: {}", error))?;

    let gcode: Vec<&str> = gcode_lines.iter().map(|s| s.as_str()).collect();

    let output_writer = if let Some(ProbeConfig {
        save_path: Some(save_path),
    }) = &step.probe
    {
        let expanded_output = expand_path(&save_path);
        let templated_output = apply_template(&expanded_output, timestamp);

        if let Some(parent) = std::path::Path::new(&templated_output).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = File::create(&templated_output).map_err(|error| {
            format!(
                "Failed to create output file '{}': {}",
                templated_output, error
            )
        })?;

        Some(BufWriter::new(file))
    } else {
        None
    };

    if step.check {
        info!("Checking G-code");

        if let Some((serial_tx, _)) = controller.serial_channel.clone() {
            serial_tx
                .send(Command::Gcode("$C".to_string()))
                .map_err(|error| format!("Failed to enable check mode: {}", error))?;
        }

        let errors: Vec<ControllerError> =
            buffered_stream(controller, gcode.clone(), rx_buffer_size)
                .map_err(|error| format!("Failed to stream G-code in check mode: {}", error))?
                .iter()
                .filter_map(|res| {
                    if let Response::Error(_) = res.1 {
                        Some(ControllerError::GcodeError(res.0, res.1.clone()))
                    } else {
                        None
                    }
                })
                .collect();

        if let Some((serial_tx, _)) = controller.serial_channel.clone() {
            serial_tx
                .send(Command::Gcode("$C".to_string()))
                .map_err(|error| format!("Failed to disable check mode: {}", error))?;
        }

        if errors.len() > 0 {
            error!(
                "Checking complete! {} errors found:\n
                 {}\n",
                errors.len(),
                errors.iter().fold(String::new(), |res, err| format!(
                    "{}\n                {}",
                    res, err
                )),
            );
            info!("Skipping streaming");

            return Ok(());
        } else {
            info!("Checking complete! No errors found");
        }
    }

    info!("Streaming G-code");

    let responses = buffered_stream(controller, gcode, rx_buffer_size)
        .map_err(|error| format!("Failed to stream G-code: {}", error))?;

    if let Some(mut writer) = output_writer {
        responses
            .iter()
            .try_for_each(|res| -> std::io::Result<()> {
                if let Response::Probe { coords, .. } = res.1 {
                    writeln!(writer, "{},{},{}", coords.0, coords.1, coords.2)?;
                }

                Ok(())
            })?;
    }

    wait_for_report(
        &controller,
        Some(|report: &Report| {
            matches!(
                report,
                &Report {
                    status: Some(Status::Idle),
                    ..
                }
            )
        }),
    )?;

    info!("Streaming complete");

    Ok(())
}
