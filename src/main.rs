mod command;
mod controller;
mod message;

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use regex::Regex;
use rppal::gpio::{self, Gpio, Trigger};
use serialport::SerialPort;

use command::Command;
use controller::Controller;
use message::{Push, Report};

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
                serial_tx
                    .send(Command::Realtime(0x85))
                    .expect("Failed to send interrupt command");
            },
        )
        .expect("Failed to initialize probe interrupt");

    (button, probe)
}

fn buffered_stream(
    controller: &Controller,
    gcode: Vec<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some((prio_serial_tx, prio_serial_rx)) = controller.prio_serial_channel.clone() else {
        panic!("Failed to clone serial: Controller not started");
    };

    let Some((serial_tx, serial_rx)) = controller.serial_channel.clone() else {
        panic!("Failed to stream gcode: Controller not started");
    };

    let re = Regex::new(r"^\$J=.* IN$")?;
    let mut bytes_queued = VecDeque::new();
    let mut received_count = 0;
    let mut sent_count = 0;

    for raw_line in gcode {
        let interruptible = re.is_match(raw_line);
        let line = raw_line.trim().strip_suffix(" IN").unwrap();

        bytes_queued.push_back(line.len());

        while bytes_queued.iter().sum::<usize>() >= RX_BUFFER_SIZE {
            serial_rx.recv()?;
            received_count += 1;
            bytes_queued.pop_front();
        }

        serial_tx.send(Command::Gcode(line.to_string()))?;
        sent_count += 1;

        if interruptible {
            let polling = Arc::new(AtomicBool::new(true));

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
                    match prio_serial_rx.recv() {
                        Ok(Push::Report(Report { status, mpos, .. }))
                            if status == Some("Idle".to_string()) && mpos.is_some() =>
                        {
                            let unwrapped_pos = mpos.unwrap();
                            println!(
                                "x: {}, y: {}, z: {}",
                                unwrapped_pos.0, unwrapped_pos.1, unwrapped_pos.2
                            );

                            break;
                        }
                        Err(err) => panic!("Failed to wait for interrupt: {}", err),
                        _ => continue,
                    }
                }

                polling.store(false, Ordering::Relaxed);
            });
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
    let serial = serialport::new(PORT, BAUDRATE)
        .timeout(Duration::from_millis(TIMEOUT_MS))
        .open()
        .expect("Failed to open serial connection");
    let mut serial_clone = serial
        .try_clone()
        .expect("Failed to clone serial connection");

    let mut controller = Controller::new();
    controller.start(serial);

    let (button, probe) = init_gpio(&controller);

    loop {
        println!("Waiting for start signal...");
        button
            .poll_interrupt(true, None)
            .expect("Failed to poll button interrupt");

        println!("Beginning execution");

        println!("Waking up Grbl");
        controller
            .serial
            .write_all(b"\n\n")
            .expect("Serial write failed");
        thread::sleep(Duration::from_secs(2));
        controller
            .serial
            .clear(serialport::ClearBuffer::Input)
            .expect("Failed to clear serial input buffer");

        // let gcode = vec[];

        buffered_stream(&mut controller, gcode).expect("Failed to stream G-code");

        wait_for_status(&mut controller, Status::Idle).expect("Failed to wait for Idle status");

        println!("Execution complete");
    }
}
