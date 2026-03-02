<p align="center">
  <img src="assets/icons/logo-primary-dark.svg" alt="Codirigent Logo" width="128" height="128" />
</p>

<h1 align="center">Codirigent</h1>

<p align="center">
  <strong>AI Coding Agent Orchestration IDE</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-alpha-orange" alt="Project Status" />
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version" />
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License" />
  <img src="https://img.shields.io/badge/rust-1.75%2B-red" alt="Rust Version" />
</p>

---

Codirigent is a Rust-based development environment for orchestrating AI coding sessions. It combines terminal sessions, clipboard state, task flow, and project navigation in one workspace.

## Features

- **Smart Clipboard Integration**: Unified monitoring across Windows and macOS with support for text and image buffers (PNG, JPEG, TIFF, BMP).
- **Advanced Session Management**: Persistent tracking of terminal sessions with deep process tree analysis and shell detection.
- **Integrated File Navigation**: High-performance file tree visualization for rapid project exploration.
- **Multi-AI Assistant Integration**: Native session readers for Claude Code, Codex, and Gemini with direct JSONL log access.
- **Task Management** (Experimental): Task scheduling and tracking with dependency support.
- **Workspace Management**: Customizable layout profiles (2x2, 1x4, 2x3, 3x3, single) with theme and keybinding management.
- **OSC Sequence Support**: OSC 133 for semantic prompts and OSC 7 for directory tracking in terminal sessions.
- **Agent-Ready Architecture**: Designed to support AI coding agents with reliable PTY and state management.

## Performance Focus

Codirigent is designed for low overhead and responsive interaction. Current optimizations include:

- **Asynchronous Event-Driven Design**: Built on a high-throughput internal event bus for decoupled component communication.
- **Efficient PTY Handling**: Uses `portable-pty` for low-latency terminal emulation.
- **Asynchronous I/O**: Leverages `tokio` for concurrent operations.
- **Memory Safety**: Native Rust execution without garbage collection pauses.

## Quick Start

### Prerequisites

- **Rust**: Latest stable toolchain (1.75+)
- **OS**: Windows or macOS (Linux support is not yet complete)

### Installation

```bash
# Clone the repository
git clone https://github.com/oso95/Codirigent.git
cd Codirigent

# Run with GPUI interface
cargo run --features gpui-full

# Optional: run with GPUI + terminal feature explicitly
cargo run --features "gpui-full,terminal"
```

**Note**: Running without the `gpui-full` feature logs startup info and exits.

## Project Structure

The codebase is organized into modular crates:

- `codirigent-core`: Fundamental data structures, configuration, shared traits, and event bus architecture.
- `codirigent-session`: PTY management, shell detection, and session lifecycle.
- `codirigent-detector`: Process and input monitoring (Windows/macOS).
- `codirigent-ui`: GPUI-based interface with terminal rendering and platform-specific clipboard integration.
- `codirigent-filetree`: File system traversal and tree display logic.
- `codirigent-verification`: Infrastructure for running verification checks on completed tasks.
- `codirigent-plugin`: Plugin infrastructure (built-in plugin lifecycle exists; external dynamic loading is not yet implemented).

## Development

### Testing

```bash
# Run all workspace tests (matches CI)
cargo test --all --all-targets

# Test a specific component
cargo test -p codirigent-session
```

### Quality Control

```bash
# Formatting (matches CI)
cargo fmt --all -- --check

# Linting (matches CI)
cargo clippy --all --all-targets -- -D warnings
```

## Documentation

- Architecture overview: `docs/architecture/overview.md`
- Data flow: `docs/architecture/data-flow.md`

## Contributing

We welcome contributions. For major changes, please open an issue first to discuss the proposal.

1. Fork the project
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'feat: add some amazing feature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a pull request

## License

Codirigent is distributed under the terms of the GNU General Public License v3.0.

See [LICENSE](LICENSE) for details.

---

<p align="center">
  Built by the Codirigent Contributors
</p>
