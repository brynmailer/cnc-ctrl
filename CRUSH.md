# CRUSH Configuration

## Build Commands
- **Build**: `cargo build`
- **Cross-compile**: `cross build` (ARM target via Cross.toml)
- **Debug build**: `cargo build --bin cnc-control`
- **Release build**: `cargo build --release`
- **Deploy to device**: `just build-sync <DEST> <PASS>`

## Lint & Format
- **Format check**: `cargo fmt --check`
- **Format code**: `cargo fmt`
- **Lint**: `cargo clippy --all-targets`
- **Lint fix**: `cargo clippy --fix --bin cnc-control`

## Test Commands
- **Run tests**: `cargo test`
- **Run specific test**: `cargo test <test_name>`
- **Test with output**: `cargo test -- --nocapture`

## Code Style Guidelines
- **Imports**: Group std imports, external crates, then local modules
- **Error handling**: Custom error enums with Display/Error traits
- **Naming**: snake_case for functions/variables, PascalCase for types
- **Constants**: UPPER_SNAKE_CASE at module level
- **Threading**: Use Arc<AtomicBool> for shared state, crossbeam channels
- **Formatting**: Use rustfmt, inline format args (clippy::uninlined_format_args)
- **Memory**: RAII patterns, explicit Drop implementations for cleanup
- **Hardware**: GPIO/serial abstractions, timeout handling for hardware ops
- **Debugging**: println! for live debugging, structured logging preferred

## Development Workflow
- **Before changes**: Define strict plan and ask for clarification
- **Process**: Analyze request → detailed plan → identify ambiguities → clarify requirements → wait for approval
- **Goal**: Ensure alignment on approach and avoid unexpected results

## Project Structure
- Embedded Rust project for CNC/GPIO control
- Hardware interfaces: serial (Grbl), GPIO (RPi), interrupts
- Multi-threaded message passing architecture