use regex::Regex;
use rppal::gpio::{Gpio, Trigger};
use serialport::SerialPort;

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::time::Duration;
use std::{fmt, thread};

// Serial
const PORT: &str = "/dev/ttyUSB0";
const BAUDRATE: u32 = 115200;
const TIMEOUT_MS: u64 = 60000;

// GPIO
const BUTTON_PIN: u8 = 22;
const PROBE_PIN: u8 = 27;

// GbrlHAL
const RX_BUFFER_SIZE: usize = 1024;

struct Controller {
    serial: Box<dyn SerialPort>,
    sent_count: usize,
    received_count: usize,
    bytes_queued: VecDeque<usize>,
}

#[derive(Debug)]
enum Status {
    Idle,
    Home,
    Jog,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_string = match self {
            Status::Idle => "Idle",
            Status::Home => "Home",
            Status::Jog => "Jog",
        };
        write!(f, "{}", status_string)
    }
}

fn wait_for_status(
    controller: &mut Controller,
    status: Status,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = BufWriter::new(controller.serial.try_clone()?);
    let mut reader = BufReader::new(controller.serial.try_clone()?);
    let re = Regex::new(r"<([^|>]+)")?;

    loop {
        writer.write_all("?".as_bytes())?;
        writer.flush()?;

        let mut res = String::new();
        reader.read_line(&mut res)?;
        res = res.trim().to_string();

        if let Some(captures) = re.captures(&res) {
            if captures[1] == status.to_string() {
                return Ok(());
            }
        }

        thread::sleep(Duration::from_millis(500));
    }
}

fn buffered_stream(
    controller: &mut Controller,
    gcode: Vec<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = BufWriter::new(controller.serial.try_clone()?);
    let mut reader = BufReader::new(controller.serial.try_clone()?);
    let re = Regex::new(r"^IN$J=")?;

    for raw_line in gcode {
        let interruptible = re.is_match(raw_line);
        let line = if interruptible {
            format!("{}\n", &raw_line[2..])
        } else {
            format!("{}\n", raw_line)
        };

        controller.bytes_queued.push_back(line.len());

        while controller.bytes_queued.iter().sum::<usize>() >= RX_BUFFER_SIZE {
            let mut res = String::new();
            reader.read_line(&mut res)?;
            res = res.trim().to_string();

            if !res.contains("ok") && !res.contains("error") {
                println!("    MSG: \"{}\"", res);
            } else {
                controller.received_count += 1;
                println!("  REC<{}: \"{}\"", controller.received_count, res);

                if !controller.bytes_queued.is_empty() {
                    controller.bytes_queued.pop_front();
                }
            }
        }

        writer.write_all(line.as_bytes())?;
        writer.flush()?;
        controller.sent_count += 1;
        println!(
            "SND>{}: {}\"{}\"",
            controller.sent_count,
            interruptible.then(|| "IN-").unwrap_or(""),
            line
        );

        if interruptible {
            wait_for_status(controller, Status::Idle)?;
        }
    }

    while controller.sent_count > controller.received_count {
        let mut res = String::new();
        reader.read_line(&mut res)?;
        res = res.trim().to_string();

        if !res.contains("ok") && !res.contains("error") {
            println!("    MSG: \"{}\"", res);
        } else {
            controller.received_count += 1;
            println!("  REC<{}: \"{}\"", controller.received_count, res);

            if !controller.bytes_queued.is_empty() {
                controller.bytes_queued.pop_front();
            }
        }
    }

    Ok(())
}

fn main() {
    let serial = serialport::new(PORT, BAUDRATE)
        .timeout(Duration::from_millis(TIMEOUT_MS))
        .open()
        .expect("Failed to open serial connection!");
    let mut serial_clone = serial
        .try_clone()
        .expect("Failed to clone serial connection!");

    let mut controller = Controller {
        serial,
        sent_count: 0,
        received_count: 0,
        bytes_queued: VecDeque::new(),
    };

    let gpio = Gpio::new().expect("Failed to intialize GPIO!");
    let mut button = gpio
        .get(BUTTON_PIN)
        .expect("Failed to initialize button!")
        .into_input_pullup();
    let mut probe = gpio
        .get(PROBE_PIN)
        .expect("Failed to initialize probe!")
        .into_input_pullup();

    button
        .set_interrupt(Trigger::RisingEdge, Some(Duration::from_millis(30)))
        .expect("Failed to initialize probe interrupt!");
    probe
        .set_async_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(30)),
            move |_| {
                println!("Probe interrupt triggered! Sending jog cancel command");
                serial_clone
                    .write_all(&[0x85])
                    .expect("Serial write failed!");
            },
        )
        .expect("Failed to initialize probe interrupt!");

    loop {
        println!("Waiting for start signal...");
        button
            .poll_interrupt(true, None)
            .expect("Failed to poll button interrupt!");

        println!("Beginning execution");

        println!("Waking up Grbl");
        controller
            .serial
            .write_all(b"\n\n")
            .expect("Serial write failed!");
        thread::sleep(Duration::from_secs(2));
        controller
            .serial
            .clear(serialport::ClearBuffer::Input)
            .expect("Failed to clear serial input buffer!");

        let gcode = vec![
            "$X",                    // Unlock alarm state (if present)
            "$25=2500",              // Set home cycle feed speed
            "$H",                    // Execute home cycle
            "G91",                   // Switch to incremental positioning mode
            "IN$J=X-280 Y750 F1500", // Jog to rough center of tank
            "IN$J=Y-750 F1500",
            "IN$J=Y750 F1500",
        ];

        buffered_stream(&mut controller, gcode).expect("Failed to stream G-code!");

        wait_for_status(&mut controller, Status::Idle).expect("Failed to wait for Idle status!");

        println!("Execution complete!");
    }
}
