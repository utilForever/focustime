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

TUI-based application for **Pomodoro timing**, **distraction-site blocking**, and optional **WakaTime heartbeat tracking**.

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
      <img src="./assets/demo_session_planner.png" alt="Task setup demo" width="600">
      <p>Task Setup</p>
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

Before running `--start`, select a task label first (with `--task` or in the TUI).

```sh
# Start focus timer without entering TUI
cargo run -- --start
cargo run -- --start --json

# Control timer flow without entering TUI
cargo run -- --pause
cargo run -- --resume
cargo run -- --stop
cargo run -- --next --json

# Select task label (creates label if it does not exist yet)
cargo run -- --task "Write docs"
cargo run -- --task=Write-docs --json
# Archived labels are rejected by --task and cannot be used to start focus.

# Show or set the active profile
cargo run -- --profile
cargo run -- --profile standard
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

# Inspect the stable Focus History KPI dashboard layout
cargo run -- --history-dashboard
cargo run -- --history-dashboard --json

# Manage canonical blocklist sites
cargo run -- --blocklist-sites
cargo run -- --blocklist-site-add="youtube.com, *.facebook.com"
cargo run -- --blocklist-site-edit "youtube.com=news.ycombinator.com"
cargo run -- --blocklist-site-delete reddit.com

# Show/set schedule for the selected profile (including overlap/conflict inspection)
cargo run -- --schedule
cargo run -- --schedule-set='{"windows":[{"days":["mon","tue"],"start":"09:00","end":"11:00"}]}'
cargo run -- --schedule --json

# Show the consolidated diagnostics workflow:
# setup checks, blocking preview details, config health, and config migration guidance
cargo run -- --diagnostics
cargo run -- --diagnostics --json

# Show status (text or JSON, including growth/retention signals, live timer/session fields, and latest interruption summary)
cargo run -- --status
cargo run -- --status --json
# Export productivity comparisons for deeper status/history analysis
cargo run -- --export=./reports --json

# Watch status continuously (default 1s cadence, optional seconds override; Ctrl-C exits cleanly)
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

### Retired local daemon API

- The local daemon API lifecycle commands (`--daemon-start`, `--daemon-status`, `--daemon-stop`, and `--daemon-port`) are removed.
- New automation should use CLI timer/session/workflow commands (`--start`, `--pause`, `--resume`, `--stop`, `--next`, `--task`) or the TUI for interactive focus sessions.
- The loopback `/v1/*` daemon endpoints are no longer a supported runtime surface.

Backup/restore/export behavior:

- `--backup` and `--export` share the same target-directory handling: omitted directories use the current working directory and explicit targets are created before artifact files are written.
- `--backup` creates the target directory if needed, then copies `config.toml` and `stats.toml` into it.
- `--restore` requires both files in the source directory and uses staged replacement so failed restores roll back to the original files.
- Runtime persistence is canonical-path only; if only legacy `stats.toml` exists, copy it to the canonical stats path (the backup/restore commands can help).

### WakaTime integration runtime

`focustime` routes supported WakaTime tracking behavior through a narrow
integration runtime. The runtime exposes only the tracking calls the app uses:
polling async heartbeat outcomes, syncing focus running state, advancing
elapsed focus time, and updating heartbeat metadata.

v0.17.0 scope decision (#564): keep WakaTime as an optional heartbeat-only
integration instead of removing it from the product. Follow-up cleanup should
make heartbeat delivery fire-and-forget: keep API-key detection, global
`[wakatime]` metadata, focus-session heartbeat submission, and a simple
configured/sent/error status, while removing durable queue/replay behavior,
runtime retry/backoff tuning, and diagnostics that only exist for the richer
queueing runtime. Full WakaTime removal is deferred unless a later roadmap issue
changes product scope.

Decision comparison:

| Option | Product impact | Affected code/docs |
| --- | --- | --- |
| Remove WakaTime entirely | `focustime` becomes a Pomodoro + site-blocking app; WakaTime leaves README/Cargo positioning and setup guidance. | Remove `src/wakatime*`, `src/integration.rs` WakaTime hooks, `[wakatime]` config, TUI/setup/status copy, WakaTime tests, `ureq`/`base64` ownership, and WakaTime docs. |
| Keep minimal heartbeat-only WakaTime | `focustime` remains a Pomodoro + site-blocking app with optional WakaTime heartbeat tracking. | Keep global `[wakatime]` metadata, API-key detection, heartbeat transport, and simple status; remove `[wakatime_runtime]`, durable queue snapshots, replay/backoff diagnostics, and queue-specific tests/docs in follow-up issues. |

Current supported integration ID:

- `wakatime`

Config example (`config.toml`):

```toml
[feature_flags.integrations]
enabled = ["wakatime"]
```

Set `enabled = []` to disable all built-in integrations.

### Legacy compatibility deprecation milestones

`focustime --diagnostics` is the canonical diagnostics workflow for setup
checks, config health, and migration guidance. The older focused config
diagnostics commands have been retired, so scripts should read the
`config_doctor` and `config_migration` sections from
`focustime --diagnostics --json`. The TUI Setup Diagnostics screen reports
targeted setup deprecation warnings when legacy compatibility fields are
detected.

| Legacy field/path                                                                    | Canonical replacement                                                                                                                             | Removal milestone |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| Top-level `focus_secs`, `short_break_secs`, `long_break_secs`, `long_break_interval` | `[custom_profile]`                                                                                                                                | v0.12.0           |
| Top-level `notifications`, `auto_start`, `strict_mode`, `recurring_schedule`         | `[profile_automation.<preset>.notifications]`, `[profile_automation.<preset>.auto_start]`, and per-preset `strict_mode` / `recurring_schedule` | v0.12.0           |
| Top-level `blocked_sites`                                                            | Canonical `[[blocklist_profiles]]` entry named `Default`                                                                                          | v0.12.0           |

Milestone policy:

- **v0.10.x migration window:** warning-only window with migration tooling (`--migrate`, `--backup`, `--restore`)
- **v0.11.0+:** retired temporary migration-only CLI compatibility flags (`--migrate`, `--dry-run`); `--backup`/`--restore` remain supported.
- **v0.15.2:** consolidated diagnostics are available through `--diagnostics`; config health and migration guidance are included in the canonical diagnostics payload.
- **v0.15.3:** calendar annotation cache behavior and weekday rules are documented as compatibility cleanup paths; schedule windows and supported timer controls remain the supported behavior.
- **v0.15.4:** blocklist/allowlist site management operates on profile-level rules without selected-category branching, while temporary override state is represented through the canonical runtime model.
- **v0.15.5:** Focus History uses a stable default KPI layout, export/history remain the deeper comparison paths, and backup/export artifact workflows share target-directory handling.
- **v0.15.6:** daemon local API lifecycle commands report retirement guidance, runtime dependency ownership stays documented, and WakaTime integration uses explicit supported runtime calls.
- **v0.15.7:** standalone blocking preview access, Focus History dashboard customization paths, and dedicated status comparison guidance stay removed while diagnostics, the stable KPI dashboard, export artifacts, and Focus History remain the supported replacements.
- **v0.15.8:** blocklist category config is flattened into profile-level `sites` and `allowlist_sites`, `automation_triggers` config is removed during migration, and neither legacy surface is re-persisted by runtime writes.
- **v0.15.9:** standalone calendar sync and daemon local API command access are retired, and dependency ownership reflects the removed refresh and daemon paths.
- **v0.16.0:** daemon-owned runtime dependency cleanup is locked; WakaTime owns runtime HTTP and Basic auth while daemon-only local API server and direct random-token dependencies stay removed.
- **v0.16.1:** focused config diagnostics commands are retired in favor of `--diagnostics`; feature inventory CLI export and committed generated inventory snapshots are retired; legacy cleanup-specific regression gates are archived, and current cleanup contracts live in normal CI/module/integration tests.
- **v0.16.2:** schedule exception dates, calendar annotation cache handling, and retired calendar timezone parsing stay removed; recurring schedule windows remain the supported schedule model, and `chrono-tz` stays out of the manifest and lockfile.
- **v0.16.3:** task note metadata, focus intention metadata, task-specific goals, session-template command/config surfaces, and per-task WakaTime mappings are retired; task labels are the supported session context in status, recovery, history, and exports, while WakaTime uses one global metadata configuration.
- **v0.16.4:** allowlist site-management commands, blocklist profile CRUD/selection, custom blocking backend/fallback policy, break-glass workflow, and temporary override runtime state are retired; canonical blocklist commands, config/internal allowlist rules, hosts-file diagnostics, and normal timer controls are the supported replacements.
- **Future cleanup:** continue retiring overlapping paths only after release notes and docs name supported replacement behavior.
- **v0.12.0:** remove legacy field/path compatibility after the warning window

### v0.16.x cleanup roadmap

The v0.16.x line continues the cleanup work started in v0.14.x by reducing
overlapping command and config paths while keeping supported behavior available
through canonical surfaces. The guiding rule is that a path is only retired when
release notes and diagnostics name the replacement behavior.

Roadmap direction:

- Keep profile-oriented timer settings as the primary timer configuration path.
- Keep one focus-entry runtime path for scheduled and manual starts.
- Keep `--diagnostics` as the supported way to inspect setup health, config
  health, and migration guidance together.
- Keep local backup/restore workflows as the supported portable recovery path.
- Keep cleanup candidates tracked in GitHub roadmap issues first, with release
  notes and static documentation naming supported replacement behavior before
  paths are merged or retired. Generated feature inventory snapshots are no
  longer part of release preparation, and cleanup-specific v0.14/v0.15 gates are
  no longer part of release readiness.
- Keep broad integration lifecycle/capability hooks retired in favor of the
  supported WakaTime integration runtime calls for heartbeat polling,
  focus-running sync, elapsed focus tracking, and metadata updates.
- For v0.17.0, keep WakaTime in product positioning as optional heartbeat
  tracking, but simplify the implementation to fire-and-forget heartbeat
  submission. Follow-up cleanup issues should remove or collapse
  `[wakatime_runtime]`, durable queue snapshots, replay/backoff diagnostics, and
  dependency ownership notes that only apply to the current queueing runtime.

Early deprecation notices:

| Deprecated or overlapping path | Supported replacement behavior |
| --- | --- |
| Legacy timer duration fields (`focus_secs`, `short_break_secs`, `long_break_secs`, `long_break_interval`) | Use `[custom_profile]`, profile presets, and `--profile`; run `--diagnostics` when stale keys are reported. |
| Legacy automation and blocklist top-level fields | Use per-profile automation tables and the canonical `Default` blocklist profile; inspect with `--diagnostics`. |
| Retired blocklist category config is migration-only | `--diagnostics` reports migration guidance to flatten category `sites` and `allowlist_sites` into profile-level lists; manage blocked hostnames directly with `--blocklist-sites`, `--blocklist-site-add`, `--blocklist-site-edit`, and `--blocklist-site-delete`. |
| Allowlist site-management commands (`--allowlist-sites`, `--allowlist-site-add`, `--allowlist-site-edit`, `--allowlist-site-delete`) | Removed; keep persistent exceptions in `allowlist_sites` config/internal rules and manage canonical blocked hostnames with blocklist site commands. |
| Blocklist profile CRUD and selection (`--blocklist-profile*`) | Removed; existing profile rules are collapsed into the canonical `Default` blocklist/allowlist, and new site changes use direct blocklist site commands. |
| Custom command blocking backend and backend fallback policy | Removed; hosts-file blocking is the supported backend, and `--diagnostics` reports hosts-file readiness and preview details. |
| Temporary allowlist CLI/runtime workflow | Removed; manage blocked hostnames with blocklist commands and keep persistent exceptions in `allowlist_sites` config when needed. |
| Break-glass temporary override workflow | Removed; use normal timer controls (`--pause`, `--resume`, `--stop`) for session flow changes or blocklist commands for site-rule changes. |
| Split temporary override runtime fields | Removed; runtime persistence and `--status --json` no longer emit temporary override entries or legacy `break_glass_*` / `temporary_allowlist_*` fields. |
| Focus History dashboard customization (`[history_dashboard]`, retired customization CLI paths) | Use the stable default KPI layout shown by `--history-dashboard`; customization commands are removed from help text and command parsing. |
| Advanced status comparison slicing | Use `--export` artifacts for productivity comparison rows, or Focus History reports/dashboard filters for interactive comparison workflows. |
| Standalone automation trigger rules (`automation_triggers`, `--automation-triggers*`) | Removed; use profile schedules for automatic focus starts, supported timer controls for active windows, and task/profile/blocklist commands for defaults. |
| Standalone blocking preview command (`--blocking-preview`) | Removed; use `--diagnostics` for blocking preview details alongside setup/config health. |
| Focused config diagnostics commands (`--config-doctor`, `--config-migrate`, `--config-migrate-apply`) | Removed; use `--diagnostics` for text guidance or `--diagnostics --json` for the `config_doctor` and `config_migration` sections. |
| Standalone usage-signal command (`--usage-signals`) | Removed; use GitHub roadmap issues, release notes, and static cleanup documentation for planning while raw command/screen frequency summaries remain internal cleanup inputs. |
| Standalone feature inventory export (`--feature-inventory`) | Removed; generated inventory snapshots are no longer committed or regenerated for releases. Use GitHub roadmap issues, release notes, and static cleanup documentation for planning. |
| Schedule exception dates | Removed; represent focus availability with recurring schedule windows, inspect overlaps with `--schedule`, and use supported timer controls for one-off workflow adjustments. |
| Standalone calendar refresh command (`--calendar-sync`) and `[calendar_sync]` config | Removed; scheduling no longer reads calendar annotation caches or renders calendar-derived busy/overlap text. |
| Task note metadata and command surface (`--task-note`, timer note editing, `task_note` status/export fields) | Removed; use `--task` and task label selection as the supported session context. |
| Focus intention metadata (`--focus-intention`, `focus_intention` recovery/status/export fields) | Removed; use `--task` and task label selection as the supported session metadata. |
| Task-specific cumulative goals (`--task-goal`, selected task goal status/history/export fields) | Removed; use global daily, weekly, and monthly goals with `--goal`, `--goal-weekly`, and `--goal-monthly`; task labels remain available for grouping. |
| Session template workflows (`--session-template*` commands and session-template config/runtime persistence) | Removed; select task, profile, schedule, and blocklist settings directly through their dedicated controls. |
| Per-task WakaTime metadata mappings (`[[wakatime.task_mappings]]`) | Removed; configure one global `[wakatime]` project/language pair for heartbeat metadata. |
| Rich WakaTime queue/replay runtime (`[wakatime_runtime]`, durable queue snapshots, retry/backoff diagnostics) | v0.17.0 cleanup target; keep optional global WakaTime heartbeat submission, but make delivery fire-and-forget and remove queue-specific configuration/status surfaces in follow-up issues. |
| Daemon local API lifecycle (`--daemon-start`, `--daemon-status`, `--daemon-stop`, `--daemon-port`, `/v1/*`) | Removed; use CLI timer/session/workflow commands (`--start`, `--pause`, `--resume`, `--stop`, `--next`, `--task`) for automation, or the TUI for interactive focus sessions. |
| Duplicate schedule/session start entry points | Select the task/profile/blocklist/schedule directly, then start focus through the unified timer flow with `--start` or the TUI. |

Runtime dependency ownership after daemon and calendar cleanup:

| Dependency | Owning feature paths | Ownership note |
| --- | --- | --- |
| `ureq` JSON feature | WakaTime heartbeat transport. Retired calendar annotation and daemon paths no longer own runtime HTTP. | Keep while WakaTime heartbeat submission uses `send_json`; re-audit if the transport changes. |
| `base64` | WakaTime Basic auth. | Keep while WakaTime uses Basic auth. |
| `chrono` | Timer/stat dates and recurring schedule windows. | Keep for core time/date handling; retired calendar timezone parsing must not reintroduce `chrono-tz`. |

When changing `Cargo.toml` dependency ownership, run `rg -n "chrono_tz|chrono-tz" src tests Cargo.toml Cargo.lock` to confirm retired calendar timezone parsing stays removed, then run `rg -n "ureq" src tests`, `cargo check --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`, and `cargo audit`.

### CLI JSON/error contract

- `--json` success responses are emitted to `stdout` as JSON and exit with code `0`.
- `--json` failures are emitted to `stdout` as JSON (no mixed human text) and exit with a non-zero code; ordinary unsupported options omit `error.hint`.
- `--status --watch --json` emits newline-delimited compact JSON snapshots continuously until interrupted, then exits cleanly after the current snapshot.
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
  session context (task, profile, schedule, stats, WakaTime, strict mode).
- **Manager/detail views** (sites, profiles, task setup, history, diagnostics)
  follow a consistent pattern: context header, primary content block, feedback
  line, and compact command legend.
- **Profiles, task setup, and history** are now laid out to fit narrower and
  shorter terminal windows than before.
- **Focus History** uses a redesigned structure with an overview block and
  dedicated task/trend/audit panels for faster scanning.
- **Command legends** use short grouped lines so common actions are easier to
  scan quickly during focus sessions.

## Keyboard shortcut customization

Core command shortcuts are configurable in `config.toml` under `[shortcuts]`.
Shortcut customization now also covers navigation/edit interactions used across
manager/editor flows.

For safety:

- `Ctrl-C` as a quit fallback
- text-entry `Type` and paste behavior stay native

Example:

```toml
[shortcuts]
open_stats_history = "y"
# Compatibility key: opens Task Setup.
open_session_planner = "g"
back_stats_history = "y"
timer_stop_reset = "x"
quit = "q"
timer_toggle_pause = "space"
navigate_up = "up"
navigate_down = "down"
navigate_left = "left"
navigate_right = "right"
confirm = "enter"
cancel = "esc"
delete = "delete"
backspace = "backspace"
```

Navigation/edit tokens accept either a single character or a named key:
`enter`, `esc`, `up`, `down`, `left`, `right`, `delete`, `backspace`, `space`.

## Pomodoro presets

`focustime` now supports selectable Pomodoro presets:

- **Basic** (25/5/15, long break every 4 focus sessions)
- **Standard** (50/10/30, long break every 3 focus sessions)
- **Advanced** (editable in-app)

Open profile manager from timer view with **`p`**.

- `↑/↓` (default `navigate_up`/`navigate_down`): move between presets
- `Enter` (default `confirm`): apply selected preset
- `e`: open profile/settings editor
- In editor: `↑/↓` selects field, `←/→` adjusts numeric/boolean values (including **Theme preset**), `Type/Backspace` edits WakaTime project/language, `Enter` saves (all defaults are configurable via navigation/edit shortcut fields)

Preset selection, theme preset selection, custom durations, and preset-scoped
automation settings are persisted in `config.toml`.

## Task setup

Open task setup from timer view with **`t`**.

- `a`: add a new task label
- `e`: rename the highlighted task label
- `d` or `Delete`: delete the highlighted task label
- `Enter` (default `confirm`): select the highlighted task label (archived labels are visible but cannot be selected)
- `f` toggle favorite (favorites are listed first), `x` toggle archive, `r` or `1-5` quick-pick recent labels
- `↑/↓` (default `navigate_up`/`navigate_down`): move selection
- `t` or `Esc` (default `cancel`): return to timer view
- while adding/renaming a label, `Enter` (default `confirm`) saves and `Esc` (default `cancel`) cancels

Starting a focus session from idle now requires a selected task label. The timer
view always shows the current task label (or a reminder to select one).

## Focus history dashboard

Open Focus History from timer view with **`h`**.

- `←/→`: cycle comparison dimension
- `↑/↓`: cycle task slice, `[`/`]`: cycle profile slice, `,`/`.`: cycle time-of-day slice

Focus History renders a stable default KPI layout covering session summary,
focus score, goal streak, focus risk, weekly allocation, last interruption,
stats growth, retention, and comparison filters. Dashboard pin, unpin, and order
customization commands are retired; use `--history-dashboard` for CLI layout
inspection.

CLI layout inspection remains available through `--history-dashboard`.

Blocklist rules support exact hosts and wildcard subdomain rules. `*.example.com`
matches `docs.example.com` and `api.example.com`, but does **not** match
`example.com`.

### Example config

```toml
schema_version = 2
selected_profile = "advanced"
selected_theme_preset = "classic"
selected_blocklist_profile = "Default"

[shortcuts]
timer_toggle_pause = "space"
timer_stop_reset = "s"
# Compatibility key: opens Task Setup.
open_session_planner = "t"
open_stats_history = "h"
quit = "q"

[[blocklist_profiles]]
name = "Default"
sites = ["youtube.com", "*.facebook.com", "reddit.com"]
allowlist_sites = ["reddit.com"]

[custom_profile]
focus_secs = 1800
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3

[profile_automation.advanced]
# Strict mode for the selected profile.
strict_mode = false

[profile_automation.advanced.notifications]
enabled = true
sound = false

[profile_automation.advanced.auto_start]
focus_to_break = false
break_to_focus = false

[[profile_automation.advanced.recurring_schedule.windows]]
days = ["mon", "tue", "wed", "thu", "fri"]
start = "09:00"
end = "11:00"

[schedule_runtime]
time_step_minutes = 15

[daily_goal]
minutes = 120
pomodoros = 4

[weekly_goal]
minutes = 600
pomodoros = 20

[monthly_goal]
minutes = 2400
pomodoros = 80

[stats_retention]
preset = "balanced" # keep_all | balanced | aggressive

[wakatime]
project = "focustime"
language = "Pomodoro"

[wakatime_runtime]
retry_backoff_secs = [2, 5, 10]
queue_capacity = 512
queue_retry_delay_secs = 30
```

`schema_version` is managed by focustime when writing `config.toml`. Files
without this key are treated as legacy and migrated automatically. If a file
declares a newer schema version than the running binary supports, focustime
attempts a best-effort load of known fields.

`[wakatime]` is optional. If omitted (or set to blank values), `focustime` uses
the defaults above for one global heartbeat metadata configuration.

`[schedule_runtime]` is optional. When omitted, focustime keeps existing
schedule runtime defaults (`time_step_minutes = 15`).
`time_step_minutes` is clamped to `1..60`.

`[calendar_sync]` is retired. Existing configs that still contain this section
are loaded without using calendar data, and runtime writes omit the section.
Schedule output remains deterministic without calendar-derived busy or overlap
annotations.


`[wakatime_runtime]` is optional. When omitted, focustime keeps existing
WakaTime runtime defaults (`retry_backoff_secs = [2, 5, 10]`,
`queue_capacity = 512`, `queue_retry_delay_secs = 30`). Backoff entries are
bounded to `1..300` seconds (up to 8 entries, empty/invalid falls back to
defaults), queue capacity is clamped to `1..4096`, and queue replay delay is
clamped to `1..3600`.

## Site manager workflow

Open the site manager from timer view with **`b`**.

- `a`: add/import hostnames
- `e`: edit the selected hostname
- `d` or `Delete`: remove the selected hostname
- `m`: toggle between editing blocklist sites and allowlist exceptions
- `↑/↓` (default `navigate_up`/`navigate_down`): move selection
- `b`: return to timer view
- `Esc` (default `cancel`): return to timer view only when add/edit mode is not active

Add/import input supports:

- single hostnames (`youtube.com`)
- wildcard subdomain rules (`*.example.com`; subdomains only)
- comma-separated lists (`youtube.com, reddit.com`)
- newline-separated lists (paste multi-line blocklists, then press `Enter`)
- while add/import or edit mode is active, `Enter` (default `confirm`) commits and `Esc` (default `cancel`) cancels the current draft

Invalid and duplicate entries are reported inline so you can fix them without leaving the view.

Allowlist entries act as explicit exceptions: effective focus blocking is computed as
**blocklist sites minus allowlist sites** for the canonical blocklist, using exact and
wildcard rule matching. Allowlist management is config-only; use the
`allowlist_sites` list in the canonical `[[blocklist_profiles]]` entry for
persistent exceptions.

Older blocklist category config is accepted only as migration input.
`--diagnostics` reports guidance to flatten category `sites` and
`allowlist_sites`, and runtime saves persist one canonical `Default`
`[[blocklist_profiles]]` entry.

For hosts-based blocking to apply reliably, keep DNS-over-HTTPS disabled in your browser.
`focustime` supports hosts-file blocking as the single blocking backend.

## Setup diagnostics

Open the setup diagnostics screen from timer view with **`d`**.

- `r`: refresh diagnostics checks
- `d` or `Esc`: return to timer view

The diagnostics screen reports:

- blocking permissions
- hosts file write capability
- blocking preview summary and hosts-file target details
- remediation guidance when hosts-file readiness is insufficient
- WakaTime config status (`~/.wakatime.cfg` and `api_key` availability)
- WakaTime runtime queue/retry status (`not configured`, `idle`,
  `tracking`, `sending`, `queued`, `replaying`, `retrying`, `error`, and
  related pending counts/backoff details)

The CLI diagnostics command adds blocking preview details, config health
findings, and migration preview guidance to the same workflow:

```sh
focustime --diagnostics
focustime --diagnostics --json
```

The standalone `focustime --blocking-preview` path has been removed. Scripts
should read the `blocking_preview` section from
`focustime --diagnostics --json`.

The standalone `focustime --usage-signals` path has been removed; cleanup
planning should use GitHub roadmap issues, release notes, and static cleanup
documentation while treating raw usage-signal summaries as internal cleanup
inputs.

The standalone `focustime --feature-inventory` path and generated inventory
snapshots have been removed. Releases no longer require regenerating
`FEATURE_INVENTORY.md` or `FEATURE_INVENTORY.json`; keep cleanup planning in
GitHub roadmap issues, release notes, and static cleanup documentation.

The standalone `focustime --automation-triggers*` path has been removed. Scripts
should configure profile schedules with `focustime --schedule-set`, use supported
timer controls for active windows, and select task/profile/blocklist defaults
through their dedicated commands.

## Phase notifications

`focustime` emits a phase notification when a phase naturally completes at `00:00`:

- **Focus complete** → next break phase
- **Break complete** → focus phase

Manual skip (`n`) changes phase immediately but does not emit a completion notification.

Notifications are delivered best-effort:

- terminal notice in the timer view
- desktop notification via platform-specific delivery (`winrt-toast-reborn` toast on Windows with a `msg` fallback, `osascript` on macOS, `notify-send` on Linux)
- optional sound alert using platform audio capabilities when `profile_automation.<preset>.notifications.sound = true`

Natural, non-catchup phase transitions can also auto-start the next timer with safe defaults (`Off`):

- `profile_automation.<preset>.auto_start.focus_to_break` starts break timers automatically after focus completion on non-catchup ticks
- `profile_automation.<preset>.auto_start.break_to_focus` starts focus timers automatically after break completion on non-catchup ticks

Recurring schedule windows can also trigger focus behavior at wall-clock times:

- `profile_automation.<preset>.recurring_schedule.windows[].days` accepts day tokens (`mon`..`sun`, case-insensitive)
- `profile_automation.<preset>.recurring_schedule.windows[].start` / `end` use 24-hour `HH:MM` local time (`start < end`)
- when a window begins, focus auto-starts if possible; otherwise schedule mode arms and shows a reminder until you manually start focus
- if multiple windows overlap, the most recently started active window takes precedence; windows with the same start time are resolved deterministically
- `--schedule` (text and JSON) reports recurring window overlaps without rejecting the schedule
- standalone `automation_triggers[]` config entries are removed by config migration; schedule windows provide automatic focus starts and supported timer controls handle active windows
- deprecated `weekday_profile_rules[]` config entries are removed by config migration; model weekday defaults with schedule windows and profile settings instead
- the timer session overview shows the current/next scheduled window
- the standalone `--calendar-sync` refresh command and `[calendar_sync]` config are retired; runtime scheduling does not read calendar annotation cache data

You can configure notification and auto-start settings directly from the TUI:

- open profile manager with `p`
- press `e` to open the editor
- automation and schedule edits apply to the currently selected profile only
- the editor is grouped into sections (**Timer**, **Automation**, **Goals**, **Appearance**, **WakaTime**, **Schedule**) to keep settings easier to scan
- use `↑/↓` (default `navigate_up`/`navigate_down`) to select **Phase notifications**, **Sound alert**, **Auto-start break**, **Auto-start focus**, **Strict focus mode**, **Daily/Weekly/Monthly goal (minutes)**, **Daily/Weekly/Monthly goal (pomodoros)**, **Theme preset**, **WakaTime project/language**, or the **Schedule** fields
- use `←/→` (default `navigate_left`/`navigate_right`) to adjust values (or toggle `Off`/`On` for boolean fields), use `Type/Backspace` (default `backspace`) for WakaTime text fields, then `Enter` (default `confirm`) to save
- schedule editing is in-app:
  - **Schedule add/remove**: `→` adds a window, `←` removes selected window
  - **Schedule window**: `←/→` changes which window is selected
  - **Schedule day** + **Schedule day enabled**: choose day cursor and toggle it `Off/On`
  - **Schedule start/end**: adjust times in `[schedule_runtime].time_step_minutes` steps (default `15`, clamped `1..60`)

## Session recovery

`focustime` persists in-progress timer sessions so restart/crash recovery can resume where you left off.

- while a focus/break phase is running or paused, the app saves phase, remaining time, task label, and active profile
- startup recovery also reconciles transient workflow runtime artifacts when still valid (schedule arming continuity and strict-reset confirmation state)
- on startup, valid in-progress state is restored and shown in the timer notice line
- on startup, blocking is reconciled with recovered timer state: recovered active focus re-applies blocking, while non-recovered startup attempts to remove stale crash-era block entries
- stale or invalid saved recovery/runtime artifacts are ignored safely with a startup warning notice
- recovery state is cleared when an in-progress phase ends naturally or when you reset/skip out of the active session

## Strict focus mode

`focustime` provides an optional strict mode (`strict_mode = false` by default).

When strict mode is enabled during an active focus session:

- `n` (skip phase) is disabled
- `s` (stop/reset) requires confirmation by pressing `s` again
- `p` (profile manager) is disabled, so profile switching is locked
- quit shortcuts (`q`, `Esc`, `Ctrl-C`) are disabled until focus is no longer active

## Temporary override workflows

Temporary allowlist and break-glass workflows have been retired. Use permanent
`allowlist_sites` config entries for persistent exceptions, blocklist commands
for blocked-site changes, and normal timer controls (`--pause`, `--resume`,
`--stop`, `--next`) for session flow changes.

## Session stats and history

`focustime` tracks:

- completed pomodoros for the current app session
- focused minutes for the current app session
- daily aggregates persisted in `stats.toml` in the canonical data/state directory
- weekly totals derived from daily aggregates in the History view
- weekly consistency score (`active_days / 7`, rounded to `%`) derived from daily activity
- weekly focus score KPI (50/50 blend of consistency and weekly goal completion; `n/a` when weekly goal is off)
- profile effectiveness comparison (focus share % and average focused minutes per completed session)
- productivity comparison rows by task/profile/time-of-day in History and exports
- per-task totals (pomodoros and focused minutes) derived from labeled focus sessions
- per-task trend summaries in History (`last 7 days` vs `previous 7 days`)
- structured interruption events for manual `stop/reset` and `skip/next` actions
- current streak and best streak based on completed daily goals
- growth indicators (`record` count + estimated `stats.toml` size + top high-volume sections)

Current retention presets for historical records:

- `keep_all`: no automatic pruning
- `balanced` (default): keep daily aggregates, prune `focus_sessions` at 365 days, and prune `session_interruptions` at 180 days
- `aggressive`: prune daily aggregates at 365 days, `focus_sessions` at 180 days, and `session_interruptions` at 90 days

Retention is enforced when stats are persisted. Existing data older than the selected windows is pruned on save.

If daily, weekly, or monthly goals are configured, timer and history views also
show live progress for each period:

- target focused minutes
- target completed pomodoros

Streaks are evaluated against the goal that was active on each day. Changing the
daily goal later does not rewrite older tracked days, and streak tracking stays
inactive when today's goal is off.

From timer view:

- press **`h`** to open the history panel with weekly and daily summaries
- while history is open, use **`←/→`** to switch comparison dimension; **`↑/↓`** task filter; **`[`/`]`** profile filter; **`,`/`.`** time-of-day filter
- while the history panel is open, press **`e`** to export `focustime-stats.json` and `focustime-stats.csv` into the current working directory
- press **`h`** or **`Esc`** to return to timer view

Exports include daily/weekly aggregates, weekly consistency, weekly focus score,
profile effectiveness, productivity comparisons, task summaries/trends,
interruption records, and labeled focus-session records where task labels were
attached. Focus-session rows persist and export `task_label` without separate
task note metadata. Interruption records include structured `reason` values and
remaining-time metadata. Export files now also include a `history_kpis` JSON
object covering all History dashboard KPI cards (`session_summary`,
`focus_score`, `goal_streak`, `focus_risk`, `weekly_allocation`,
`last_interruption`, `stats_growth`, `retention`, `comparison_filters`), with
matching CSV `history_kpi` rows (`kpi_card_id` + `kpi_payload_json`) for
JSON/CSV parity. Export files expose `schema_version` (currently `9`) so
downstream consumers can handle versioned contracts explicitly.

## The way the system works

`focustime` is a single-binary Rust app organized around top-level facade modules
with focused submodules (updated in #240):

- `src/main.rs`: composition root, CLI/TUI dispatch, terminal lifecycle, and event loop.
- `src/app.rs` + `src/app/*.rs`: runtime state/orchestration split by domain (timer flow, task setup, profiles, site manager, schedule, persistence, diagnostics, CLI API).
- `src/cli.rs` + `src/cli/*.rs`: CLI args/parsing/execution/status/output pipeline.
- `src/stats.rs` + `src/stats/*.rs`: stats persistence, analytics, trends, recording, planner state, and exports.
- `src/ui.rs` + `src/ui/*.rs`: Ratatui rendering split by screen (timer, task setup, site manager, profile manager, history, setup diagnostics).
- `src/config.rs` + `src/config/paths.rs`: config schema/normalization and environment-aware path resolution.
- Supporting core modules: `src/timer.rs`, `src/blocker.rs`, `src/schedule.rs`, `src/session_recovery.rs`, `src/task_labels.rs`, `src/integration.rs`, `src/wakatime.rs`, and `src/notifications.rs`.

WakaTime tracking is optional and activates only when an API key is configured
(read from `~/.wakatime.cfg`).

For v0.17.0 planning, WakaTime remains in scope only as optional heartbeat
submission. Queue/replay persistence, retry tuning, and queue-specific
diagnostics are cleanup targets rather than long-term product surfaces.

Runtime flow (high-level):

1. `main` parses CLI args and either runs a CLI command path or starts the TUI loop.
2. In TUI mode, each frame renders UI and reads keyboard/paste input.
3. `App` handles key events (`start/pause`, `stop`, `next`, task setup actions, site manager actions).
4. Timer ticks advance every elapsed second while running.
5. Phase-completion notifications are dispatched asynchronously.
6. Blocking is applied during focus phases and removed outside focus.
7. WakaTime tracking is managed via the narrow `IntegrationRuntime` (`App ->
   IntegrationRuntime -> WakaTime`) and applies async heartbeat outcomes without
   blocking timer flow.

### WakaTime reliability behavior

When WakaTime is configured, heartbeats are still best-effort and non-blocking.
The timer never waits on network calls.

- transient heartbeat failures (`429`, `5xx`, and connectivity/timeout errors)
  retry with bounded backoff (default `2s`, then `5s`, then `10s`, configurable via
  `[wakatime_runtime].retry_backoff_secs`)
- retryable failures that still cannot be delivered are queued in a durable local
  backlog (bounded, drop-oldest at capacity; default `512`, configurable via
  `[wakatime_runtime].queue_capacity`) and replayed oldest-first after restart
  and when connectivity recovers
- queued heartbeat replay delay after retryable failure starts at `30s`, doubles
  after each consecutive retryable replay failure (up to `3600s`), and is
  configurable via `[wakatime_runtime].queue_retry_delay_secs`
- invalid/corrupt persisted queue snapshots are dropped on startup and surfaced
  as a runtime warning in WakaTime status
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

Pushing a tag that matches `v*` (for example, `v0.16.4`) triggers the release
workflow. It runs CI quality gates (`check`, `fmt`, `clippy`, `test`, dependency
`audit`, and `typos`), builds binaries for Linux/macOS/Windows, and publishes
them to the GitHub Release attached to that tag.

The latest stable release is [v0.16.4](https://github.com/utilForever/focustime/releases/tag/v0.16.4).

For a human-readable summary of notable changes in this release, see [CHANGELOG.md](CHANGELOG.md).

## License

<img align="right" src="https://149753425.v2.pressablecdn.com/wp-content/uploads/2009/06/OSIApproved_100X125.png">

The class is licensed under the [MIT License](https://opensource.org/licenses/MIT):

Copyright &copy; 2026 [Chris Ohk](https://www.github.com/utilForever).

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
