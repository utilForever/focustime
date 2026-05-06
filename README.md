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
      <img src="./assets/demo_focus_history.png" alt="Focus history demo" width="600">
      <p>Focus History</p>
    </td>
    <td align="center">
      <img src="./assets/demo_pomodoro_profiles.png" alt="Pomodoro profiles demo" width="600">
      <p>Pomodoro Profiles</p>
    </td>
    <td align="center">
      <img src="./assets/demo_setup_diagnostics.png" alt="Setup diagnostics demo" width="600">
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

## CLI automation commands

`focustime` supports non-interactive CLI commands for scripting and automation.

```sh
# Launch TUI with focus timer already started
cargo run -- --start

# Control timer flow without entering TUI
cargo run -- --pause
cargo run -- --resume
cargo run -- --stop
cargo run -- --next --json

# Select task label (creates label if it does not exist yet)
cargo run -- --task "Write docs"
cargo run -- --task=Write-docs --json
# Archived labels are rejected by --task and cannot be used to start focus.

# Show or set cumulative goal targets for a task label
cargo run -- --task-goal "Write docs"
cargo run -- --task-goal "Write docs:120,4"
cargo run -- --task-goal=Write-docs:120,4 --json

# Show or set the active profile
cargo run -- --profile
cargo run -- --profile deep-work
cargo run -- --profile --json

# Show or set the active theme preset
cargo run -- --theme
cargo run -- --theme high-contrast
cargo run -- --theme deuteranopia-friendly --json

# Show or set daily goal targets (minutes,pomodoros)
cargo run -- --goal
cargo run -- --goal=120,4
cargo run -- --goal --json

# Show or set weekly goal targets (minutes,pomodoros)
cargo run -- --goal-weekly
cargo run -- --goal-weekly=600,20
cargo run -- --goal-weekly --json

# Show or set monthly goal targets (minutes,pomodoros)
cargo run -- --goal-monthly
cargo run -- --goal-monthly=2400,80
cargo run -- --goal-monthly --json

# Show or set goal carry-over behavior (on/off)
cargo run -- --goal-carry
cargo run -- --goal-carry=on
cargo run -- --goal-carry-weekly=on
cargo run -- --goal-carry-monthly=off
cargo run -- --goal-carry --json

# Show or set strict mode for the selected profile
cargo run -- --strict
cargo run -- --strict=on
cargo run -- --strict --json

# Manage blocklist profiles (active profile + CRUD)
cargo run -- --blocklist-profile
cargo run -- --blocklist-profile Work
cargo run -- --blocklist-profile-create Study
cargo run -- --blocklist-profile-rename "Deep Work"
cargo run -- --blocklist-profile-delete --json

# Manage blocklist/allowlist sites for the active blocklist profile
cargo run -- --blocklist-sites
cargo run -- --allowlist-sites --json
cargo run -- --blocklist-site-add="youtube.com, reddit.com"
cargo run -- --allowlist-site-add "reddit.com"
cargo run -- --blocklist-site-edit "youtube.com=news.ycombinator.com"
cargo run -- --allowlist-site-delete reddit.com

# Show/set schedule for the selected profile (including overlap/conflict inspection)
cargo run -- --schedule
cargo run -- --schedule-set='{"windows":[{"days":["mon","tue"],"start":"09:00","end":"11:00"}],"exception_dates":["2026-12-25"],"one_time_windows":[{"date":"2026-05-02","start":"14:00","end":"16:00"}]}'
cargo run -- --schedule --json

# Show setup diagnostics checks (including hosts and WakaTime readiness)
cargo run -- --diagnostics
cargo run -- --diagnostics --json

# Preview focustime-managed hosts-file changes without writing
cargo run -- --blocking-preview
cargo run -- --blocking-preview --json

# Show status (text or JSON, including live timer/session fields, latest interruption summary, and `selected_task_goal` in JSON)
cargo run -- --status
cargo run -- --status --json

# Watch status continuously (default 1s cadence, optional seconds override)
cargo run -- --status --watch
cargo run -- --status --watch=2 --json

# Back up config.toml and stats.toml to current directory or a target directory
cargo run -- --backup
cargo run -- --backup=./reports --json

# Restore config.toml and stats.toml from current directory or a source directory
# (restore requires both files to be present in the source directory)
cargo run -- --restore
cargo run -- --restore=./reports --json

# Export stats to current directory or a target directory
cargo run -- --export
cargo run -- --export=./reports --json
```

