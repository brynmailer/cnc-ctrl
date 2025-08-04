mod config;
mod controller;

use log::{error, info};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process::Command as ProcessCommand;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rppal::gpio::{Gpio, InputPin, Trigger};

use config::{CncConfig, Step, apply_template, expand_path};
use controller::Controller;
use controller::command::Command;
use controller::message::{Report, Status};
use controller::serial::{buffered_stream, wait_for_report};

use crate::controller::ControllerError;
use crate::controller::message::Response;

struct GpioInputs {
    signal: InputPin,
    probe_xy: InputPin,
    probe_z: InputPin,
}

fn setup_gpio(config: &CncConfig) -> Result<GpioInputs, Box<dyn std::error::Error>> {
    let gpio = Gpio::new()?;

    let signal = gpio.get(config.inputs.signal.pin)?.into_input_pullup();
    let probe_xy = gpio
        .get(config.inputs.probe_xy_limit.pin)?
        .into_input_pullup();
    let probe_z = gpio
        .get(config.inputs.probe_z_limit.pin)?
        .into_input_pullup();

    Ok(GpioInputs {
        signal,
        probe_xy,
        probe_z,
    })
}

fn execute_gcode_step(
    step: &config::GcodeStep,
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

        if let Some((serial_tx, _)) = controller.serial_channel.clone() {
            serial_tx
                .send(Command::Gcode("$C".to_string()))
                .map_err(|error| format!("Failed to enable check mode: {}", error))?;
        }

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

        if let Some((serial_tx, _)) = controller.serial_channel.clone() {
            serial_tx
                .send(Command::Gcode("$C".to_string()))
                .map_err(|error| format!("Failed to disable check mode: {}", error))?;
        }

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

fn execute_bash_step(
    step: &config::BashStep,
    timestamp: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let expanded_command = expand_path(&step.command);
    let templated_command = apply_template(&expanded_command, timestamp);

    let output = ProcessCommand::new("sh")
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

fn main() -> Result<(), String> {
    env_logger::init();

    let config =
        CncConfig::load().map_err(|error| format!("Failed to load configuration: {}", error))?;

    let serial = serialport::new(&config.serial.port, config.serial.baudrate)
        .timeout(Duration::from_millis(config.serial.timeout_ms))
        .open()
        .map_err(|error| format!("Failed to open serial connection: {}", error))?;
    let mut serial_clone = serial
        .try_clone()
        .map_err(|error| format!("Failed to clone serial connection: {}", error))?;

    let mut controller = Controller::new();
    let controller_running = controller.running.clone();
    controller.start(serial, config.logs.verbose);

    ctrlc::set_handler(move || {
        info!("Shutting down...");
        controller_running.store(false, Ordering::Relaxed);
        thread::sleep(Duration::from_secs(2));
        if let Err(error) = serial_clone.write_all(&[0x18]) {
            error!("Failed to soft reset Grbl: {}", error);
        }
    })
    .map_err(|error| format!("Failed to set up exit handler: {}", error))?;

    let mut gpio_inputs =
        setup_gpio(&config).map_err(|error| format!("Failed to setup GPIO pins: {}", error))?;

    let Some((prio_serial_tx, _)) = controller.prio_serial_channel.clone() else {
        return Err("Failed to clone serial tx: Controller not started".to_string());
    };

    let prio_serial_tx_xy = prio_serial_tx.clone();
    gpio_inputs
        .probe_xy
        .set_async_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(
                config.inputs.probe_xy_limit.debounce_ms,
            )),
            move |_| {
                if let Err(error) = prio_serial_tx_xy.send(Command::Realtime(0x85)) {
                    error!("Failed to send XY probe interrupt signal: {}", error);
                }
            },
        )
        .map_err(|error| format!("Failed to set probe XY interrupt: {}", error))?;

    let prio_serial_tx_z = prio_serial_tx.clone();
    gpio_inputs
        .probe_z
        .set_async_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(
                config.inputs.probe_z_limit.debounce_ms,
            )),
            move |_| {
                if let Err(error) = prio_serial_tx_z.send(Command::Realtime(0x85)) {
                    error!("Failed to send Z probe interrupt signal: {}", error);
                }
            },
        )
        .map_err(|error| format!("Failed to set probe Z interrupt: {}", error))?;

    gpio_inputs
        .signal
        .set_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(config.inputs.signal.debounce_ms)),
        )
        .map_err(|error| format!("Failed to set signal interrupt: {}", error))?;

    while controller.running.load(Ordering::Relaxed) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("Time went backwards: {}", error))?
            .as_secs()
            .to_string();

        for (i, step) in config.steps.iter().enumerate() {
            if step.wait_for_signal(i == 0) {
                info!("Waiting for start signal...");
                gpio_inputs
                    .signal
                    .poll_interrupt(true, None)
                    .map_err(|error| format!("Failed to poll signal interrupt: {}", error))?;
            }

            info!("Executing step {} (timestamp: {})", i + 1, timestamp);

            let result = match step {
                Step::Gcode(gcode_step) => execute_gcode_step(
                    gcode_step,
                    &controller,
                    &timestamp,
                    config.grbl.rx_buffer_size_bytes,
                ),
                Step::Bash(bash_step) => execute_bash_step(bash_step, &timestamp),
            };

            match result {
                Ok(()) => info!("Step {} completed successfully", i + 1),
                Err(e) => {
                    return Err(format!("Step {} failed: {}", i + 1, e));
                }
            }
        }

        info!("Sequence complete (timestamp: {})", timestamp);
    }

    Ok(())
}
