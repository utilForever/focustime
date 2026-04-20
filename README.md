# focustime

[![Rust CI](https://github.com/utilForever/focustime/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/utilForever/focustime/actions/workflows/rust.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/utilForever/focustime)](https://github.com/utilForever/focustime/releases)
[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=utilForever_focustime&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=utilForever_focustime)
[![Lines of Code](https://sonarcloud.io/api/project_badges/measure?project=utilForever_focustime&metric=ncloc)](https://sonarcloud.io/summary/new_code?id=utilForever_focustime)

[![Maintainability Rating](https://sonarcloud.io/api/project_badges/measure?project=utilForever_focustime&metric=sqale_rating)](https://sonarcloud.io/summary/new_code?id=utilForever_focustime)
[![Reliability Rating](https://sonarcloud.io/api/project_badges/measure?project=utilForever_focustime&metric=reliability_rating)](https://sonarcloud.io/summary/new_code?id=utilForever_focustime)
[![Security Rating](https://sonarcloud.io/api/project_badges/measure?project=utilForever_focustime&metric=security_rating)](https://sonarcloud.io/summary/new_code?id=utilForever_focustime)
[![Technical Debt](https://sonarcloud.io/api/project_badges/measure?project=utilForever_focustime&metric=sqale_index)](https://sonarcloud.io/summary/new_code?id=utilForever_focustime)
[![Vulnerabilities](https://sonarcloud.io/api/project_badges/measure?project=utilForever_focustime&metric=vulnerabilities)](https://sonarcloud.io/summary/new_code?id=utilForever_focustime)

TUI-based application for **Pomodoro timing**, **distraction-site blocking**, and **WakaTime tracking**.

<table>
  <tr>
    <td align="center">
      <img src="./assets/demo_focus.png" alt="Focus mode demo" width="600">
      <p>Pomodoro - Focus</p>
    </td>
    <td align="center">
      <img src="./assets/demo_short_break.png" alt="Short break demo" width="600">
      <p>Pomodoro - Short Break</p>
    </td>
    <td align="center">
      <img src="./assets/demo_long_break.png" alt="Long break demo" width="600">
      <p>Pomodoro - Long Break</p>
    </td>
  </tr>
  <tr>
    <td align="center">
      <img src="./assets/demo_site_blocking_inactive.png" alt="Site blocking inactive demo" width="600">
      <p>Site Blocking - Inactive</p>
    </td>
    <td align="center">
      <img src="./assets/demo_site_blocking_active.png" alt="Site blocking active demo" width="600">
      <p>Site Blocking - Active</p>
    </td>
    <td align="center">
      <img src="./assets/demo_session_planner.png" alt="Session planner demo" width="600">
      <p>Session Planner</p>
    </td>
  </tr>
  <tr>
    <td align="center">
      <img src="./assets/demo_history.png" alt="History demo" width="600">
      <p>Daily Focus History</p>
    </td>
    <td align="center">
      <img src="./assets/demo_profiles.png" alt="Profiles demo" width="600">
      <p>Pomodoro Profiles</p>
    </td>
    <td align="center">
      <img src="./assets/demo_diagnostics.png" alt="Setup diagnostics demo" width="600">
      <p>Setup Diagnostics</p>
    </td>
  </tr>
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

### SonarCloud setup

Static analysis runs in `.github/workflows/sonarcloud.yml` for pushes and pull
requests to `main`.

1. Import `utilForever/focustime` into SonarCloud.
2. Confirm `sonar.projectKey` and `sonar.organization` in
   `sonar-project.properties` match your SonarCloud project.
3. Add `SONAR_TOKEN` as a repository secret in GitHub.

> For pull requests from forks, the SonarCloud job is skipped because
> repository secrets are not available in untrusted fork contexts.

## Pomodoro profiles

`focustime` now supports selectable Pomodoro profiles:

- **Classic** (25/5/15, long break every 4 focus sessions)
- **Deep Work** (50/10/30, long break every 3 focus sessions)
- **Custom** (editable in-app)

Open profile manager from timer view with **`p`**.

- `↑/↓`: move between profiles
- `Enter`: apply selected profile
- `e`: open profile/settings editor
- In editor: `↑/↓` selects field, `←/→` adjusts numeric/boolean values, `Type/Backspace` edits WakaTime project/language, `Enter` saves

Profile selection and custom values are persisted in `config.toml`.

## Session planner

Open the session planner from timer view with **`t`**.

- `a`: add a new task label
- `↑/↓`: move selection
- `Enter`: select highlighted task label
- `t` or `Esc`: return to timer view
- while adding a label, `Enter` saves and `Esc` cancels

Starting a focus session from idle now requires a selected task label. The timer
view always shows the current task label (or a reminder to select one).

### Example config

```toml
selected_profile = "custom"
selected_blocklist_profile = "Work"
strict_mode = false
break_glass_duration_secs = 300

[[blocklist_profiles]]
name = "Work"
sites = ["youtube.com", "reddit.com"]

[[blocklist_profiles]]
name = "Study"
sites = ["x.com", "news.ycombinator.com"]

[custom_profile]
focus_secs = 1800
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3

[notifications]
enabled = true
sound = false

[auto_start]
focus_to_break = false
break_to_focus = false

[daily_goal]
minutes = 120
pomodoros = 4

[wakatime]
project = "focustime"
language = "Pomodoro"
```

`[wakatime]` is optional. If omitted (or set to blank values), `focustime` uses
the defaults above for heartbeat metadata labels.

## Site manager workflow

Open the site manager from timer view with **`b`**.

- `a`: add/import hostnames
- `e`: edit the selected hostname
- `d` or `Delete`: remove the selected hostname
- `[` / `]`: switch active blocklist profile
- `n`: create a blocklist profile
- `r`: rename the active blocklist profile
- `x`: delete the active blocklist profile
- `↑/↓`: move selection
- `b`: return to timer view
- `Esc`: return to timer view only when add/edit mode is not active

Add/import input supports:

- single hostnames (`youtube.com`)
- comma-separated lists (`youtube.com, reddit.com`)
- newline-separated lists (paste multi-line blocklists, then press `Enter`)
- while add/import or edit mode is active, `Enter` commits and `Esc` cancels the current draft

Invalid and duplicate entries are reported inline so you can fix them without leaving the view.

## Setup diagnostics

Open the setup diagnostics screen from timer view with **`d`**.

- `r`: refresh diagnostics checks
- `d` or `Esc`: return to timer view

The diagnostics screen reports:

- blocking permissions
- hosts file write capability
- WakaTime config status (`~/.wakatime.cfg` and `api_key` availability)

## Phase notifications

`focustime` emits a phase notification when a phase naturally completes at `00:00`:

- **Focus complete** → next break phase
- **Break complete** → focus phase

Manual skip (`n`) changes phase immediately but does not emit a completion notification.

Notifications are delivered best-effort:

- terminal notice in the timer view
- desktop notification via platform-specific delivery (`winrt-toast-reborn` toast on Windows with a `msg` fallback, `osascript` on macOS, `notify-send` on Linux)
- optional sound alert using platform audio capabilities when `notifications.sound = true`

Natural, non-catchup phase transitions can also auto-start the next timer with safe defaults (`Off`):

- `auto_start.focus_to_break` starts break timers automatically after focus completion on non-catchup ticks
- `auto_start.break_to_focus` starts focus timers automatically after break completion on non-catchup ticks

You can configure notification and auto-start settings directly from the TUI:

- open profile manager with `p`
- press `e` to open the editor
- use `↑/↓` to select **Phase notifications**, **Sound alert**, **Auto-start break**, **Auto-start focus**, **Strict focus mode**, **Daily goal (minutes)**, **Daily goal (pomodoros)**, **WakaTime project**, or **WakaTime language**
- use `←/→` to adjust values (or toggle `Off`/`On` for boolean fields), use `Type/Backspace` for WakaTime text fields, then `Enter` to save

## Strict focus mode

`focustime` provides an optional strict mode (`strict_mode = false` by default).

When strict mode is enabled during an active focus session:

- `n` (skip phase) is disabled
- `s` (stop/reset) requires confirmation by pressing `s` again
- `p` (profile manager) is disabled, so profile switching is locked
- quit shortcuts (`q`, `Esc`, `Ctrl-C`) are disabled until focus is no longer active

## Break-glass override for site blocking

During an active focus session, you can temporarily pause site blocking with an explicit two-step confirm:

- press `u` to arm break-glass
- press `u` again to confirm and temporarily unblock

While active, timer status shows a live countdown. When the countdown expires, blocking resumes automatically if focus is still active.

Override events are recorded for audit visibility in the History view and included in export metadata (`focustime-stats.json` and `focustime-stats.csv`).

## Session stats and history

`focustime` tracks:

- completed pomodoros for the current app session
- focused minutes for the current app session
- daily aggregates persisted in `stats.toml` (in the same config directory as `config.toml`)
- weekly totals derived from daily aggregates in the History view
- current streak and best streak based on completed daily goals

If a daily goal is configured, timer and history views also show live progress for
both:

- target focused minutes
- target completed pomodoros

Streaks are evaluated against the goal that was active on each day. Changing the
daily goal later does not rewrite older tracked days, and streak tracking stays
inactive when today's goal is off.

From timer view:

- press **`h`** to open the history panel with weekly and daily summaries
- while the history panel is open, press **`e`** to export `focustime-stats.json` and `focustime-stats.csv` into the current working directory
- press **`h`** or **`Esc`** to return to timer view

Exports include daily/weekly aggregates plus labeled focus-session records where
task labels were attached. Focus-session rows also include `focus_intention` and
`task_note` fields (mirroring `task_label`) for downstream consumers.

## The way the system works

`focustime` is a single-binary Rust TUI app composed of seven modules in `src/`:

- `src/main.rs`: terminal lifecycle and event loop.
- `src/app.rs`: application state and orchestration.
- `src/timer.rs`: Pomodoro timer state machine.
- `src/blocker.rs`: hosts-file site blocking and unblocking.
- `src/wakatime.rs`: heartbeat tracking integration.
- `src/notifications.rs`: phase transition notifications and optional sound.
- `src/ui.rs`: Ratatui rendering for timer, session planner, site manager, profile, history, and setup diagnostics views.

WakaTime tracking is optional and activates only when an API key is configured
(read from `~/.wakatime.cfg`).

Runtime flow (high-level):

1. The main loop renders UI and reads keyboard input.
2. `App` handles key events (`start/pause`, `stop`, `next`, session planner actions, site manager actions).
3. Timer ticks advance every elapsed second while running.
4. Phase-completion notifications are dispatched asynchronously.
5. Blocking is applied during focus phases and removed outside focus.
6. WakaTime tracking stays in sync with focus-running state and applies async
   heartbeat outcomes without blocking timer flow.

### WakaTime reliability behavior

When WakaTime is configured, heartbeats are still best-effort and non-blocking.
The timer never waits on network calls.

- transient heartbeat failures (`429`, `5xx`, and connectivity/timeout errors)
  retry with bounded backoff (`1s`, then `2s`)
- non-retryable failures are surfaced in the timer view status line
- status line reflects runtime states (`tracking`, `sending`, `retrying`,
  `error`, `idle`, `not configured`) and, when configured, also shows the last
  successful heartbeat time (`HH:MM:SS`) or `not yet sent` before first success

For full module map and design details, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) for:

- local quality checks
- coding and commit conventions
- pull request workflow

## Release automation

Pushing a tag that matches `v*` (for example, `v0.4.0`) triggers the release
workflow. It runs CI quality gates (`check`, `fmt`, `clippy`, `test`, dependency
`audit`, and `typos`), builds binaries for Linux/macOS/Windows, and publishes
them to the GitHub Release attached to that tag.

The latest stable release is [v0.4.0](https://github.com/utilForever/focustime/releases/tag/v0.4.0).

For a human-readable summary of notable changes in this release, see [CHANGELOG.md](CHANGELOG.md).

## License

<img align="right" src="https://149753425.v2.pressablecdn.com/wp-content/uploads/2009/06/OSIApproved_100X125.png">

The class is licensed under the [MIT License](https://opensource.org/licenses/MIT):

Copyright &copy; 2026 [Chris Ohk](https://www.github.com/utilForever).

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