Backup/restore behavior:

- `--backup` creates the target directory if needed, then copies `config.toml` and `stats.toml` into it.
- `--restore` requires both files in the source directory and uses staged replacement so failed restores roll back to the original files.

### CLI JSON/error contract

- `--json` success responses are emitted to `stdout` as JSON and exit with code `0`.
- `--json` failures are emitted to `stdout` as JSON (no mixed human text) and exit with a non-zero code.
- `--status --watch --json` emits newline-delimited compact JSON snapshots continuously until interrupted.
- Text-mode failures are emitted to `stderr` for interactive readability.

Exit codes:

- `0`: success
- `1`: runtime/command failure
- `2`: argument/usage failure

JSON failure shape:

```json
{
  "ok": false,
  "error": {
    "kind": "usage",
    "exit_code": 2,
    "message": "Unknown option `--unknown`.\n\nUsage:\n..."
  }
}
```

When no CLI command is provided, `focustime` keeps the default interactive TUI mode.

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

## TUI layout and interaction model

The TUI keeps the same keyboard shortcuts, but uses a cleaner hierarchy and more
consistent screen structure:

- **Timer view** prioritizes phase/countdown/progress first, then shows grouped
  session context (task, profile, schedule, stats, WakaTime, strict/break-glass).
- **Manager/detail views** (sites, profiles, planner, history, diagnostics)
  follow a consistent pattern: context header, primary content block, feedback
  line, and compact command legend.
- **Profiles, planner, and history** are now laid out to fit narrower and
  shorter terminal windows than before.
- **Focus History** uses a redesigned structure with an overview block and
  dedicated task/trend/audit panels for faster scanning.
- **Command legends** use short grouped lines so common actions are easier to
  scan quickly during focus sessions.

## Keyboard shortcut customization

Core command shortcuts are configurable in `config.toml` under `[shortcuts]`.
This first pass covers command actions (timer controls, view switching, manager
actions, export/refresh, and quit).

For safety and editing ergonomics, these keys remain fixed in this version:

- `Enter`, `Esc`, arrow keys, `Delete`
- text-entry behavior (`Type`, `Backspace`, paste)
- `Ctrl-C` as a quit fallback

Example:

```toml
[shortcuts]
open_stats_history = "y"
open_session_planner = "g"
back_stats_history = "y"
timer_stop_reset = "x"
quit = "q"
timer_toggle_pause = "space"
```

## Pomodoro profiles

`focustime` now supports selectable Pomodoro profiles:

- **Classic** (25/5/15, long break every 4 focus sessions)
- **Deep Work** (50/10/30, long break every 3 focus sessions)
- **Custom** (editable in-app)

Open profile manager from timer view with **`p`**.

- `↑/↓`: move between profiles
- `Enter`: apply selected profile
- `e`: open profile/settings editor
- `[` / `]`: cycle the active break template for fast short/long break switching
- In editor: `↑/↓` selects field, `←/→` adjusts numeric/boolean values (including **Theme preset**), `Type/Backspace` edits WakaTime project/language, `Enter` saves

Profile selection, break template selection, theme preset selection, custom durations,
and profile-scoped automation settings are persisted in `config.toml`.

## Session planner

Open the session planner from timer view with **`t`**.

- `a`: add a new task label
- `e`: rename highlighted task label
- `f`: toggle favorite for highlighted task label (favorites are listed first)
- `x`: toggle archive state for highlighted task label
- `d` or `Delete`: delete highlighted task label
- `r` or `1-5`: quick-pick recent task labels
- `↑/↓`: move selection
- `Enter`: select highlighted task label (archived labels are visible but cannot be selected)
- `t` or `Esc`: return to timer view
- while adding/renaming a label, `Enter` saves and `Esc` cancels

Starting a focus session from idle now requires a selected task label. The timer
view always shows the current task label (or a reminder to select one).

### Mid-session notes

While a focus session is running or paused, press **`m`** in timer view to edit a
quick session note.

- type or paste note text
- `Enter`: save (replaces the previous note); if the draft is blank or only whitespace, the note is not saved and the task label is used instead
- `Esc`: cancel without changing the current note

Saved notes are reflected in live status metadata (`task_note`), recovery state,
and interruption/completed-session history export fields.

### Example config

