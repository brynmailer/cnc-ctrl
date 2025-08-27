# CNC Ctrl

A Rust-based CNC control system designed for Raspberry Pi 4B, providing GPIO-based input handling and serial communication with CNC controllers running [grblHAL](https://github.com/grblHAL/core) firmware.

## Features

- Serial communication with grblHAL CNC controllers
- GPIO input handling to support buttons for control flow (designed for Raspberry Pi)
- Multi-threaded message passing architecture for minimal latency when issuing commands over serial
- Configurable workflow steps (G-code execution and bash commands)
- Configurable logging

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

## Job configuration

The `cnc-ctrl` command expects a path to a job configuration file as its first positional argument. This configuration describes general operational settings, as well as the tasks that should be executed as part of the job.

### Options

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
  signal: 17              # Manual signal butoon for control flow
```

#### Workflow Steps
Define a sequence of operations to execute:

```yaml
steps:
  - type: gcode                                         # Execute G-code file
    path: "~/path/to/step.gcode"                        # Path to G-code file
    check: false                                        # Skip G-code syntax checking
    wait_for_signal: true                               # Wait for signal input (default: true)
    probe:                                              # Probe point logging (optional)
      save_path: "~/path/to/probe-points.csv"           # Output file path
  
  - type: bash                                          # Execute bash command
    wait_for_signal: false                              # Don't wait for signal (default: false)
    command: "python some-script.py"
```

### Template Variables

The `{%t}` template variable in file paths is replaced with a timestamp (format: `YYYYMMDD_HHMMSS`) when the application starts.

### Step Types

- **gcode**: Execute G-code files via serial communication with grblHAL
  - `path`: Path to G-code file
  - `check`: Validate G-code syntax via Grbl check mode before execution (default: true)
  - `wait_for_signal`: Wait for signal input before execution (default: true)
  - `probe`: Optional probe point logging configuration
    - `save_path`: Path to file that probed points should be saved to (points are output in csv format)

- **bash**: Execute shell commands
  - `command`: Shell command to execute
  - `wait_for_signal`: Wait for signal input before execution (default: false)

## Usage

1. Create your job config file as described above
2. Connect your grblHAL controller via serial
3. Wire GPIO signal input according to your configuration
4. Run the application:
   ```bash
   cnc-ctrl ~/path/to/job-config.yml
   ```

The application will execute the configured workflow steps in sequence, waiting for signal input before proceeding with steps as specified in the job configuration.
