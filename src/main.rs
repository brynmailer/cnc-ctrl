use rppal::gpio::{Gpio, InputPin};
use serialport::{self, SerialPort};

use std::io::{self, Write};
use std::thread;
use std::time::Duration;

// Serial configuration
const PORT: &str = "/dev/ttyUSB0";
const BAUDRATE: u32 = 115200;
const TIMEOUT_MS: u64 = 60000; // 60 seconds

// Grbl configuration
const RX_BUFFER_SIZE: usize = 128;

// Logging
const VERBOSE: bool = true;

// GPIO configuration
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
        let switch = gpio.get(SWITCH_PIN)?.into_input();

        Ok(Controller { serial, switch })
    }

    fn send_gcode(&mut self, commands: Vec<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let mut l_count = 0;
        let mut g_count = 0;
        let mut c_line = Vec::new();
        let mut error_count = 0;

        for command in commands {
            l_count += 1;
            let l_block = command.trim();
            c_line.push(l_block.len() + 1);

            // Wait for buffer space or incoming data
            while c_line.iter().sum::<usize>() >= RX_BUFFER_SIZE - 1
                || self.serial.bytes_to_read()? > 0
            {
                let mut buffer = vec![0; 256];
                let bytes_read = self.serial.read(&mut buffer)?;
                let response = String::from_utf8_lossy(&buffer[..bytes_read])
                    .trim()
                    .to_string();

                if !response.contains("ok") && !response.contains("error") {
                    println!("    MSG: \"{}\"", response);
                } else {
                    if response.contains("error") {
                        error_count += 1;
                    }
                    g_count += 1;
                    if VERBOSE {
                        println!("  REC<{}: \"{}\"", g_count, response);
                    }
                    if !c_line.is_empty() {
                        c_line.remove(0);
                    }
                }
            }

            // Send command
            let command_with_newline = format!("{}\n", l_block);
            self.serial.write_all(command_with_newline.as_bytes())?;
            if VERBOSE {
                println!("SND>{}: \"{}\"", l_count, l_block);
            }
        }

        // Wait for all responses
        while l_count > g_count {
            let mut buffer = vec![0; 256];
            let bytes_read = self.serial.read(&mut buffer)?;
            let response = String::from_utf8_lossy(&buffer[..bytes_read])
                .trim()
                .to_string();

            println!("{response}");
            if !response.contains("ok") && !response.contains("error") {
                println!("    MSG: \"{}\"", response);
            } else {
                if response.contains("error") {
                    error_count += 1;
                }
                g_count += 1;
                if !c_line.is_empty() {
                    c_line.remove(0);
                }
                if VERBOSE {
                    println!("  REC<{}: \"{}\"", g_count, response);
                }
            }
        }

        Ok(())
    }

    fn initialize_grbl(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Initializing Grbl...");

        // Wake up grbl
        self.serial.write_all(b"\r\n\r\n")?;

        // Wait for grbl to initialize
        thread::sleep(Duration::from_secs(2));

        // Clear input buffer
        self.serial.clear(serialport::ClearBuffer::Input)?;

        Ok(())
    }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.initialize_grbl()?;

        // Send initialization commands
        self.send_gcode(vec![
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