```toml
selected_profile = "custom"
selected_break_template = "Classic"
selected_theme_preset = "classic"
selected_blocklist_profile = "Work"
# Legacy compatibility mirror for the selected profile's automation strict mode.
strict_mode = false
break_glass_duration_secs = 300

[shortcuts]
timer_toggle_pause = "space"
timer_stop_reset = "s"
open_session_planner = "t"
open_stats_history = "h"
quit = "q"

[[break_templates]]
name = "Classic"
short_break_secs = 300
long_break_secs = 900
long_break_interval = 4

[[break_templates]]
name = "Deep Work"
short_break_secs = 600
long_break_secs = 1800
long_break_interval = 3

[[blocklist_profiles]]
name = "Work"
sites = ["youtube.com", "reddit.com"]
allowlist_sites = ["reddit.com"]

[[blocklist_profiles]]
name = "Study"
sites = ["x.com", "news.ycombinator.com"]
allowlist_sites = []

[custom_profile]
focus_secs = 1800
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3

[profile_automation.custom.notifications]
enabled = true
sound = false

[profile_automation.custom.auto_start]
focus_to_break = false
break_to_focus = false

[profile_automation.custom.recurring_schedule]
exception_dates = ["2026-12-25", "2027-01-01"]
[[profile_automation.custom.recurring_schedule.windows]]
days = ["mon", "tue", "wed", "thu", "fri"]
start = "09:00"
end = "11:00"
[[profile_automation.custom.recurring_schedule.one_time_windows]]
date = "2026-05-02"
start = "14:00"
end = "16:00"

[daily_goal]
minutes = 120
pomodoros = 4

[weekly_goal]
minutes = 600
pomodoros = 20

[monthly_goal]
minutes = 2400
pomodoros = 80

[wakatime]
project = "focustime"
language = "Pomodoro"

[[wakatime.task_mappings]]
task_label = "Docs"
project = "Documentation"
language = "Markdown"

[[wakatime.task_mappings]]
task_label = "Review"
language = "Code Review"
```

`[wakatime]` is optional. If omitted (or set to blank values), `focustime` uses
the defaults above for heartbeat metadata labels.

`[[wakatime.task_mappings]]` is also optional. When present, focustime matches
the active task label case-insensitively and overrides WakaTime metadata per
field:

- `project`: task-mapped value if provided, otherwise `[wakatime].project`
- `language`: task-mapped value if provided, otherwise `[wakatime].language`

Mappings with blank `task_label` or blank override values are ignored. If
duplicate task labels are configured, the first valid mapping is used.

## Site manager workflow

Open the site manager from timer view with **`b`**.

- `a`: add/import hostnames
- `e`: edit the selected hostname
- `d` or `Delete`: remove the selected hostname
- `m`: toggle between editing blocklist sites and allowlist exceptions
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

Allowlist entries act as explicit exceptions: effective focus blocking is computed as **blocklist sites minus allowlist sites** for the active profile.

For hosts-based blocking to apply reliably, keep DNS-over-HTTPS disabled in your browser.

## Setup diagnostics

Open the setup diagnostics screen from timer view with **`d`**.

- `r`: refresh diagnostics checks
- `d` or `Esc`: return to timer view

The diagnostics screen reports:

- blocking permissions
- hosts file write capability
- blocking preview summary and focustime-managed hosts section
- remediation guidance when hosts permissions are insufficient
- WakaTime config status (`~/.wakatime.cfg` and `api_key` availability)

## Phase notifications

`focustime` emits a phase notification when a phase naturally completes at `00:00`:

- **Focus complete** → next break phase
- **Break complete** → focus phase

Manual skip (`n`) changes phase immediately but does not emit a completion notification.

Notifications are delivered best-effort:

- terminal notice in the timer view
- desktop notification via platform-specific delivery (`winrt-toast-reborn` toast on Windows with a `msg` fallback, `osascript` on macOS, `notify-send` on Linux)
- optional sound alert using platform audio capabilities when `profile_automation.<profile>.notifications.sound = true`

Natural, non-catchup phase transitions can also auto-start the next timer with safe defaults (`Off`):

- `profile_automation.<profile>.auto_start.focus_to_break` starts break timers automatically after focus completion on non-catchup ticks
- `profile_automation.<profile>.auto_start.break_to_focus` starts focus timers automatically after break completion on non-catchup ticks

Recurring schedule windows can also trigger focus behavior at wall-clock times:

