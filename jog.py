from serial import Serial, SerialException
import time
from gpiozero import Button
import numpy as np

# Configuration
PORT = '/dev/ttyUSB0'       # Windows: 'COMx', Linux: '/dev/ttyUSB0', macOS: '/dev/tty.usb*'
BAUDRATE = 115200           # Standard baud rate for GRBL-HAL
TIMEOUT = 1                 # Serial timeout (seconds)
PROBE_SPEED = 0.5           # Multiplier on direction for each step (shouldn't need to change)

tank_directions = [
    np.array([1, 0]),
    np.array([0, 1]),
    np.array([-1, 0]),
    np.array([-0.2402, -0.9707]),
]

bounds = np.array([375, 635])
xy_limit = Button(17)


def send_gcode(gcode_commands: list[str], ser: Serial):
    # Flush startup messages (GRBL version, settings, etc.)
    #time.sleep(2)
    ser.reset_input_buffer()
    
    # Send commands line-by-line
    for command in gcode_commands:
        # Skip empty lines/comments
        stripped_cmd = command.strip()
        if not stripped_cmd or stripped_cmd.startswith(';'):
            continue

        # Send command
        ser.write((stripped_cmd + '\n').encode())
        print(f"Sent: {stripped_cmd}")

        # Wait for response (blocking)
        response = ser.readline().decode().strip()
        while 'ok' not in response and 'error' not in response:
            if response:  # Print non-empty intermediate messages
                print(f"Received: {response}")
            response = ser.readline().decode().strip()

        print(f"Response: {response}")  # Show final "ok" or "error"

    print("All commands executed.")

# Initialization routine
init = [
    "$X",                           # Unlock alarm state (if present)
    "$H",                           # Execute home cycle
    "G91",                          # Switch to incremental positioning mode
    "$J=X-280 Y750 F2500",
]

try:
    with Serial(PORT, BAUDRATE, timeout=TIMEOUT) as serial:
        print(f"Connected to {serial.name}")
        send_gcode(init, serial)
except SerialException as err:
    print(f"Serial error: {err}")
except KeyboardInterrupt:
    print("\nOperation cancelled.")
