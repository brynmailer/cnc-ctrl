use std::fs::File;
use std::io::{BufRead, BufReader};

use log::{error, info};

use crate::config::{apply_template, expand_path};
use crate::controller::command::Command;
use crate::controller::message::{Report, Response, Status};
use crate::controller::serial::{buffered_stream, wait_for_report};
use crate::controller::{Controller, ControllerError};

use super::GcodeStep;

pub fn execute_gcode_step(
    step: &GcodeStep,
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

    let mut output_file = if let Some(points_config) = &step.points {
        if points_config.save {
            let expanded_output = expand_path(&points_config.path);
            let templated_output = apply_template(&expanded_output, timestamp);

            if let Some(parent) = std::path::Path::new(&templated_output).parent() {
                std::fs::create_dir_all(parent)?;
            }

            Some(File::create(&templated_output).map_err(|error| {
                format!(
                    "Failed to create output file '{}': {}",
                    templated_output, error
                )
            })?)
        } else {
            None
        }
    } else {
        None
    };

    let gcode: Vec<&str> = gcode_lines.iter().map(|s| s.as_str()).collect();

    if step.check {
        info!("Checking G-code");

        let (serial_tx, _) = controller.serial.clone();

        serial_tx
            .send(Command::Gcode("$C".to_string()))
            .map_err(|error| format!("Failed to enable check mode: {}", error))?;

        let errors: Vec<(i32, Response)> =
            buffered_stream(controller, gcode.clone(), rx_buffer_size, None)
                .map_err(|error| format!("Failed to stream G-code in check mode: {}", error))?
                .iter()
                .filter_map(|res| {
                    if let Response::Error(_) = res.1 {
                        Some(*res)
                    } else {
                        None
                    }
                })
                .collect();

        serial_tx
            .send(Command::Gcode("$C".to_string()))
            .map_err(|error| format!("Failed to disable check mode: {}", error))?;

        if errors.len() > 0 {
            error!("Checking complete! {} errors found", errors.len());
            return Err(Box::new(ControllerError::GcodeError(errors)));
        } else {
            info!("Checking complete! No errors found");
        }
    }

    info!("Streaming G-code");

    buffered_stream(controller, gcode, rx_buffer_size, output_file.as_mut())
        .map_err(|error| format!("Failed to stream G-code: {}", error))?;

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