- `profile_automation.<profile>.recurring_schedule.windows[].days` accepts day tokens (`mon`..`sun`, case-insensitive)
- `profile_automation.<profile>.recurring_schedule.windows[].start` / `end` use 24-hour `HH:MM` local time (`start < end`)
- `profile_automation.<profile>.recurring_schedule.exception_dates` accepts `YYYY-MM-DD` local dates and skips automatic schedule triggering on those days
- `profile_automation.<profile>.recurring_schedule.one_time_windows[]` accepts one-time date windows with `date` (`YYYY-MM-DD`) plus `start`/`end` (`HH:MM`)
- when a window begins, focus auto-starts if possible; otherwise schedule mode arms and shows a reminder until you manually start focus
- while a schedule window is active and focus is not already running, press `z` to delay the scheduled start by 10 minutes
- recurring exception dates only skip recurring windows; one-time windows still apply on their configured date
- if multiple windows overlap, the most recently started active window takes precedence; windows with the same start time are resolved deterministically
- `--schedule` (text and JSON) reports detected schedule conflicts/overlaps without rejecting the schedule
- the timer session overview shows the current/next scheduled window

You can configure notification and auto-start settings directly from the TUI:

- open profile manager with `p`
- press `e` to open the editor
- automation and schedule edits apply to the currently selected profile only
- the editor is grouped into sections (**Timer**, **Automation**, **Goals**, **Appearance**, **WakaTime**, **Schedule**) to keep settings easier to scan
- use `↑/↓` to select **Phase notifications**, **Sound alert**, **Auto-start break**, **Auto-start focus**, **Strict focus mode**, **Daily/Weekly/Monthly goal (minutes)**, **Daily/Weekly/Monthly goal (pomodoros)**, **Theme preset**, **WakaTime project/language**, or the **Schedule** fields
- use `←/→` to adjust values (or toggle `Off`/`On` for boolean fields), use `Type/Backspace` for WakaTime text fields, then `Enter` to save
- schedule editing is in-app:
  - **Schedule add/remove**: `→` adds a window, `←` removes selected window
  - **Schedule window**: `←/→` changes which window is selected
  - **Schedule day** + **Schedule day enabled**: choose day cursor and toggle it `Off/On`
  - **Schedule start/end**: adjust times in 15-minute steps
  - **Schedule exception**: `←/→` changes which exception date is selected
  - **Exception date**: `←/→` moves selected exception date backward/forward by 1 day
  - **Exception add/remove**: `→` adds a date (starting from today), `←` removes selected date
  - **One-time window**: `←/→` changes which one-time window is selected
  - **One-time date**: `←/→` moves selected one-time window date backward/forward by 1 day
  - **One-time start/end**: adjust one-time window times in 15-minute steps
  - **One-time add/remove**: `→` adds a one-time window (starting from today), `←` removes selected window
  - **Conflict inspector**: read-only summary of detected schedule overlaps/conflicts

## Session recovery

`focustime` persists in-progress timer sessions so restart/crash recovery can resume where you left off.

- while a focus/break phase is running or paused, the app saves phase, remaining time, task metadata (`task_label`, `focus_intention`, `task_note`), and active profile
- while editing with `m`, pressing `Enter` replaces the in-progress session `task_note` and immediately syncs recovery metadata; if the draft is blank or only whitespace, the note is not saved and the task label is used instead
- on startup, valid in-progress state is restored and shown in the timer notice line
- on startup, blocking is reconciled with recovered timer state: recovered active focus re-applies blocking, while non-recovered startup attempts to remove stale crash-era block entries
- stale or invalid saved recovery state is ignored safely with a warning notice
- recovery state is cleared when an in-progress phase ends naturally or when you reset/skip out of the active session

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
- weekly consistency score (`active_days / 7`, rounded to `%`) derived from daily activity
- weekly focus score KPI (50/50 blend of consistency and weekly goal completion; `n/a` when weekly goal is off)
- profile effectiveness comparison (focus share % and average focused minutes per completed session)
- per-task totals (pomodoros and focused minutes) derived from labeled focus sessions
- per-task trend summaries in History (`last 7 days` vs `previous 7 days`)
- per-task cumulative goals (minutes/pomodoros) with per-label progress and met/in-progress evaluation
- structured interruption events for manual `stop/reset` and `skip/next` actions
- current streak and best streak based on completed daily goals

If daily, weekly, or monthly goals are configured, timer and history views also
show live progress for each period:

