import time
import numpy as np
from serial import Serial, SerialException
from gpiozero import Button

# Configuration
PORT = '/dev/ttyUSB0'       # Windows: 'COMx', Linux: '/dev/ttyUSB0', macOS: '/dev/tty.usb*'
BAUDRATE = 115200           # Standard baud rate for GRBL-HAL
TIMEOUT = 60                # Serial timeout (seconds)
RX_BUFFER_SIZE = 128
VERBOSE = True

tank_directions = [
    np.array([1, 0]),
    np.array([0, 1]),
    np.array([-1, 0]),
    np.array([-0.2402, -0.9707]),
]

bounds = np.array([375, 635])
xy_limit = Button(17)



def send(gcode: list[str], serial: Serial):
    # Send g-code program via a more agressive streaming protocol that forces characters into
    # Grbl's serial read buffer to ensure Grbl has immediate access to the next g-code command
    # rather than wait for the call-response serial protocol to finish. This is done by careful
    # counting of the number of characters sent by the streamer to Grbl and tracking Grbl's 
    # responses, such that we never overflow Grbl's serial read buffer. 
    l_count = 0
    g_count = 0
    c_line = []
    error_count = 0
    for command in gcode:
        l_count += 1 # Iterate line counter
        l_block = command.strip()
        c_line.append(len(l_block) + 1) # Track number of characters in grbl serial read buffer
        while sum(c_line) >= RX_BUFFER_SIZE - 1 or serial.in_waiting:
            out_temp = serial.readline().strip().decode() # Wait for grbl response
            if out_temp.find('ok') < 0 and out_temp.find('error') < 0:
                print("    MSG: \"" + out_temp + "\"") # Debug response
            else:
                if out_temp.find('error') >= 0: error_count += 1
                g_count += 1 # Iterate g-code counter
                if VERBOSE: print("  REC<" + str(g_count) + ": \"" + out_temp + "\"")
                del c_line[0] # Delete the block character count corresponding to the last 'ok'
        serial.write((l_block + '\n').encode()) # Send g-code block to grbl
        if VERBOSE: print("SND>" + str(l_count) + ": \"" + l_block + "\"")
    # Wait until all responses have been received.
    while l_count > g_count:
        out_temp = serial.readline().strip().decode() # Wait for grbl response
        if out_temp.find('ok') < 0 and out_temp.find('error') < 0:
            print("    MSG: \"" + out_temp + "\"") # Debug response
        else:
            if out_temp.find('error') >= 0: error_count += 1
            g_count += 1 # Iterate g-code counter
            del c_line[0] # Delete the block character count corresponding to the last 'ok'
            if VERBOSE: print("  REC<" + str(g_count) + ": \"" + out_temp + "\"")



serial = Serial(PORT, BAUDRATE, timeout=TIMEOUT)

# Wake up grbl
print("Initializing Grbl...")
serial.write("\r\n\r\n".encode())

# Wait for grbl to initialize and flush startup text in serial input
time.sleep(2)
serial.reset_input_buffer()

send([
    "$X",                           # Unlock alarm state (if present)
    "$25=2500",                     # Set home cycle feed speed
    "$H",                           # Execute home cycle
    "G91",                          # Switch to incremental positioning mode
    "$J=X-280 Y750 F2500",          # Jog to rough center of tank
], serial)

print("WARNING: Wait until Grbl completes buffered g-code blocks before exiting.")
input("  Press <Enter> to exit and disable Grbl.") 
serial.close()
