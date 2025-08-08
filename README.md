# CNC Ctrl

A Rust-based CNC control system designed for Raspberry Pi 4B, providing GPIO-based input handling and serial communication with CNC controllers running [grblHAL](https://github.com/grblHAL/core) firmware.

## Features

- Serial communication with grblHAL CNC controllers
- GPIO input handling to support a custom probing attachment, and buttons for control flow (designed for Raspberry Pi)
- Multi-threaded message passing architecture for minimal latency when issuing commands over serial
- Configurable workflow steps (G-code execution and bash commands)
- Automatic probe point logging and data transformation

## Installation

### Prerequisites

- Rust toolchain (latest stable)
- Cross compilation tools for ARM64 targets

### Cross-compilation Setup

This project is configured to cross-compile for ARM64 Linux targets (e.g., Raspberry Pi 4). Install the cross compilation tool:

```bash
cargo install cross
```

### Building

For local development:
```bash
cargo build
```

For ARM64 target (recommended for deployment):
```bash
cross build --release
```

The project automatically installs required dependencies (`libudev-dev`) during cross-compilation via the `Cross.toml` configuration.

### Deployment

Use the provided justfile command to build and sync to your target device:
```bash
just build-sync <DESTINATION> <PASSWORD>
```

## Configuration

The application expects a configuration file at `~/.config/cnc-ctrl/config.yml`. See `docs/example-config.yml` for a complete example.

### Configuration Options

#### Logs
```yaml
logs:
  verbose: true             # Enable verbose logging output
  save: true                # Save logs to file
  path: "~/cnc/logs/{%t}"   # Log file path (supports {%t} timestamp template)
```

#### Serial Communication
```yaml
serial:
  port: "/dev/ttyUSB0"   # Serial port for grblHAL controller
  baudrate: 115200       # Communication baud rate
  timeout_ms: 60000      # Command timeout in milliseconds
```

#### grblHAL Settings
```yaml
grbl:
  rx_buffer_size_bytes: 1024  # grblHAL RX buffer size for command batching
```

#### GPIO Inputs
```yaml
inputs:
  signal:               # Manual signal input
    pin: 17             # GPIO pin number
    debounce_ms: 30     # Debounce delay in milliseconds
  probe_xy:             # XY probe input
    pin: 27
    debounce_ms: 30
  probe_z:              # Z probe input
    pin: 22
    debounce_ms: 30
```

#### Workflow Steps
Define a sequence of operations to execute:

```yaml
steps:
  - type: gcode                                   # Execute G-code file
    path: "~/cnc/tanks/probe.gcode"               # Path to G-code file
    check: false                                  # Skip G-code syntax checking
    wait_for_signal: true                         # Wait for signal input (default: true)
    points:                                       # Probe point logging (optional)
      save: true                                  # Save probe points to file
      path: "~/cnc/tanks/points/tank-{%t}.csv"    # Output file path
  
  - type: bash                                    # Execute bash command
    wait_for_signal: false                        # Don't wait for signal (default: false)
    command: "python ~/cnc/transform.py -p ~/cnc/points/tank-{%t}.csv -o ~/cnc/cut.gcode"
```

### Template Variables

The `{%t}` template variable in file paths is replaced with a timestamp (format: `YYYYMMDD_HHMMSS`) when the application starts.

### Step Types

- **gcode**: Execute G-code files via serial communication with grblHAL
  - `path`: Path to G-code file
  - `check`: Validate G-code syntax before execution (default: true)
  - `wait_for_signal`: Wait for signal input before execution (default: true)
  - `points`: Optional probe point logging configuration

- **bash**: Execute shell commands
  - `command`: Shell command to execute
  - `wait_for_signal`: Wait for signal input before execution (default: false)

## Usage

1. Create your configuration file at `~/.config/cnc-ctrl/config.yml`
2. Connect your grblHAL controller via serial
3. Wire GPIO inputs according to your configuration
4. Run the application:
   ```bash
   ./cnc-ctrl
   ```

The application will execute the configured workflow steps in sequence, waiting for signal inputs as specified in the configuration.
