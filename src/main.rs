use rppal::gpio::{Gpio, Trigger};
use serialport::{Error as SerialError, SerialPort};

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::thread;
use std::time::Duration;

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

fn stream_buffered(controller: &mut Controller, gcode: Vec<&str>) -> Result<(), SerialError> {
    let mut writer = BufWriter::new(controller.serial.try_clone()?);
    let mut reader = BufReader::new(controller.serial.try_clone()?);

    for line in gcode {
        controller.bytes_queued.push_back(line.len() + 1); // Additional byte for newline char

        // Wait for buffer space
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

        // Send command
        writer.write_all(format!("{}\n", line).as_bytes())?;
        writer.flush()?;
        controller.sent_count += 1;
        println!("SND>{}: \"{}\"", controller.sent_count, line);
    }

    // Wait for remaining responses
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
        .expect("SERIAL: Failed to open connection");

    let mut controller = Controller {
        serial,
        sent_count: 0,
        received_count: 0,
        bytes_queued: VecDeque::new(),
    };

    let gpio = Gpio::new().expect("GPIO: Failed to intialize");
    let mut button = gpio
        .get(BUTTON_PIN)
        .expect("GPIO: Failed to initialize button")
        .into_input_pullup();
    let mut probe = gpio
        .get(PROBE_PIN)
        .expect("GPIO: Failed to initialize probe")
        .into_input_pullup();

    button
        .set_interrupt(Trigger::RisingEdge, Some(Duration::from_millis(30)))
        .expect("GPIO: Failed to initialize probe interrupt");
    probe
        .set_async_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(30)),
            move |_| {
                println!("GPIO: Probe interrupt triggered");
            },
        )
        .expect("GPIO: Failed to initialize probe interrupt");

    loop {
        button
            .poll_interrupt(true, None)
            .expect("GPIO: Failed to poll button interrupt");

        println!("SERIAL: Waking up Grbl");
        controller
            .serial
            .write_all(b"\n\n")
            .expect("SERIAL: Write failed");
        thread::sleep(Duration::from_secs(2));
        controller
            .serial
            .clear(serialport::ClearBuffer::Input)
            .expect("SERIAL: Failed to clear input buffer");

        let gcode = vec![
            "$X",                  // Unlock alarm state (if present)
            "$25=2500",            // Set home cycle feed speed
            "$H",                  // Execute home cycle
            "G91",                 // Switch to incremental positioning mode
            "$J=X-280 Y750 F3000", // Jog to rough center of tank
        ];

        stream_buffered(&mut controller, gcode).expect("SERIAL: Failed to stream G-code");
    }
}
