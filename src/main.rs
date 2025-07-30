mod command;
mod controller;
mod message;

use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use regex::Regex;
use rppal::gpio::{self, Gpio, Trigger};

use command::Command;
use controller::Controller;
use message::{Push, Report, Status};

// Serial
const PORT: &str = "/dev/ttyUSB0";
const BAUDRATE: u32 = 115200;
const TIMEOUT_MS: u64 = 60000;

// GPIO
const BUTTON_PIN: u8 = 22;
const PROBE_PIN: u8 = 27;

// GbrlHAL
const RX_BUFFER_SIZE: usize = 1024;

fn init_gpio(controller: &Controller) -> (gpio::InputPin, gpio::InputPin) {
    let gpio = Gpio::new().expect("Failed to intialize GPIO");
    let mut button = gpio
        .get(BUTTON_PIN)
        .expect("Failed to initialize button")
        .into_input_pullup();
    let mut probe = gpio
        .get(PROBE_PIN)
        .expect("Failed to initialize probe")
        .into_input_pullup();

    let Some((serial_tx, _)) = controller.prio_serial_channel.clone() else {
        panic!("Failed to init gpio: Controller not started");
    };

    button
        .set_interrupt(Trigger::RisingEdge, Some(Duration::from_millis(30)))
        .expect("Failed to initialize probe interrupt");
    probe
        .set_async_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(30)),
            move |_| {
                println!("Interrupting");
                serial_tx
                    .send(Command::Realtime(0x85))
                    .expect("Failed to send interrupt command");
            },
        )
        .expect("Failed to initialize probe interrupt");

    (button, probe)
}

fn wait_for_report<F: Fn(&Report) -> bool>(
    controller: &Controller,
    predicate: Option<F>,
) -> Option<Report> {
    let Some((prio_serial_tx, prio_serial_rx)) = controller.prio_serial_channel.clone() else {
        panic!("Failed to clone serial: Controller not started");
    };

    let polling = Arc::new(AtomicBool::new(true));
    let running = controller.running.clone();

    thread::scope(|scope| {
        scope.spawn(|| {
            while polling.load(Ordering::Relaxed) {
                prio_serial_tx
                    .send(Command::Realtime(b'?'))
                    .expect("Failed to poll grbl status report");

                thread::sleep(Duration::from_millis(200));
            }
        });

        loop {
            if !running.load(Ordering::Relaxed) {
                return None;
            }

            match prio_serial_rx.recv() {
                Ok(Push::Report(report)) => {
                    if let Some(matcher) = &predicate {
                        if !matcher(&report) {
                            continue;
                        }
                    }

                    polling.store(false, Ordering::Relaxed);
                    return Some(report);
                }
                Err(err) => panic!("Failed to wait for interrupt: {}", err),
            }
        }
    })
}

fn buffered_stream(
    controller: &Controller,
    gcode: Vec<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some((serial_tx, serial_rx)) = controller.serial_channel.clone() else {
        panic!("Failed to stream gcode: Controller not started");
    };

    let re = Regex::new(r"^\$J=.* IN$")?;
    let mut bytes_queued = VecDeque::new();
    let mut received_count = 0;
    let mut sent_count = 0;

    for raw_line in gcode {
        let interruptible = re.is_match(raw_line);
        let line = if interruptible {
            raw_line.trim().strip_suffix(" IN").unwrap()
        } else {
            raw_line.trim()
        };

        bytes_queued.push_back(line.len());

        while bytes_queued.iter().sum::<usize>() >= RX_BUFFER_SIZE {
            serial_rx.recv()?;
            received_count += 1;
            bytes_queued.pop_front();
        }

        serial_tx.send(Command::Gcode(line.to_string()))?;
        sent_count += 1;

        if interruptible {
            if let Some(report) = wait_for_report(
                controller,
                Some(|report: &Report| {
                    matches!(
                        report,
                        &Report {
                            status: Some(Status::Idle),
                            mpos: Some(_),
                            ..
                        }
                    )
                }),
            ) {
                let unwrapped_mpos = report.mpos.unwrap();
                println!(
                    "X{} Y{} Z{}",
                    unwrapped_mpos.0, unwrapped_mpos.1, unwrapped_mpos.2
                );
            }
        }
    }

    while sent_count > received_count {
        serial_rx.recv()?;
        received_count += 1;
        bytes_queued.pop_front();
    }

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <gcode_file>", args[0]);
        std::process::exit(1);
    }

    let gcode_file_path = &args[1];
    let file = File::open(gcode_file_path).expect("Failed to open gcode file");
    let reader = BufReader::new(file);

    let gcode_lines: Vec<String> = reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to read gcode file");

    let serial = serialport::new(PORT, BAUDRATE)
        .timeout(Duration::from_millis(TIMEOUT_MS))
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

    let (mut button, _) = init_gpio(&controller);

    while controller.running.load(Ordering::Relaxed) {
        println!("Waiting for start signal...");
        button
            .poll_interrupt(true, None)
            .expect("Failed to poll button interrupt");

        println!("Beginning execution");

        println!("Waking up Grbl");
        // TODO: Add grbl wake up sequence if needed

        let gcode: Vec<&str> = gcode_lines.iter().map(|s| s.as_str()).collect();
        buffered_stream(&controller, gcode).expect("Failed to stream G-code");

        println!("Execution complete");
    }
}
