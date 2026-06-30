# Architecture

`focustime` is a single-binary Rust TUI application organized around stable
top-level facades (`app.rs`, `cli.rs`, `stats.rs`, `ui.rs`) plus focused
domain submodules. This structure keeps runtime behavior and public entry points
stable while splitting implementation details by responsibility.

## Visual overview

```mermaid
flowchart LR
    M["main.rs<br/>entrypoint + terminal lifecycle"]
    CLI["cli.rs + cli/*<br/>CLI contract/parsing/execution/output"]
    APP["app.rs + app/*<br/>runtime orchestration + state transitions"]
    UI["ui.rs + ui/*<br/>screen rendering"]
    ST["stats.rs + stats/*<br/>persistence/analytics/export"]
    CFG["config.rs + config/paths.rs<br/>config model + path resolution"]
    TM["timer.rs<br/>Pomodoro state machine"]
    BL["blocker.rs<br/>hosts-file blocking"]
    NT["notifications.rs<br/>phase notifications"]
    SCH["schedule.rs<br/>window compilation/selection"]
    REC["session_recovery.rs<br/>runtime snapshot I/O"]
    TL["task_labels.rs<br/>task label normalization/indexing"]
    OS["OS / filesystem / hosts / notifications"]

    M --> CLI
    M --> APP
    M --> UI
    APP --> CFG
    APP --> TM
    APP --> BL
    APP --> NT
    APP --> SCH
    APP --> ST
    APP --> TL
    APP --> REC
    CLI --> APP
    CLI --> CFG
    CLI --> ST
    UI --> APP
    BL --> OS
    NT --> OS
```

## Module map

| Module                          | Responsibility                                                                                                                                                                                                                                                                                  | Main collaborators                                                                            |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `main.rs`                       | Composition root, CLI vs TUI dispatch, terminal setup/teardown, frame/tick loop                                                                                                                                                                                                                 | `cli`, `app`, `ui`, `crossterm`, `ratatui`                                                    |
| `app.rs` + `app/*`              | Core runtime state and orchestration split into focused domains (`timer_flow`, `task setup`, `site_manager`, `profile_management`, `schedule_*`, `persistence`, `history_goals`, `feedback_diagnostics`, `cli_api`, `mode_keys`) | `timer`, `blocker`, `notifications`, `schedule`, `stats`, `config` |
| `cli.rs` + `cli/*`              | CLI contract and execution pipeline split into `args`, `parsing`, `execute`, `status`, and `output`, including headless timer/session/workflow controls and stats export. Local backup/restore, standalone calendar refresh, feature inventory export, task metadata/goal, session template, temporary allowlist, break-glass override, and daemon local API lifecycle commands are retired.                    | `app`, `config`, `stats`, `blocker`                                     |
| `stats.rs` + `stats/*`          | Stats data model plus split persistence/analytics/export/recording/planner/trends helpers, including canonical-path persistence, task-label grouping, and legacy read-time compatibility handling during deprecation windows                                                                                          | `app`, `task_labels`, filesystem                                                              |
| `ui.rs` + `ui/*`                | Screen-oriented Ratatui rendering split into `timer`, task setup, `site_manager`, `profile_manager`, `history`, and `setup`                                                                                                                                                                      | `app`, `timer`                                                                 |
| `config.rs` + `config/paths.rs` | Config schema/normalization and environment-aware config path resolution, including feature-flag compatibility defaults and runtime knob settings                                                                                                          | `app`, `cli`, filesystem/env                                                                  |
| `timer.rs`                      | Pomodoro timer domain model and phase transitions                                                                                                                                                                                                                                               | `app`, `ui`                                                                                   |
| `blocker.rs`                    | Hosts-file blocking, preview generation, rollback-aware hosts updates, and hosts permission diagnostics                                                                                                                                                                                          | `app`, `cli`, OS/filesystem                                                                   |
| `schedule.rs`                   | Recurring schedule window compile, overlap inspection, and occurrence selection logic                                                                                                                                                                                                            | `app`, `cli`, `config`                                                                        |
| `session_recovery.rs`           | Runtime recovery snapshot read/write, transient runtime artifact reconciliation, and startup warning notices for dropped invalid fragments                                                                                                                                                      | `app`, `cli`, filesystem                                                                      |
| `task_labels.rs`                | Task-label normalization, canonicalization, and index helpers                                                                                                                                                                                                                                   | `app`, `stats`, `cli`                                                                         |
| `notifications.rs`              | Phase completion notifications and optional sound alerts                                                                                                                                                                                                                                        | `app`, OS notification commands                                                               |

## Runtime flow (timer mode)

```mermaid
sequenceDiagram
    participant Main as main loop
    participant App as App
    participant Timer as TimerState
    participant Blocker as SiteBlocker
    participant Schedule as ScheduleState
    participant Stats as Stats
    participant Notify as PhaseNotifier
    participant UI as ui::render

    loop every frame
        Main->>App: poll_runtime_status()
        Main->>UI: render(frame, &app)
        Main->>App: handle_key/handle_paste (if input)
        Main->>App: on_tick() (when 1s elapsed)
        App->>Timer: tick()
        App->>Schedule: reconcile schedule state
        alt phase changed
            App->>Blocker: block()/unblock()
            App->>Notify: notify_phase_completion()
            App->>Stats: record completed focus/interruption data
        end
        Main->>App: on_runtime_elapsed(elapsed_secs)
        App-->>UI: expose timer, task, schedule, blocking, goal, and notice state
    end
```

1. `main` parses CLI arguments and either executes a CLI command path (`cli`) or
   boots the interactive TUI loop (`app` + `ui`).
2. Each TUI frame polls local runtime status, renders via `ui::render`, then
   processes keyboard/paste input through `App` key handlers.
3. A 100ms cadence accumulates elapsed time; each elapsed second advances
   `App::on_tick()` and applies phase-driven side effects.
4. `App` keeps blocking, notifications, scheduling, stats, and recovery in sync
   with timer state; side effects are isolated in dedicated modules.
5. Headless automation routes through CLI timer/session/workflow commands; the
   daemon local API lifecycle and loopback `/v1/*` endpoints are retired.
6. WakaTime heartbeat tracking, retry queues, and runtime diagnostics are
   retired; runtime architecture has no external heartbeat service dependency.

## Visibility rules

- Keep top-level modules crate-private via `mod ...` declarations in `main.rs`.
- Use root facade modules (`app.rs`, `cli.rs`, `stats.rs`, `ui.rs`) as stable
  integration points; keep submodule details internal by default.
- Expose only cross-module API with `pub`/`pub(crate)` when required by
  collaborators or tests.
- Prefer private fields/functions unless cross-module mutation is intentional.

## File conventions

- Use a **facade + submodule** pattern for large domains:
  - `src/app.rs` with `src/app/*.rs`
  - `src/cli.rs` with `src/cli/*.rs`
  - `src/stats.rs` with `src/stats/*.rs`
  - `src/ui.rs` with `src/ui/*.rs`
  - `src/config.rs` with `src/config/paths.rs`
- Keep behavior grouped by domain responsibility (timer flow, task setup, schedule,
  CLI parsing, export logic, screen rendering) rather than by utility type.
- Place domain test suites in colocated `tests.rs` submodules where it improves
  readability and keeps facades focused.
- Keep platform-specific behavior explicit with `#[cfg(...)]` in the module that
  owns it.
- For release docs updates, keep architecture-facing terminology aligned with the
  module names and responsibility language used in `README.md`, `CHANGELOG.md`,
  and release tag examples in contributor-facing docs (for example, `v0.17.0`
  for this release cycle).
