import os
import numpy as np

TRAVEL_SPEED=5000
PROBE_SPEED=250
START_Z = -65
OFFSET_MM = 12.75
  
def shell_probe(type):
    inner_coords = np.loadtxt(f"{type}concentric_inner.csv", delimiter=',', skiprows=1)
    outer_coords = np.loadtxt(f"{type}concentric_outer.csv", delimiter=',', skiprows=1)

    with open(os.path.expanduser(f'./{type}probe.gcode'), 'w') as output:
        for line in startup:
            output.write(line + '\n')

        for index in range(len(inner_coords)):
            output.write(f"G0 X{inner_coords[index][0]:.4f} Y{inner_coords[index][1]:.4f} F{TRAVEL_SPEED}\n")
            
            if index == 0:
                output.write(f"G0 Z{START_Z} F{TRAVEL_SPEED / 4}\n")
            
            output.write(f"G38.2 X{outer_coords[index][0]:.4f} Y{outer_coords[index][1]:.4f} F{PROBE_SPEED}\n")

            angle = np.arctan2(outer_coords[index][1] - inner_coords[index][1], outer_coords[index][0] - inner_coords[index][0])
            x_offset = (-OFFSET_MM) * np.cos(angle)
            y_offset = (-OFFSET_MM) * np.sin(angle)

            output.write("G91\n")                                                         
            output.write(f"G0 X{x_offset:.4f} Y{y_offset:.4f} F{TRAVEL_SPEED / 4}\n")
            output.write("G90\n")
            output.write(f"G38.2 Z-80 F{PROBE_SPEED}\n")
            output.write(f"G0 Z{START_Z} F{TRAVEL_SPEED / 4}\n")

            output.write(f"G0 X{inner_coords[index][0]:.4f} Y{inner_coords[index][1]:.4f} F{TRAVEL_SPEED / 4}\n")

# Example G-Code commands
startup = [
    "$X",           # Unlock alarm state (if present)
    "$H",           # Execute homing cycle
    "G21",          # Set units to mm
    "G90",          # Absolute positioning
]

shell_probe('a')
shell_probe('b')
