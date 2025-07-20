import re
import numpy as np
from serial import Serial, SerialException
from gpiozero import Button

# Configuration
PORT = '/dev/ttyUSB0'       # Windows: 'COMx', Linux: '/dev/ttyUSB0', macOS: '/dev/tty.usb*'
BAUDRATE = 115200           # Standard baud rate for GRBL-HAL
RX_BUFFER_SIZE = 128

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
    for command in gcode:
        l_count += 1 # Iterate line counter
        # l_block = re.sub('\s|\(.*?\)','',command).upper() # Strip comments/spaces/new line and capitalize
        l_block = command.strip()
        c_line.append(len(l_block) + 1) # Track number of characters in grbl serial read buffer
        grbl_out = '' 
        while sum(c_line) >= RX_BUFFER_SIZE-1 | s.inWaiting() :
            out_temp = serial.readline().strip() # Wait for grbl response
            if out_temp.find('ok') < 0 and out_temp.find('error') < 0 :
                print("    MSG: \"" + out_temp + "\"") # Debug response
            else :
                if out_temp.find('error') >= 0 : error_count += 1
                g_count += 1 # Iterate g-code counter
                if verbose: print "  REC<"+str(g_count)+": \""+out_temp+"\""
                del c_line[0] # Delete the block character count corresponding to the last 'ok'
        s.write(l_block + '\n') # Send g-code block to grbl
        if verbose: print "SND>"+str(l_count)+": \"" + l_block + "\""
    # Wait until all responses have been received.
    while l_count > g_count :
        out_temp = s.readline().strip() # Wait for grbl response
        if out_temp.find('ok') < 0 and out_temp.find('error') < 0 :
            print "    MSG: \""+out_temp+"\"" # Debug response
        else :
            if out_temp.find('error') >= 0 : error_count += 1
            g_count += 1 # Iterate g-code counter
            del c_line[0] # Delete the block character count corresponding to the last 'ok'
            if verbose: print "  REC<"+str(g_count)+": \""+out_temp + "\""

# Initialization routine
init = [
    "$X",                           # Unlock alarm state (if present)
    "$25=2500",                     # Set home cycle feed speed
    "$H",                           # Execute home cycle
    "G91",                          # Switch to incremental positioning mode
    "$J=X-280 Y750 F2500",          # Jog to rough center of tank
]

try:
    with Serial(PORT, BAUDRATE, timeout=TIMEOUT) as serial:
        print(f"Connected to {serial.name}")
        send(init, serial)
except SerialException as err:
    print(f"Serial error: {err}")
except KeyboardInterrupt:
    print("\nOperation cancelled.")
