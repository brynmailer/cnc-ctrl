mod config;
mod connection;
mod task;

use std::sync::atomic;
use std::{fs, sync, time};

use anyhow::{Context, Result, bail};
use log::{info, warn};
use rppal::gpio;

use config::{ConnectionKind, GeneralConfig, GpioConfig, JobConfig, LogsConfig, expand_path};
use connection::Connection;
use task::Task;

fn setup_logs(config: &LogsConfig) -> Result<()> {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();

    if let Some(dir) = &config.path {
        fs::create_dir_all(dir)?;

        let path = expand_path(dir.to_path_buf()).join(timestamp);
        let log_file = fs::File::create(&path)
            .with_context(|| format!("Failed to create log file {}", path.to_string_lossy()))?;

        simplelog::CombinedLogger::init(vec![
            simplelog::TermLogger::new(
                config.level,
                simplelog::Config::default(),
                simplelog::TerminalMode::Mixed,
                simplelog::ColorChoice::Auto,
            ),
            simplelog::WriteLogger::new(config.level, simplelog::Config::default(), log_file),
        ])?;
    } else {
        simplelog::TermLogger::init(
            config.level,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        )?;
    }

    Ok(())
}

/*
struct GpioInputs {
    signal: gpio::InputPin,
}

fn setup_gpio(config: &GpioConfig) -> Result<GpioInputs> {
    let gpio = gpio::Gpio::new()?;

    let signal = gpio.get(config.signal.pin)?.into_input_pullup();

    Ok(GpioInputs { signal })
}
*/

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        bail!("Usage: cnc-ctrl <path-to-job-config>".to_string());
    }

    let config = GeneralConfig::load().context("Failed to load config")?;

    let job_config_path = &args[1];
    let job_config = JobConfig::load(job_config_path)
        .with_context(|| format!("Failed to load job config from {}", job_config_path))?;

    setup_logs(&config.logs).context("Failed to setup logging")?;

    /*
    let mut gpio_inputs = setup_gpio(&config.gpio).context("Failed to setup GPIO pins")?;
    gpio_inputs
        .signal
        .set_interrupt(
            gpio::Trigger::RisingEdge,
            Some(time::Duration::from_millis(config.gpio.signal.debounce_ms)),
        )
        .context("Failed to set signal interrupt")?;
    */

    let connection = match job_config.connection.kind {
        ConnectionKind::Tcp(tcp_config) => Connection::new(&tcp_config)?.open()?,
        ConnectionKind::Serial(_) => unimplemented!(),
    };

    let running = sync::Arc::new(atomic::AtomicBool::new(true));

    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        warn!("Shutting down...");
        running_clone.store(false, atomic::Ordering::Relaxed);
    })?;

    while running.load(atomic::Ordering::Relaxed) {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();

        for (i, task_config) in job_config.tasks.iter().enumerate() {
            let task: Box<dyn Task> = task_config.into();

            if i == 0 || task_config.wait {
                info!("Waiting for signal to proceed...");
                /*
                gpio_inputs
                    .signal
                    .poll_interrupt(true, None)
                    .context("Failed to poll signal interrupt")?;
                */
            }

            info!("Executing task {} (timestamp: {})", i + 1, timestamp);

            let result = task.execute(&timestamp, running.clone(), &connection);

            match result {
                Ok(()) => info!("Task {} completed successfully", i + 1),
                Err(e) => {
                    bail!("Task {} failed: {}", i + 1, e);
                }
            }
        }

        info!("Job complete (timestamp: {})", timestamp);
    }

    Ok(())
}
