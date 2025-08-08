mod config;
mod controller;
mod steps;

use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use chrono::Local;
use log::{LevelFilter, error, info, warn};
use rppal::gpio::{Gpio, InputPin, Trigger};
use simplelog::*;

use config::{CncConfig, apply_template, expand_path};
use controller::Controller;
use controller::command::Command;

struct GpioInputs {
    signal: InputPin,
    probe_xy: InputPin,
    probe_z: InputPin,
}

fn setup_gpio(config: &CncConfig) -> Result<GpioInputs, Box<dyn std::error::Error>> {
    let gpio = Gpio::new()?;

    let signal = gpio.get(config.inputs.signal.pin)?.into_input_pullup();
    let probe_xy = gpio.get(config.inputs.probe_xy.pin)?.into_input_pullup();
    let probe_z = gpio.get(config.inputs.probe_z.pin)?.into_input_pullup();

    Ok(GpioInputs {
        signal,
        probe_xy,
        probe_z,
    })
}

fn setup_logging(config: &CncConfig) -> Result<(), Box<dyn std::error::Error>> {
    let log_level = if config.logs.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    if config.logs.save {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();

        let expanded_path = expand_path(&config.logs.path);
        let templated_path = apply_template(&expanded_path, &timestamp);

        if let Some(parent) = std::path::Path::new(&templated_path).parent() {
            fs::create_dir_all(parent)?;
        }

        let log_file = File::create(&templated_path)
            .map_err(|e| format!("Failed to create log file '{}': {}", templated_path, e))?;

        CombinedLogger::init(vec![
            TermLogger::new(
                log_level,
                Config::default(),
                TerminalMode::Mixed,
                ColorChoice::Auto,
            ),
            WriteLogger::new(log_level, Config::default(), log_file),
        ])?;
    } else {
        TermLogger::init(
            log_level,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        )?;
    }

    Ok(())
}

fn main() -> Result<(), String> {
    let config =
        CncConfig::load().map_err(|error| format!("Failed to load configuration: {}", error))?;

    setup_logging(&config).map_err(|error| format!("Failed to setup logging: {}", error))?;

    let config =
        CncConfig::load().map_err(|error| format!("Failed to load configuration: {}", error))?;

    let serial = serialport::new(&config.serial.port, config.serial.baudrate)
        .timeout(Duration::from_millis(config.serial.timeout_ms))
        .open()
        .map_err(|error| format!("Failed to open serial connection: {}", error))?;

    let mut controller = Controller::new();

    let mut serial_clone = serial
        .try_clone()
        .map_err(|error| format!("Failed to clone serial connection: {}", error))?;
    let controller_running = controller.running.clone();
    ctrlc::set_handler(move || {
        warn!("Shutting down...");

        controller_running.store(false, Ordering::Relaxed);
        thread::sleep(Duration::from_secs(2));

        if let Err(error) = serial_clone.write_all(&[0x18]) {
            error!("Failed to soft reset Grbl: {}", error);
        }
    })
    .map_err(|error| format!("Failed to set up exit handler: {}", error))?;

    let mut gpio_inputs =
        setup_gpio(&config).map_err(|error| format!("Failed to setup GPIO pins: {}", error))?;

    let prio_serial_tx_xy = controller.prio_serial.0.clone();
    gpio_inputs
        .probe_xy
        .set_async_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(config.inputs.probe_xy.debounce_ms)),
            move |_| {
                if let Err(error) = prio_serial_tx_xy.send(Command::Realtime(0x85)) {
                    error!("Failed to send XY probe interrupt signal: {}", error);
                }
            },
        )
        .map_err(|error| format!("Failed to set probe XY interrupt: {}", error))?;

    let prio_serial_tx_z = controller.prio_serial.0.clone();
    gpio_inputs
        .probe_z
        .set_async_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(config.inputs.probe_z.debounce_ms)),
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
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();

        for (i, step) in config.steps.iter().enumerate() {
            if i == 0 || step.should_wait() {
                info!("Waiting for start signal...");
                gpio_inputs
                    .signal
                    .poll_interrupt(true, None)
                    .map_err(|error| format!("Failed to poll signal interrupt: {}", error))?;
            }

            info!("Executing step {} (timestamp: {})", i + 1, timestamp);

            controller.connect(
                serial
                    .try_clone()
                    .map_err(|error| format!("Failed to start controller: {}", error))?,
                config.logs.verbose,
            );

            let result = step.execute(&controller, &timestamp, &config);

            controller.disconnect();

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
