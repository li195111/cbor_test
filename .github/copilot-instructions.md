# cbor_test Copilot Instructions

This document provides essential knowledge for AI agents to be productive in the `cbor_test` codebase.

## Architecture Overview

This project is a `tokio`-based Rust command-line application designed to test and interact with an Arduino Giga hardware device over a serial port. It serves as a communication bridge, accepting user commands in JSON format via standard input and translating them into a binary protocol for the hardware.

The core communication protocol stack is as follows:
1.  **JSON (stdin)**: The user provides commands as JSON strings.
2.  **Rust Structs**: The JSON is deserialized into Rust structs (e.g., `MotorCommandParams`).
3.  **CBOR Serialization**: These structs are then serialized into the Concise Binary Object Representation (CBOR) format for a compact binary payload.
4.  **COBS Framing**: The CBOR payload is wrapped in a Consistent Overhead Byte Stuffing (COBS) frame to ensure reliable data transmission over the serial line.

The application is built around these key components:
-   `src/main.rs`: The main application entry point. It handles command-line argument parsing, sets up `tracing` for logging, and manages the main user input loop and a background task for serial communication.
-   `pingpong-arduino` (git dependency): This is a critical external crate from GitHub that defines the core data structures (`Action`, `Command`, `StateMessage`) and the main `Giga` struct which encapsulates all the logic for connecting to, sending data to, and receiving data from the serial device.
-   **Asynchronous Tasks**: The application uses `tokio` to manage concurrency. A main task handles user input from the console, while a background task manages the persistent serial port connection, listening for incoming data and sending outgoing commands. Communication between these tasks is handled by `tokio::sync::mpsc` channels.

## Developer Workflow

### Build & Run

The project is a standard Cargo project.

1.  **Build**:
    ```sh
    cargo build
    ```

2.  **Run**:
    The application requires the name of the serial port as a command-line argument. Optional arguments can be provided in `key=value` format.

    ```sh
    # Run with default settings on a specific port
    cargo run -- /dev/tty.usbmodem1234561

    # Run in debug mode, showing raw byte traffic
    cargo run -- /dev/tty.usbmodem1234561 debug=true show_byte=true
    ```

    -   **Positional Argument 1**: The serial port name (e.g., `COM5` on Windows, `/dev/tty.usbmodem...` on macOS/Linux).
    -   **Keyword Arguments**:
        -   `debug=true`: Enables verbose debug logging.
        -   `show_byte=true`: Logs the raw bytes being sent and received.
        -   `show_giga=true`: Toggles the display of real-time sensor messages from the Giga.
        -   `show_giga_interval=0.1`: Sets the interval (in seconds) for displaying Giga messages.
        -   `timeout=0.001`: Sets the serial port read timeout in seconds.

### Interacting with the Application

Once running, the application accepts commands via standard input:
-   **JSON Commands**: Paste a JSON object representing a `MotorCommandParams` struct to send a command to the device.
    -   Example: `{"action": "SEND", "cmd": "Motor", "payload": {"PMt": { "id": 1, "motion": 1, "rpm": 500, "acc": 0, "volt": 0, "temp": 0, "amp": 0 }}}`
-   **Control Commands**:
    -   `q` or `/q`: Quit the application.
    -   `r` or `/r`: Force a reconnection to the serial port.
    -   `/t=N`: Generate and send a test JSON command with `N` motor payloads.

### Logging

The application uses the `tracing` crate for structured logging. Logs are output to both the console and a rolling daily log file stored in the `logs/` directory (e.g., `logs/cbor_test.log.YYYY-MM-DD`). This is useful for debugging issues after a session has ended.

## Key Implementation Details

### Command Structure

The `MotorCommandParams` struct in `src/main.rs` defines the JSON command interface:
- `action`: Can be `SEND`, `READ`, or `GIGA` (from `pingpong-arduino` crate)
- `cmd`: Can be `Motor`, `Sensor`, `File`, `Ack`, or `NAck`
- `payload`: Either `Set` (HashMap with motor parameters) or `Read` (HashMap with requested values)

### Error Handling & Connection Management

The application implements robust error handling:
- Automatic reconnection on serial port failures
- Background task isolation prevents UI blocking
- Graceful degradation when device disconnects

### Performance Considerations

- The app uses `Arc<AtomicBool>` for thread-safe connection state tracking
- COBS framing ensures data integrity over unreliable serial connections
- Configurable timeouts prevent blocking on slow devices

## Key Files & Dependencies

-   `src/main.rs`: Contains the application's primary logic, including the user input loop and the background task for serial communication.
-   `Cargo.toml`: Defines all dependencies. Note the Git dependency on `pingpong-arduino`, which is essential for understanding the communication protocol.
-   `pingpong-arduino` crate: This crate from GitHub is fundamental. It defines the `Giga` struct, which is the heart of the hardware communication, as well as the data structures for `Action`, `Command`, and message payloads. Any changes to the communication protocol will likely involve this crate.
