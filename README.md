# cbor_test

`cbor_test` is a Rust command-line application for testing and interacting with an Arduino Giga device over a serial port. It sends and receives data using a CBOR (Concise Binary Object Representation) and COBS (Consistent Overhead Byte Stuffing) based protocol.

## Features

- **Serial Communication**: Connects to a specified serial port to communicate with the Arduino Giga.
- **CBOR/COBS Protocol**: Serializes commands into CBOR and frames them with COBS for reliable transmission.
- **Interactive Command-Line**: Accepts JSON-formatted commands from the user via standard input.
- **Asynchronous I/O**: Uses `tokio` for non-blocking serial port communication and user input.
- **Structured Logging**: Employs the `tracing` library to log to both the console and daily rolling log files in the `logs/` directory.
- **Real-time Monitoring**: Optional display of live sensor data from the Arduino device.
- **Automatic Reconnection**: Robust error handling with automatic reconnection capabilities.

## Architecture

The application implements a layered communication protocol:

1. **JSON Input** → User provides commands as JSON strings
2. **Rust Structs** → JSON deserialized to `MotorCommandParams`
3. **CBOR Serialization** → Structs serialized to compact binary format
4. **COBS Framing** → Binary data wrapped for reliable serial transmission

The system uses asynchronous tasks for concurrent operation:

- Main task handles user input and command parsing
- Background task manages persistent serial connection and device communication

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) toolchain (edition 2024)
- An Arduino Giga device flashed with the corresponding firmware
- Access to the `pingpong-arduino` library (GitHub dependency)

### Building and Running

1. **Clone the repository**:

   ```bash
   git clone <repository-url>
   cd cbor_test
   ```

2. **Build the project**:

   ```bash
   cargo build
   ```

3. **Run the application**:

   You must provide the serial port name as a command-line argument:

   ```bash
   # Example on macOS/Linux
   cargo run -- /dev/tty.usbmodem1234561

   # Example on Windows
   cargo run -- COM5
   ```

   **Optional arguments** (provided in `key=value` format):

   - `debug=true`: Enable detailed debug logging
   - `show_byte=true`: Display raw bytes sent and received
   - `show_giga=true`: Show real-time messages from the Giga device
   - `show_giga_interval=0.1`: Set interval (seconds) for Giga message display
   - `timeout=0.001`: Set serial port read timeout in seconds

   **Example with multiple options**:

   ```bash
   cargo run -- /dev/tty.usbmodem1234561 debug=true show_byte=true show_giga=true
   ```

## Usage

Once the application is running, you can interact with it through the terminal.

### JSON Commands

Enter JSON strings that match the `MotorCommandParams` structure:

**Structure**:

- `action`: `"SEND"`, `"READ"`, or `"GIGA"`
- `cmd`: `"Motor"`, `"Sensor"`, `"File"`, `"Ack"`, or `"NAck"`
- `payload`: Object containing command-specific data

**Motor Control Example**:

```json
{"action": "SEND", "cmd": "Motor", "payload": {"PMt": {"id": 1, "motion": 1, "rpm": 500, "acc": 0, "volt": 0, "temp": 0, "amp": 0}}}
```

**Sensor Read Example**:

```json
{"action": "READ", "cmd": "Sensor", "payload": { "name": "SRc" }}
```

### Control Commands

- `q` or `/q`: Quit the application
- `r` or `/r`: Reconnect to the serial port
- `/t=N`: Generate and send a test command with `N` motor payloads

**Test Command Example**:

```bash
/t=3
```

This generates a test JSON with 3 motor payloads for quick testing.

### Real-time Monitoring

When `show_giga=true` is enabled, the application displays live sensor data from the Arduino device. Use `show_giga_interval` to control the display frequency.

## Project Structure

- `src/main.rs`: Main application logic including argument parsing, logging setup, and async communication loops
- `Cargo.toml`: Project dependencies and configuration
- `logs/`: Directory containing daily rolling log files (`cbor_test.log.YYYY-MM-DD`)
- `.github/copilot-instructions.md`: AI agent guidance for this codebase

### Key Dependencies

- **`pingpong-arduino`** (Git): Core communication protocol definitions, `Giga` struct, and device interaction logic
- **`tokio`**: Asynchronous runtime for concurrent operations
- **`serde_cbor`**: CBOR serialization for compact binary protocol
- **`cobs`**: Consistent Overhead Byte Stuffing for reliable serial framing
- **`tracing`**: Structured logging with console and file output
- **`serialport`**: Cross-platform serial port communication

## Troubleshooting

### Common Issues

1. **Serial Port Access**: Ensure you have permission to access the serial port
2. **Device Not Found**: Check the serial port name and that the Arduino is connected
3. **Connection Lost**: Use `/r` command to reconnect, or restart the application
4. **Invalid JSON**: Verify JSON format matches `MotorCommandParams` structure

### Logging

- Console logs show real-time application status
- File logs in `logs/` directory provide detailed history for debugging
- Use `debug=true` for verbose logging during development
- Use `show_byte=true` to inspect raw protocol data

### Performance Tips

- Adjust `timeout` parameter for slower devices
- Use `show_giga_interval` to control sensor data display frequency
- Monitor log file sizes in production environments
