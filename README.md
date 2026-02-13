<p align="center">
  <img src="assets/icons/logo-primary-dark.svg" alt="Codirigent Logo" width="128" height="128" />
</p>

<h1 align="center">Codirigent</h1>

<p align="center">
  <strong>An Intelligent Terminal-Based Development Environment</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-alpha-orange" alt="Project Status" />
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version" />
  <img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-green" alt="License" />
  <img src="https://img.shields.io/badge/rust-1.75%2B-red" alt="Rust Version" />
</p>

---

Codirigent is a terminal-based development environment (TDE) designed for the age of AI-assisted coding. It seamlessly orchestrates terminal sessions, clipboard state, and file navigation into a unified workflow, providing a robust foundation for both manual development and AI agent integration.

## Features

- **Smart Clipboard Integration**: Unified monitoring across Windows and macOS with support for text and image buffers.
- **Advanced Session Management**: Persistent tracking of terminal sessions with deep process tree analysis.
- **Integrated File Navigation**: High-performance file tree visualization for rapid project exploration.
- **Agent-Ready Architecture**: Built from the ground up to support AI coding agents with reliable PTY and state management.
- **Performance First**: Written in Rust for minimal latency and memory footprint.

## Performance & Resource Usage

Codirigent is engineered for high-performance orchestration with minimal overhead. By leveraging Rust's zero-cost abstractions, it maintains a significantly lower resource footprint compared to Electron-based alternatives.

| Metric | Codirigent | Electron-based IDEs |
| :--- | :--- | :--- |
| **Startup Time** | < 200ms | 2s - 5s |
| **Idle Memory** | ~45 MB | 400 MB - 800 MB |
| **Binary Size** | ~15 MB | 150 MB - 300 MB |
| **Architecture** | Native Rust | Node.js + Chromium |

### Key Optimization Pillars
- **Zero-Copy Event Bus**: Internal communication uses `Arc<T>` to avoid expensive data cloning between services.
- **Asynchronous I/O**: Built on `tokio`, allowing thousands of concurrent terminal operations with minimal thread context switching.
- **Memory Safety**: No garbage collector pauses, ensuring consistent latency for real-time terminal interactions.

## Quick Start

### Prerequisites

- **Rust**: Latest stable toolchain (1.75+)
- **OS**: Windows or macOS (Linux support coming soon)

### Installation

```bash
# Clone the repository
git clone https://github.com/user/codirigent.git
cd codirigent

# Build and run (Terminal mode)
cargo run --release
```

### GUI Features (Experimental)

On macOS, you can enable the experimental GPUI-based interface:

```bash
cargo run --release --features gpui-full
```

## Project Structure

The codebase is organized into highly modular crates to ensure separation of concerns:

- `codirigent-core`: Fundamental data structures, configuration, and shared traits.
- `codirigent-session`: PTY management, shell detection, and session lifecycle.
- `codirigent-detector`: Intelligent monitoring of system and process events.
- `codirigent-ui`: Cross-platform UI components (both TUI and experimental GUI).
- `codirigent-filetree`: Specialized logic for efficient file system traversal and display.

## Development

### Testing

We maintain high testing standards across all platforms:

```bash
# Run all workspace tests
cargo test

# Test a specific component
cargo test -p codirigent-session
```

### Quality Control

```bash
# Linting
cargo clippy --workspace -- -D warnings

# Formatting
cargo fmt --all --check
```

## Contributing

We welcome contributions! Please feel free to open issues or submit pull requests. For major changes, please open an issue first to discuss what you would like to change.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'feat: add some amazing feature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## License

Codirigent is distributed under the terms of both the MIT License and the Apache License (Version 2.0).

See [LICENSE](LICENSE) for details.

---

<p align="center">
  Built with ❤️ by the Codirigent Contributors
</p>
