import time
import numpy as np
from serial import Serial
from gpiozero import Button



# Serial
PORT = "/dev/ttyUSB0"
BAUDRATE = 115200
TIMEOUT = 60 # Seconds

# Grbl
RX_BUFFER_SIZE = 128

# Logging
VERBOSE = True



switch = Button(17)



def send(gcode: list[str], serial: Serial):
    l_count = 0
    g_count = 0
    c_line = []
    error_count = 0
    
    for command in gcode:
        l_count += 1
        l_block = command.strip()
        c_line.append(len(l_block) + 1)
        while sum(c_line) >= RX_BUFFER_SIZE - 1 or serial.in_waiting:
            out_temp = serial.readline().strip().decode()
            if out_temp.find('ok') < 0 and out_temp.find('error') < 0:
                print("    MSG: \"" + out_temp + "\"")
            else:
                if out_temp.find('error') >= 0: error_count += 1
                g_count += 1
                if VERBOSE: print("  REC<" + str(g_count) + ": \"" + out_temp + "\"")
                del c_line[0]
        serial.write((l_block + '\n').encode())
        if VERBOSE: print("SND>" + str(l_count) + ": \"" + l_block + "\"")

    while l_count > g_count:
        out_temp = serial.readline().strip().decode()
        if out_temp.find('ok') < 0 and out_temp.find('error') < 0:
            print("    MSG: \"" + out_temp + "\"")
        else:
            if out_temp.find('error') >= 0: error_count += 1
            g_count += 1
            del c_line[0]
            if VERBOSE: print("  REC<" + str(g_count) + ": \"" + out_temp + "\"")



serial = Serial(PORT, BAUDRATE, timeout=TIMEOUT)

# Wake up grbl
print("Initializing Grbl...")
serial.write("\r\n\r\n".encode())

# Wait for grbl to initialize and flush startup text in serial input
time.sleep(2)
serial.reset_input_buffer()

def handle_switch():
    send([chr(0x85)], serial)

switch.when_activated = handle_switch

send([
    "$X",                           # Unlock alarm state (if present)
    "$25=2500",                     # Set home cycle feed speed
    "$H",                           # Execute home cycle
    "G91",                          # Switch to incremental positioning mode
    "$J=X-280 Y750 F3000",          # Jog to rough center of tank
    "$J=Y-750 F1500",
], serial)

print("\nWARNING: Wait until Grbl completes buffered g-code blocks before exiting.")
input("  Press <Enter> to exit and disable Grbl.") 
serial.close()
