<p align="center">
  <img src="assets/icons/logo-primary-dark.svg" alt="Codirigent Logo" width="128" height="128" />
</p>

<h1 align="center">Codirigent</h1>

<p align="center">
  <strong>A terminal workspace for running multiple AI coding agents in parallel</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-alpha-orange" alt="Project Status" />
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version" />
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License" />
  <img src="https://img.shields.io/badge/rust-1.75%2B-red" alt="Rust Version" />
</p>

---

If you're running Claude Code, Codex, or Gemini across multiple projects at the same time, you know the pain: opening terminals, `cd`-ing into repos, arranging windows, losing track of which agent is doing what.

Codirigent is a Tmux-style workspace built for this workflow. Open it once, and your sessions are already where you left them — right directory, right layout, right agent.

## Why Codirigent

- **Stop reopening terminals** — sessions persist across restarts with their working directories
- **See everything at once** — grid layouts show all agents simultaneously
- **Know what each agent is doing** — status indicators (Working / Needs Attention / Idle) per session
- **Paste images directly** — clipboard screenshots go straight into Claude Code as file references
- **Switch projects instantly** — file tree syncs to whichever session is focused
- **Works across CLIs** — Claude Code, Codex, and Gemini in the same workspace

## Quick Start

**Prerequisites:** Rust 1.75+, Windows or macOS

```bash
git clone https://github.com/oso95/Codirigent.git
cd Codirigent
cargo run --all-features
```

> Linux support is not yet complete.

## Development

```bash
cargo test --all --all-targets        # run tests
cargo fmt --all                       # format
cargo clippy --all -- -D warnings     # lint
```

## Contributing

Open an issue before major changes. PRs welcome.

## License

GPL-3.0 — see [LICENSE](LICENSE).
