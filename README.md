# cbor_test

`cbor_test` is a Rust command-line application for testing and interacting with an Arduino Giga device over a serial port. It sends and receives data using a CBOR (Concise Binary Object Representation) and COBS (Consistent Overhead Byte Stuffing) based protocol.

## Features

- **Serial Communication**: Connects to a specified serial port to communicate with the Arduino Giga.
- **CBOR/COBS Protocol**: Serializes commands into CBOR and frames them with COBS for reliable transmission.
- **Interactive Command-Line**: Accepts JSON-formatted commands from the user via standard input.
- **Asynchronous I/O**: Uses `tokio` for non-blocking serial port communication and user input.
- **Structured Logging**: Employs the `tracing` library to log to both the console and daily rolling log files in the `logs/` directory.

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) toolchain
- An Arduino Giga device flashed with the corresponding firmware.
- The `pingpong-core` library, which is a local dependency. Ensure it is located at the path specified in `Cargo.toml` (`/Users/liyuefong/Desktop/JamesYeh/pingpong-core`).

### Building and Running

1. **Clone the repository**:

   ```sh
   git clone <repository-url>
   cd cbor_test
   ```

2. **Build the project**:

   ```sh
   cargo build
   ```

3. **Run the application**:
   You must provide the serial port name as a command-line argument.

   ```sh
   # Example on macOS/Linux
   cargo run -- /dev/tty.usbmodem1234561

   # Example on Windows
   cargo run -- COM5
   ```

   You can also provide optional arguments:
   - `debug=true`: Enable detailed debug logging.
   - `show_byte=true`: Show raw bytes sent and received.
   - `show_giga=true`: Display real-time messages from the Giga.
   - `timeout=0.001`: Set the serial port read timeout in seconds.

   Example with optional arguments:

   ```sh
   cargo run -- /dev/tty.usbmodem1234561 debug=true show_byte=true
   ```

## Usage

Once the application is running, you can send commands through the terminal.

### Sending JSON Commands

Enter a JSON string that matches the `MotorCommandParams` structure.

**Example**:

```json
{"action": "SEND", "cmd": "Motor", "payload": {"PMt": {  "id": 1,  "motion": 1,  "rpm": 500,  "acc": 0,  "volt": 0,  "temp": 0,  "amp": 0 }}}
```

### Control Commands

- `q` or `/q`: Quit the application.
- `r` or `/r`: Reconnect to the serial port.
- `/t=N`: Generate and send a test command with `N` motor payloads. For example, `/t=5` will create a command with 5 motor entries.

## Project Structure

- `src/main.rs`: The main application logic, including argument parsing, logging setup, and the primary async loops for user input and device communication.
- `Cargo.toml`: Project dependencies, including `serde_cbor`, `serialport`, `tokio`, and the local `pingpong-core` crate.
- `logs/`: Directory where log files are stored.
- `pingpong-core` (External Dependency): This local crate contains the core definitions for the communication protocol, including the `Giga` struct and data structures like `Action`, `Command`, and `StateMessage`.
