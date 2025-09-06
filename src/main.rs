mod config;
mod connection;
mod steps;

use std::fs::{self, File};
use std::sync::{self, atomic};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::Local;
use log::{LevelFilter, info, warn};
use rppal::gpio::{Gpio, InputPin, Trigger};
use simplelog::*;

use config::{ConnectionConfig, JobConfig, apply_template, expand_path};
use connection::Connection;

struct GpioInputs {
    signal: InputPin,
}

fn setup_logging(config: &JobConfig) -> Result<()> {
    let log_level = if config.logging.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    if config.logging.save {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();

        let expanded_path = expand_path(&config.logging.path);
        let templated_path = apply_template(&expanded_path, &timestamp);

        if let Some(parent) = std::path::Path::new(&templated_path).parent() {
            fs::create_dir_all(parent)?;
        }

        let log_file = File::create(&templated_path)
            .with_context(|| format!("Failed to create log file {}", templated_path))?;

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

fn setup_gpio(config: &JobConfig) -> Result<GpioInputs> {
    let gpio = Gpio::new()?;

    let signal = gpio.get(config.inputs.signal.pin)?.into_input_pullup();

    Ok(GpioInputs { signal })
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        bail!("Usage: cnc-ctrl <path-to-job-config>".to_string());
    }

    let config_path = &args[1];
    let config = JobConfig::load(config_path)
        .with_context(|| format!("Failed to load job configuration from {}", config_path))?;

    setup_logging(&config).context("Failed to setup logging")?;

    let connection = match config.connection {
        ConnectionConfig::Tcp(tcp_config) => Connection::tcp(&tcp_config)?,
        ConnectionConfig::Serial(serial_config) => unimplemented!(),
    };

    let mut gpio_inputs = setup_gpio(&config).context("Failed to setup GPIO pins")?;
    gpio_inputs
        .signal
        .set_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(config.inputs.signal.debounce_ms)),
        )
        .context("Failed to set signal interrupt")?;

    let alive = sync::Arc::new(atomic::AtomicBool::new(true));

    let alive_clone = alive.clone();
    ctrlc::set_handler(move || {
        warn!("Shutting down gracefully...");
        alive_clone.store(false, atomic::Ordering::Relaxed);
    })?;

    while alive.load(atomic::Ordering::Relaxed) {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();

        for (i, step) in config.steps.iter().enumerate() {
            if i == 0 || step.should_wait() {
                info!("Waiting for start signal...");
                gpio_inputs
                    .signal
                    .poll_interrupt(true, None)
                    .context("Failed to poll signal interrupt")?;
            }

            info!("Executing step {} (timestamp: {})", i + 1, timestamp);

            let result = step.execute(&timestamp, &connection, &config);

            match result {
                Ok(()) => info!("Step {} completed successfully", i + 1),
                Err(e) => {
                    bail!("Step {} failed: {}", i + 1, e);
                }
            }
        }

        info!("Sequence complete (timestamp: {})", timestamp);
    }

    Ok(())
}
