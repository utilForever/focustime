# focustime

TUI-based application for **Pomodoro timing**, **distraction-site blocking**, and **WakaTime tracking**.

## Status badges

[![Rust CI](https://github.com/utilForever/focustime/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/utilForever/focustime/actions/workflows/rust.yml)
![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)

## Screenshot

> Screenshot will be added later.

## Quick Start

### Prerequisites

- Rust stable toolchain
- Git

### Build and run

```sh
git clone https://github.com/utilForever/focustime.git
cd focustime
cargo run
```

### Development checks

```sh
cargo check --all
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

## The way the system works

`focustime` is a single-binary Rust TUI app composed of six modules:

- `main.rs`: terminal lifecycle and event loop.
- `app.rs`: application state and orchestration.
- `timer.rs`: Pomodoro timer state machine.
- `blocker.rs`: hosts-file site blocking and unblocking.
- `wakatime.rs`: heartbeat tracking integration.
- `ui.rs`: Ratatui rendering for timer and site manager views.

Runtime flow (high-level):

1. The main loop renders UI and reads keyboard input.
2. `App` handles key events (`start/pause`, `stop`, `next`, site manager actions).
3. Timer ticks advance every elapsed second while running.
4. Blocking is applied during focus phases and removed outside focus.
5. WakaTime tracking stays in sync with focus-running state.

For full module map and design details, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) for:

- local quality checks
- coding and commit conventions
- pull request workflow

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE).
