mod config;
mod controller;

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
        .map_err(|e| format!("Failed to open G-code file '{}': {}", templated_path, e))?;
    let reader = BufReader::new(file);

    let gcode_lines: Vec<String> = reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read G-code file: {}", e))?;

    let mut output_file = if let Some(points_config) = &step.points {
        if points_config.save {
            let expanded_output = expand_path(&points_config.path);
            let templated_output = apply_template(&expanded_output, timestamp);

            if let Some(parent) = std::path::Path::new(&templated_output).parent() {
                std::fs::create_dir_all(parent)?;
            }

            Some(File::create(&templated_output).map_err(|e| {
                format!("Failed to create output file '{}': {}", templated_output, e)
            })?)
        } else {
            None
        }
    } else {
        None
    };

    let gcode: Vec<&str> = gcode_lines.iter().map(|s| s.as_str()).collect();

    if step.check {
        println!("Checking G-code");

        if let Some((serial_tx, _)) = controller.serial_channel.clone() {
            serial_tx
                .send(Command::Gcode("$C".to_string()))
                .map_err(|e| format!("Failed to enable check mode: {}", e))?;
        }

        let errors: Vec<(i32, Response)> =
            buffered_stream(controller, gcode.clone(), rx_buffer_size, None)
                .map_err(|e| format!("Failed to stream G-code in check mode: {}", e))?
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
                .map_err(|e| format!("Failed to disable check mode: {}", e))?;
        }

        if errors.len() > 0 {
            eprintln!("Checking complete! {} errors found", errors.len());
            return Err(Box::new(ControllerError::GcodeError(errors)));
        } else {
            println!("Checking complete! No errors found");
        }
    }

    println!("Streaming G-code");

    buffered_stream(controller, gcode, rx_buffer_size, output_file.as_mut())
        .map_err(|e| format!("Failed to stream G-code: {}", e))?;

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
    );

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
        .map_err(|e| format!("Failed to execute command '{}': {}", templated_command, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Command failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        println!("Command output: {}", stdout.trim());
    }

    Ok(())
}

fn main() {
    let config = CncConfig::load().expect("Failed to load configuration");

    let serial = serialport::new(&config.serial.port, config.serial.baudrate)
        .timeout(Duration::from_millis(config.serial.timeout_ms))
        .open()
        .expect("Failed to open serial connection");
    let mut serial_clone = serial
        .try_clone()
        .expect("Failed to clone serial connection");

    let mut controller = Controller::new();
    let controller_running = controller.running.clone();
    controller.start(serial);

    ctrlc::set_handler(move || {
        println!("Shutting down...");
        controller_running.store(false, Ordering::Relaxed);
        thread::sleep(Duration::from_secs(2));
        serial_clone
            .write_all(&[0x18])
            .expect("Failed to soft reset Grbl");
    })
    .expect("Failed to set up exit handler");

    let mut gpio_inputs = setup_gpio(&config).expect("Failed to setup GPIO");

    let Some((prio_serial_tx, _)) = controller.prio_serial_channel.clone() else {
        panic!("Failed to init gpio: Controller not started");
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
                prio_serial_tx_xy
                    .send(Command::Realtime(0x85))
                    .expect("Failed to send XY probe interrupt");
            },
        )
        .expect("Failed to initialize probe XY interrupt");

    let prio_serial_tx_z = prio_serial_tx.clone();
    gpio_inputs
        .probe_z
        .set_async_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(
                config.inputs.probe_z_limit.debounce_ms,
            )),
            move |_| {
                prio_serial_tx_z
                    .send(Command::Realtime(0x85))
                    .expect("Failed to send Z probe interrupt");
            },
        )
        .expect("Failed to initialize probe Z interrupt");

    while controller.running.load(Ordering::Relaxed) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            .to_string();

        for (i, step) in config.steps.iter().enumerate() {
            if step.wait_for_signal(i == 0) {
                gpio_inputs
                    .signal
                    .set_interrupt(
                        Trigger::RisingEdge,
                        Some(Duration::from_millis(config.inputs.signal.debounce_ms)),
                    )
                    .expect("Failed to configure signal interrupt");

                println!("Waiting for start signal...");
                gpio_inputs
                    .signal
                    .poll_interrupt(true, None)
                    .expect("Failed to poll signal interrupt");
            }

            println!("Executing step {} (timestamp: {})", i + 1, timestamp);

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
                Ok(()) => println!("Step {} completed successfully", i + 1),
                Err(e) => {
                    eprintln!("Step {} failed: {}", i + 1, e);
                    eprintln!("Continuing to next step...");
                }
            }
        }

        println!("Sequence complete (timestamp: {})", timestamp);
    }
}
