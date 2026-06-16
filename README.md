# clickup-rui

A terminal user interface (TUI) for ClickUp built with Rust and [Ratatui](https://ratatui.rs/).

## Prerequisites & System Dependencies

To build `clickup-rui` from source, you will need the Rust toolchain installed. Additionally, the project relies on dependencies that require OpenSSL and pkg-config.

### Linux Packages Needed

Ensure the following packages are installed on your Linux system before building:

- **Debian / Ubuntu / Pop!_OS:**
  ```bash
  sudo apt update
  sudo apt install libssl-dev pkg-config
  ```


## Getting Started

### Building the Project

To compile the application in release mode:

```bash
cargo build --release
```

The compiled binary will be located at `target/release/clickup-rui`.

### Running the Application

To run the application directly using Cargo:

```bash
cargo run
```

### Running Tests

To run the project's test suite:

```bash
cargo test
```
