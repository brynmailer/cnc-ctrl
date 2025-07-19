from serial import Serial, SerialException
import time
from gpiozero import Button
import numpy as np

# Configuration
PORT = '/dev/ttyUSB0'       # Windows: 'COMx', Linux: '/dev/ttyUSB0', macOS: '/dev/tty.usb*'
BAUDRATE = 115200   # Standard baud rate for GRBL-HAL
TIMEOUT = 1         # Serial timeout (seconds)
PROBE_SPEED = 0.5     # Multiplier on direction for each step (shouldnt need to change)

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

def limit_hit():
    if xy_limit.is_pressed:
        return False
    else:
        return True

def in_bounds(new_coordinates):
    if (abs(new_coordinates[0] < bounds[0])) and (abs(new_coordinates[1] < bounds[1])):
        return True
    else:
        return False

def probe_direction(direction, ser, current_coordinates, speed=5000):
    while True:
        new_coordinates = np.round(current_coordinates + direction, decimals=3)
        if not in_bounds(new_coordinates):
            break
        send_gcode([f"G1 X{new_coordinates[0]} Y{new_coordinates[1]} F{speed}"], ser)
        current_coordinates = new_coordinates
        time.sleep(0.05)
        if limit_hit():
            break
    return current_coordinates

def unit_circle_vectors(n):
    angles = np.linspace(0, 2 * np.pi, n, endpoint=False)
    vectors = np.stack((np.cos(angles), np.sin(angles)), axis=-1)
    return vectors

def circle_probe(resolution, ser):
    unit_directions = PROBE_SPEED * unit_circle_vectors(resolution)
    with open("base.csv", "w") as file:
        for direction in unit_directions:
            send_gcode(["G1 X0 Y0 F5000"], ser)
            current_coordinates = np.array([0, 0])
            point = probe_direction(direction, ser, current_coordinates)
            file.write(f"{point[0]}, {point[1]}\n")
            print(point)
            time.sleep(1)

def normalize(v):
    norm = np.linalg.norm(v)
    return v / norm if norm != 0 else v

def concentric_directions(coords1, coords2):
    """
    Returns a function that computes the normalised directions from coords1 to coords2.

    Parameters:
    - coords1 (np.ndarray): Source coordinates of shape (N, D)
    - coords2 (np.ndarray): Target coordinates of shape (N, D)
    - normaliser (callable): Function that normalises a vector or array of vectors

    Returns:
    - function: A function that returns the normalised direction vectors
    """

    # Validate shape
    if coords1.shape != coords2.shape:
        raise ValueError("Coordinate arrays must have the same shape.")
    
    # Compute raw direction vectors
    direction_vectors = coords2 - coords1

    # Normalize directions
    normalised_directions = normaliser(direction_vectors)
    
    return normalised_directions

def get_normals(points):
    normals = []
    n = len(points)
    for i in range(n):
        prev = points[i - 1]
        curr = points[i]
        next = points[(i + 1) % n]

        edge1 = normalize(curr - prev)
        edge2 = normalize(next - curr)

        # Get normals (perpendiculars)
        normal1 = np.array([-edge1[1], edge1[0]])
        normal2 = np.array([-edge2[1], edge2[0]])

        # Average of two normals
        avg_normal = normalize(normal1 + normal2)
        normals.append(avg_normal)
    return np.array(normals)

def offset_shape(points, offset):
    normals = get_normals(points)
    return points + normals * offset

def concentric_path():
    coords = np.loadtxt("base.csv", delimiter=',')
    concentric_coords = offset_shape(coords, 5)
    np.savetxt("concentric.csv", concentric_coords, delimiter=',')

def shell_probe(ser):
    coords = np.loadtxt("base.csv", delimiter=',')
    concentric_coords = np.loadtxt("concentric.csv", delimiter=',')
    directions = concentric_directions(coords, concentric_coords)
    i = 0
    for i in range(len(coords)):
        send_gcode([f"G1 X{coords[i][0]} Y{coords[i][1]} F5000"], ser)
        current_coordinates = np.array([coords[i][0], coords[i][1]])
        probe_direction(directions[i], ser, current_coordinates)
        send_gcode([f"G1 X{coords[i][0]} Y{coords[i][1]} F5000"], ser)

def tank_center(ser):
    tank_coords = np.zeros((4, 2))
    for i in range(len(tank_directions)):
        tank_coords[i] = probe_direction(tank_directions[i], ser, np.array([0, 0]))
        send_gcode(["G1 X0 Y0 F5000"], ser)
        time.sleep(1)
    center = tank_coords.mean(axis=0)
    send_gcode([f"G10 L20 P1 X{339 + center[0]} Y{-744.735 + center[1]}", "G1 X0 Y0 F5000"], ser)
    

# Example G-Code commands
startup = [
    "$H",
    "$X",           # Unlock alarm state (if present)
    "G21",          # Set units to mm
    "G90",          # Absolute positioning   ; Rapid move
    "G1 Z0 F1000",
    "G10 L20 P1 X339 Y-744.735",
    "G1 X0 Y0 F5000",
    #"M3 S1000",     # Spindle on at 1000 RPM
    "G1 Z-60 F1000",
    #"G4 P2",        # Pause for 2 seconds
    #"M5",           # Spindle off
]

# Execute
try:
    # Open connection
    with Serial(PORT, BAUDRATE, timeout=TIMEOUT) as ser:
        print(f"Connected to {ser.name}")
        send_gcode(startup, ser)
        #circle_probe(12, ser)
        #shell_probe(ser)
        tank_center(ser)
except SerialException as e:
    print(f"Serial error: {e}")
except KeyboardInterrupt:
    print("\nOperation cancelled.")
#concentric_path()
