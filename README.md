# focustime

[![Rust CI](https://github.com/utilForever/focustime/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/utilForever/focustime/actions/workflows/rust.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

TUI-based application for **Pomodoro timing**, **distraction-site blocking**, and **WakaTime tracking**.

<table>
  <tr>
    <td align="center">
      <img src="./assets/demo_focus.png" alt="Focus mode demo" width="800">
      <p>Pomodoro - Focus</p>
    </td>
    <td align="center">
      <img src="./assets/demo_short_break.png" alt="Short break demo" width="800">
      <p>Pomodoro - Short Break</p>
    </td>
  </tr>
  <tr>
    <td align="center">
      <img src="./assets/demo_site_blocking_inactive.png" alt="Site blocking inactive demo" width="800">
      <p>Site blocking - Inactive</p>
    </td>
    <td align="center">
      <img src="./assets/demo_site_blocking_active.png" alt="Site blocking active demo" width="800">
      <p>Site blocking - Active</p>
    </td>
</table>

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

> Site blocking updates your OS hosts file and may require elevated privileges
> (`sudo`/Administrator). If permissions are insufficient, timer functionality
> still works, but blocking operations can fail.

### Development checks

```sh
cargo check --all
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

## The way the system works

`focustime` is a single-binary Rust TUI app composed of six modules in `src/`:

- `src/main.rs`: terminal lifecycle and event loop.
- `src/app.rs`: application state and orchestration.
- `src/timer.rs`: Pomodoro timer state machine.
- `src/blocker.rs`: hosts-file site blocking and unblocking.
- `src/wakatime.rs`: heartbeat tracking integration.
- `src/ui.rs`: Ratatui rendering for timer and site manager views.

WakaTime tracking is optional and activates only when an API key is configured
(read from `~/.wakatime.cfg`).

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

## Release automation

Pushing a tag that matches `v*` (for example, `v0.2.0`) triggers the release
workflow. It runs CI quality gates (`check`, `fmt`, `clippy`, `test`, dependency
`audit`, and `typos`), builds binaries for Linux/macOS/Windows, and publishes
them to the GitHub Release attached to that tag.

## License

<img align="right" src="https://149753425.v2.pressablecdn.com/wp-content/uploads/2009/06/OSIApproved_100X125.png">

The class is licensed under the [MIT License](http://opensource.org/licenses/MIT):

Copyright &copy; 2026 [Chris Ohk](http://www.github.com/utilForever).

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