- target focused minutes
- target completed pomodoros

Streaks are evaluated against the goal that was active on each day. Changing the
daily goal later does not rewrite older tracked days, and streak tracking stays
inactive when today's goal is off.

From timer view:

- press **`h`** to open the history panel with weekly and daily summaries
- while the history panel is open, press **`e`** to export `focustime-stats.json` and `focustime-stats.csv` into the current working directory
- press **`h`** or **`Esc`** to return to timer view

Exports include daily/weekly aggregates, weekly consistency, weekly focus score,
profile effectiveness, task summaries/trends, interruption records, and labeled
focus-session records where task labels were attached. Focus-session rows
persist and export first-class `focus_intention` and `task_note` fields; when
dedicated metadata input is not provided, both fields default to the selected
`task_label`. Interruption records include structured `reason` values and
remaining-time metadata. Export files expose `schema_version` (currently `5`)
so downstream consumers can handle versioned contracts explicitly.

## The way the system works

`focustime` is a single-binary Rust app organized around top-level facade modules
with focused submodules (updated in #240):

- `src/main.rs`: composition root, CLI/TUI dispatch, terminal lifecycle, and event loop.
- `src/app.rs` + `src/app/*.rs`: runtime state/orchestration split by domain (timer flow, planner, profiles, site manager, schedule, persistence, diagnostics, CLI API).
- `src/cli.rs` + `src/cli/*.rs`: CLI args/parsing/execution/status/output pipeline.
- `src/stats.rs` + `src/stats/*.rs`: stats persistence, analytics, trends, recording, planner state, and exports.
- `src/ui.rs` + `src/ui/*.rs`: Ratatui rendering split by screen (timer, session planner, site manager, profile manager, history, setup diagnostics).
- `src/config.rs` + `src/config/paths.rs`: config schema/normalization and environment-aware path resolution.
- Supporting core modules: `src/timer.rs`, `src/blocker.rs`, `src/schedule.rs`, `src/session_recovery.rs`, `src/task_labels.rs`, `src/wakatime.rs`, and `src/notifications.rs`.

WakaTime tracking is optional and activates only when an API key is configured
(read from `~/.wakatime.cfg`).

Runtime flow (high-level):

1. `main` parses CLI args and either runs a CLI command path or starts the TUI loop.
2. In TUI mode, each frame renders UI and reads keyboard/paste input.
3. `App` handles key events (`start/pause`, `stop`, `next`, session planner actions, site manager actions).
4. Timer ticks advance every elapsed second while running.
5. Phase-completion notifications are dispatched asynchronously.
6. Blocking is applied during focus phases and removed outside focus.
7. WakaTime tracking stays in sync with focus-running state and applies async
   heartbeat outcomes without blocking timer flow.

### WakaTime reliability behavior

When WakaTime is configured, heartbeats are still best-effort and non-blocking.
The timer never waits on network calls.

- transient heartbeat failures (`429`, `5xx`, and connectivity/timeout errors)
  retry with bounded backoff (`1s`, then `2s`)
- retryable failures that still cannot be delivered are queued in-memory (bounded,
  drop-oldest at capacity) and replayed automatically when connectivity recovers
- non-retryable failures are surfaced in the timer view status line
- status line reflects runtime states (`tracking`, `sending`, `queued`,
  `replaying`, `retrying`, `error`, `idle`, `not configured`) and, when
  configured, also shows the last successful heartbeat time (`HH:MM:SS`) or
  `not yet sent` before first success

For full module map and design details, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) for:

- local quality checks
- coding and commit conventions
- pull request workflow

## Release automation

Pushing a tag that matches `v*` (for example, `v0.9.0`) triggers the release
workflow. It runs CI quality gates (`check`, `fmt`, `clippy`, `test`, dependency
`audit`, and `typos`), builds binaries for Linux/macOS/Windows, and publishes
them to the GitHub Release attached to that tag.

The latest stable release is [v0.9.0](https://github.com/utilForever/focustime/releases/tag/v0.9.0).

For a human-readable summary of notable changes in this release, see [CHANGELOG.md](CHANGELOG.md).

## License

<img align="right" src="https://149753425.v2.pressablecdn.com/wp-content/uploads/2009/06/OSIApproved_100X125.png">

The class is licensed under the [MIT License](https://opensource.org/licenses/MIT):

Copyright &copy; 2026 [Chris Ohk](https://www.github.com/utilForever).

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
