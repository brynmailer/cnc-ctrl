use rppal::gpio::{Gpio, InputPin, Trigger};
use serialport::{self, SerialPort};

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::thread;
use std::time::Duration;

const PORT: &str = "/dev/ttyUSB0";
const BAUDRATE: u32 = 115200;
const TIMEOUT_MS: u64 = 60000; // 60 seconds
const RX_BUFFER_SIZE: usize = 128;
const SWITCH_PIN: u8 = 27;

struct Controller {
    serial: Box<dyn SerialPort>,
    switch: InputPin,
}

impl Controller {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize serial port
        let serial = serialport::new(PORT, BAUDRATE)
            .timeout(Duration::from_millis(TIMEOUT_MS))
            .open()?;

        // Initialize GPIO
        let gpio = Gpio::new()?;
        let switch = gpio.get(SWITCH_PIN)?.into_input_pullup();

        Ok(Controller { serial, switch })
    }

    fn execute(&mut self, gcode: Vec<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let mut sent_count = 0;
        let mut received_count = 0;
        let mut bytes_written = VecDeque::new();

        let mut reader = BufReader::new(self.serial.try_clone()?);
        let mut writer = BufWriter::new(self.serial.try_clone()?);

        for line in gcode {
            sent_count += 1;
            bytes_written.push_back(line.len());

            // Wait for buffer space
            while bytes_written.iter().sum::<usize>() >= RX_BUFFER_SIZE {
                let mut res = String::new();
                reader.read_line(&mut res)?;
                res = res.trim().to_string();

                if !res.contains("ok") && !res.contains("error") {
                    println!("    MSG: \"{}\"", res);
                } else {
                    received_count += 1;
                    println!("  REC<{}: \"{}\"", received_count, res);

                    if !bytes_written.is_empty() {
                        bytes_written.pop_front();
                    }
                }
            }

            // Send command
            writer.write_all(format!("{}\n", line).as_bytes())?;
            writer.flush()?;
            println!("SND>{}: \"{}\"", sent_count, line);
        }

        // Wait for remaining responses
        while sent_count > received_count {
            let mut res = String::new();
            reader.read_line(&mut res)?;
            res = res.trim().to_string();

            if !res.contains("ok") && !res.contains("error") {
                println!("    MSG: \"{}\"", res);
            } else {
                received_count += 1;
                println!("  REC<{}: \"{}\"", received_count, res);

                if !bytes_written.is_empty() {
                    bytes_written.pop_front();
                }
            }
        }

        Ok(())
    }

    fn initialize_grbl(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("INIT: Started");

        // Wake up grbl
        self.serial.write_all(b"\r\n\r\n")?;

        // Wait for grbl to initialize
        thread::sleep(Duration::from_secs(2));

        // Clear input buffer
        self.serial.clear(serialport::ClearBuffer::Input)?;

        println!("INIT: Complete");
        Ok(())
    }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.initialize_grbl()?;

        self.switch.set_async_interrupt(
            Trigger::RisingEdge,
            Some(Duration::from_millis(100)),
            |_| {
                println!("BTN: Pressed");
            },
        )?;

        // Send initialization commands
        self.execute(vec![
            "$X",                  // Unlock alarm state (if present)
            "$25=2500",            // Set home cycle feed speed
            "$H",                  // Execute home cycle
            "G91",                 // Switch to incremental positioning mode
            "$J=X-280 Y750 F3000", // Jog to rough center of tank
            "$J=Y-750 F1500",
        ])?;

        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = Controller::new()?;
    controller.run()?;
    Ok(())
}
