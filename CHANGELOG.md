# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.1] - 2026-05-18

### Changed

- **Restart/recovery elapsed reconciliation and status hydration (#309):** recovery snapshot hydration now reconciles elapsed wall time so restored phase/remaining/session totals stay accurate across relaunch gaps in both startup restoration and CLI status output paths.
- **Hosts-file rollback hardening on partial failures (#310):** hosts backend block/unblock updates now use explicit rollback behavior to restore the prior hosts state when write/replace operations fail mid-update.
- **WakaTime queue replay/backoff tuning (#311):** retuned default retry/runtime queue knobs to reduce aggressive retry loops, added exponential replay cooldown for consecutive retryable queue failures, and expanded queued/replaying/retrying/error status transition coverage in runtime/UI tests.
- **CLI `--json` error contract consistency sweep (#312):** expanded parser/runtime error-contract coverage to enforce stable JSON error envelope shape, exit-code mapping, and stream behavior across command families.
- **`--status --watch` cadence and interrupt polish (#313):** switched watch scheduling to monotonic deadlines for steadier cadence and improved interrupt handling so watch mode exits cleanly without partial output.

## [0.11.0] - 2026-05-16

### Added

- **Blocking backend strategy and diagnostics surfaces (#300):** added configurable blocking backend policy (`hosts_only`, `hosts_then_command`, `command_then_hosts`, `command_only`) and exposed backend diagnostics/reporting paths in CLI and TUI setup workflows.

### Changed

- **Removed legacy config mirror writes and compatibility-sync paths (#297):** canonical config/profile/blocklist and canonical stats persistence are now the only runtime/write paths, while legacy top-level config fields remain load-time compatibility input for migrated user configs.
- **Finalized canonical persistence paths and retired deprecated schema handling (#298):** removed remaining migration-window persistence branches and aligned runtime/docs with canonical config and stats path behavior.
- **Retired migration-window CLI compatibility flags (#299):** removed deprecated `--migrate`/`--dry-run` command paths and transitional migration output shims; `--backup`/`--restore` remain supported and docs/guidance now reflect canonical-path-only persistence.

## [0.10.1] - 2026-05-13

### Added

- **Expanded runtime recovery scope and startup reconciliation notices (#294):** recovery now includes schedule arming continuity and strict-reset pending confirmation alongside existing timer/schedule-delay/break-glass artifacts, with deterministic partial-recovery startup notices when saved runtime fragments are dropped.
- **Dual-read/dual-write stats persistence compatibility path (#291):** added canonical data/state stats persistence with legacy-path read fallback and dual-write mirroring controls, including CLI backup/restore and diagnostics visibility updates for migration windows.
- **Migration tooling with dry-run and rollback safety (#292):** added `--migrate` with explicit `--dry-run` preview mode, structured migration step reporting, and rollback-aware migration execution/diagnostics for stats-path compatibility finalization.
- **Deprecation warnings and migration docs for legacy fields/paths (#293):** added targeted setup/CLI diagnostics warnings for detected legacy config and stats-path compatibility usage, plus README mapping and planned removal milestones.

## [0.10.0] - 2026-05-11

### Added

- **CLI schedule-delay and break-glass workflow controls (#282):** added reusable app APIs and non-interactive CLI commands to delay active schedule windows and toggle break-glass mode across invocations.
- **Runtime-tunable schedule and WakaTime queue knobs (#283):** added config/runtime controls for schedule granularity and WakaTime retry queue replay behavior, with validation and runtime application.
- **Headless session start command contract (#284):** made `--start` fully non-interactive and documented/tested the process-safe contract for CLI-driven session start/recovery flows.
- **Configurable navigation and edit shortcuts (#285):** added user-configurable navigation/edit key bindings and propagated those bindings through timer note prompts and related UI interactions.

### Changed

- **Cross-platform CI test parity documentation and workflow alignment (#286):** updated CI test matrix expectations and contributor documentation for Linux/Windows/macOS parity behavior.

## [0.9.1] - 2026-05-08

### Added

- **Feature flags for rollout-safe compatibility paths (#258):** added centralized `[feature_flags]` config defaults plus diagnostics visibility to gate legacy automation/blocklist mirrors and task-label metadata fallback behavior across config, CLI, recovery, and stats loading paths.
- **CLI parity for session metadata commands (#259):** added non-interactive `--focus-intention` and `--task-note` read/set workflows so in-progress session metadata can be inspected and updated from CLI output contracts without entering TUI mode.
- **Durable WakaTime offline queue replay (#268):** persisted retryable heartbeat backlog to local app data so queued/replaying heartbeats survive restarts/crashes, replay deterministically with existing queue bounds, and surface startup warnings when invalid persisted queue snapshots are dropped.
- **Config schema/versioning groundwork for upcoming migrations (#274):** added explicit config `schema_version` handling, migration-path scaffolding for legacy snapshots, and lenient forward-compatible loading for newer schema versions.
- **Stats growth observability and retention policy controls (#275):** added growth/size summary signals and configurable `stats_retention` presets surfaced in CLI `--status` outputs and the TUI history overview.

## [0.9.0] - 2026-05-05

### Added

- **WakaTime task-label metadata mapping (#247):** added optional `[[wakatime.task_mappings]]` config entries to override heartbeat `project`/`language` per task label with case-insensitive matching and per-field fallback to global `[wakatime]` defaults across runtime tracking.
- **Offline WakaTime heartbeat queue/replay (#248):** added bounded in-memory queuing for retryable heartbeat delivery failures with automatic oldest-first replay after connectivity recovers, plus timer status visibility for `queued` and `replaying` states.
- **CLI backup/restore commands (#249):** added `--backup[=DIR]` and `--restore[=DIR]` automation commands to copy `config.toml` and `stats.toml`, including strict restore validation that requires both files in the restore source directory.

## [0.8.1] - 2026-05-04

### Added

- **Keyboard shortcut customization for command actions (#243):** added configurable `[shortcuts]` bindings for timer controls, view switching, manager actions, export/refresh, and quit command routing across config, runtime action dispatch, and UI command legends.
- **Theme presets with accessibility options (#244):** added global theme preset selection (`classic`, `high-contrast`, `deuteranopia-friendly`) across config persistence, TUI appearance rendering, profile editor controls, and CLI `--theme` management/status outputs.

### Changed

- **Shortcut input handling hardening (#243):** prevented quit-key collisions and preserved `Ctrl-C` fallback behavior during text-entry workflows, including site manager editing.

## [0.8.0] - 2026-05-03

### Added

- **Task label favorites and archive workflow (#234):** added favorite/archive controls for planner task labels, with persisted state and archived-label safeguards across planner and schedule flows.
- **Profile-aware automation rules (#235):** automation settings (`notifications`, `auto_start`, `strict_mode`, and recurring schedule windows) are now stored and applied per timer profile across config, TUI profile editing, and CLI automation commands.
- **Configurable break templates (#237):** added selectable break templates with profile-manager workflow and CLI output visibility, aligned with custom break timing behavior.

### Changed

- **Codebase refactor and reliability hardening (#240):** reorganized app/UI/CLI/stats modules for maintainability and fixed edge-case behavior in planner selection, archived-label handling, config path normalization, schedule edit deduplication, export durability, and history/streak presentation.
- **CI dependency maintenance (#241):** updated release-quality workflow dependencies by bumping SonarQube scan action to v8 and typos tooling to 1.46.0.

## [0.7.1] - 2026-04-28

### Added

- **Session interruption tracking and visibility (#229):** added interruption event tracking for manual stop/skip flows, plus latest interruption summaries in CLI/TUI status outputs and stats exports.
- **Mid-session note editing workflow (#230):** added in-focus note editing and timer-view note surfaces for active sessions.

### Changed

- **Interruption summary reliability hardening (#229):** fixed latest-interruption selection behavior and canonical-label rendering consistency.
- **Mid-session note UX and input hardening (#230):** preserved custom notes on task rename, aligned note-editing UI hints, and sanitized pasted timer notes.

## [0.7.0] - 2026-04-28

### Added

- **Weekly and monthly goals across CLI and TUI (#218):** added configurable weekly/monthly goal targets across automation surfaces and in-app workflows.
- **Optional goal carry-over rules (#219):** added configurable carry-over behavior for unmet goals to support flexible planning cycles.
- **Task-level cumulative goals (#220):** added per-task goal targets (minutes/pomodoros) with CLI management via `--task-goal`, status JSON exposure for the selected task, and history-panel progress evaluation against each task target.
- **Weekly focus score KPI across stats/history/CLI (#221):** added weekly focus score metric surfaces for analysis workflows in TUI history and CLI outputs.

### Changed

- **TUI layout and Focus History ergonomics refresh (#225):** reworked timer/history screen structure for clearer navigation and readability.

## [0.6.2] - 2026-04-26

### Added

- **One-time ad-hoc schedule windows (#213):** added date-specific one-time schedule windows across config/CLI/TUI/runtime flows while preserving recurring schedule defaults and compatibility.
- **Schedule delay shortcut for active windows (#214):** added a 10-minute schedule snooze/delay control for currently active schedule windows.
- **Schedule conflict inspector for UI/CLI (#215):** added overlap/conflict inspection surfaces in both TUI and CLI schedule workflows.

### Changed

- **One-time schedule normalization hardening (#213):** prevented malformed one-time window date/time fields from being coerced into valid windows during normalization.
- **Schedule conflict review hardening (#215):** refined conflict-inspection implementation and added direct one-time overlap regression coverage from review follow-up.

## [0.6.1] - 2026-04-26

### Added

- **CLI blocklist profile/site parity commands (#208):** added non-interactive commands for blocklist profile selection/CRUD and blocklist/allowlist site listing and mutation workflows, including text/JSON automation outputs.
- **CLI status watch mode (#209):** added `--status --watch[=SECONDS]` with streaming text and NDJSON status snapshots for automation monitoring.
- **Blocking preview mode (#211):** added read-only hosts-file preview flows (`--blocking-preview`) for CLI and setup diagnostics, including merged-block-section preview correctness.

## [0.6.0] - 2026-04-25

### Added

- **CLI v3 core management commands (#198):** added `--goal`, `--strict`, `--schedule`, `--schedule-set`, and `--diagnostics` commands with text/JSON output for automation workflows.
- **Session metadata v2 persistence (#200):** promoted `focus_intention` and `task_note` to first-class persisted session metadata across recovery, stats storage, and CLI JSON surfaces (defaulting to `task_label` when no dedicated metadata input is provided).

### Changed

- **CLI JSON/error contract and exit-code consistency (#199):** centralized CLI error handling, added machine-readable JSON error envelopes for `--json` failures, and standardized exit-code mapping (`0` success, `1` runtime failure, `2` usage failure).
- **Export schema v3 compatibility (#201):** bumped stats export schema to `v3`, exported focus-session `focus_intention`/`task_note` from persisted metadata, and preserved backward compatibility for legacy stats entries by defaulting missing metadata to `task_label`.
- **Schedule reliability hardening (#202):** made overlapping recurring-window activation deterministic (most recently started active window wins, stable same-start tie handling), tightened next-occurrence scanning around exception-date edge cases, and added regression coverage for schedule trigger consistency.
- **Startup blocking/recovery reconciliation hardening (#205):** improved startup reconciliation of focus blocking with recovered timer state to avoid stale or inconsistent blocking state after restart.

## [0.5.2] - 2026-04-23

### Added

- **Schedule exception dates and skip-day UI (#158):** added exception-date scheduling support with skip-day controls in schedule workflows.
- **Allowlist exceptions and blocking remediation UX (#161):** added allowlist exception policy handling and setup remediation guidance for blocking workflows.
- **History insights v3 (#162):** added weekly consistency scoring (active days per ISO week), profile effectiveness comparison metrics, and corresponding history/export surfaces.

### Changed

- **Dependency update (#159):** bumped `rand` from `0.8.5` to `0.8.6`.

## [0.5.1] - 2026-04-22

### Added

- **Task planner label management and quick picks (#154):** improved session-planner label management flow with quicker task selection.
- **Per-task history totals and trends (#155):** added per-task focus totals and trend summaries in history insights.

## [0.5.0] - 2026-04-22

### Added

- **Session recovery after restart (#147):** added recovery of in-progress timer sessions after app restart.
- **CLI automation v2 commands (#148):** added non-interactive `--pause`, `--resume`, `--stop`, `--next`, and `--task` commands for scriptable timer/session control.

### Changed

- **Richer status JSON for automation (#148):** extended `--status --json` with live timer/session state fields from recovery-aware runtime context.
- **Schedule next-run and active-window guidance (#149):** improved schedule guidance messaging for overdue and active windows.

## [0.4.2] - 2026-04-20

### Added

- **CLI automation commands (#133):** added non-interactive `--start`, `--profile`, `--status`, and `--export` commands for scripting and automation workflows.
- **Recurring focus schedule automation (#134):** added recurring schedule windows, schedule-mode reminders, and in-app schedule editing controls.

### Changed

- **Schedule UX and messaging polish:** clarified schedule UI text and break interruption guidance for more predictable timer behavior.

## [0.4.1] - 2026-04-20

### Added

- **History v2 monthly insights (#129):** introduced monthly trend and heatmap views in History for faster long-horizon focus analysis.
- **In-app WakaTime metadata editor (#130):** added editable WakaTime project/language fields in the profile settings flow.
- **Focus intention export fields (#131):** added `focus_intention` and `task_note` alongside `task_label` in stats export file outputs (JSON/CSV)

### Changed

- **WakaTime metadata editor quit handling:** preserved timer-view quit behavior while editing metadata fields.

## [0.4.0] - 2026-04-18

### Added

- **Session planner workflow (#120):** introduced timer-integrated task labeling with required task selection before starting focus sessions.
- **Blocklist profile management (#121):** added create/rename/delete/switch flows for named site-blocking profiles in the site manager.
- **Break-glass override for focus blocking (#122):** added explicit two-step temporary unblock control with automatic re-enable countdown during active focus sessions.

### Changed

- **Timer layout and UX polish:** improved timer screen readability and spacing in timer and session-planner views.

## [0.3.2] - 2026-04-16

### Added

- **Stats export from History (#105):** added `e` shortcut in History view to export persisted focus data as `focustime-stats.json` and `focustime-stats.csv` in the current working directory.
- **Configurable WakaTime metadata labels (#106):** added optional `[wakatime]` `project` and `language` config fields so heartbeat labels can be customized while preserving default behavior.

### Changed

- **Stats export documentation polish:** clarified Windows export overwrite behavior in user-facing docs.

## [0.3.1] - 2026-04-16

### Added

- **Streak tracking (#101):** added current-streak and best-streak tracking based on completed daily focus goals while preserving historical accuracy when goals change later.
- **Weekly history summaries (#102):** added weekly aggregation of focused minutes and completed Pomodoros in the History view.

## [0.3.0] - 2026-04-15

### Added

- **Daily focus goals and live progress (#92):** introduced configurable daily minute/pomodoro goals with live progress shown in timer and history views.
- **Auto-start phase options (#93):** added opt-in auto-start controls for focus-to-break and break-to-focus transitions on natural (non-catchup) completions.
- **Setup diagnostics screen (#96):** added in-app diagnostics for blocking permissions, hosts file write access, and WakaTime configuration readiness.
- **WakaTime success timestamp visibility (#95):** surfaced the last successful heartbeat time in the timer status line when tracking is configured.

### Changed

- **Windows keyboard input handling (#94):** fixed duplicate key processing on Windows and added regression coverage for key filtering behavior.

## [0.2.1] - 2026-04-13

### Added

- **Strict focus mode safeguards (#74):** optional strict mode now blocks skip/profile/quit shortcuts during active focus and requires explicit confirmation before stop/reset.
- **SonarCloud analysis pipeline (#75):** added SonarCloud workflow and project configuration for continuous quality checks on pushes and pull requests.

### Changed

- **WakaTime tracking reliability and visibility (#72):** improved heartbeat retry behavior and surfaced clearer runtime tracking/retrying/error status in the timer view.
- **Site manager bulk import and editing flow (#73):** added comma/newline batch input support with stronger inline validation and duplicate handling.

## [0.2.0] - 2026-04-10

### Added

- **Persistent settings and blocked sites (#50):** timer preferences, selected profile, notification settings, and blocked-site lists are now saved to `config.toml` and restored at startup with safe fallback defaults for missing/corrupt config.
- **Configurable Pomodoro profiles (#51):** includes built-in **Classic** and **Deep Work** presets plus an editable **Custom** profile with configurable focus/short-break/long-break durations and long-break cadence.
- **Session stats and daily history (#52):** tracks focused time and completed Pomodoros for the active session and per-day aggregates, then surfaces them in the timer summary and history view.
- **Project review and refactoring improvements (#61):** consolidated app orchestration and state transitions to improve reliability around timer flow, persistence, and error reporting.
- **Phase notifications and optional sound (#53):** sends completion notices only on natural `00:00` phase transitions (not manual skip), dispatches desktop notifications asynchronously (`winrt-toast-reborn` toast with `msg` fallback on Windows, `osascript` on macOS, `notify-send` on Linux), and supports `notifications.enabled`/`notifications.sound` toggles from config and the TUI settings editor.

## [0.1.0] - 2026-04-06

### Added

- Initial release of `focustime` as a Rust TUI application.
- Pomodoro timer with focus, short break, and long break session flow.
- Distraction website blocking through hosts file updates during focus sessions.
- Optional WakaTime heartbeat integration for focus activity tracking.
- Release automation for tagged builds across Linux, macOS, and Windows.

[Unreleased]: https://github.com/utilForever/focustime/compare/v0.11.1...HEAD
[0.11.1]: https://github.com/utilForever/focustime/compare/v0.11.0...v0.11.1
[0.11.0]: https://github.com/utilForever/focustime/compare/v0.10.1...v0.11.0
[0.10.1]: https://github.com/utilForever/focustime/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/utilForever/focustime/compare/v0.9.1...v0.10.0
[0.9.1]: https://github.com/utilForever/focustime/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/utilForever/focustime/compare/v0.8.1...v0.9.0
[0.8.1]: https://github.com/utilForever/focustime/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/utilForever/focustime/compare/v0.7.1...v0.8.0
[0.7.1]: https://github.com/utilForever/focustime/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/utilForever/focustime/compare/v0.6.2...v0.7.0
[0.6.2]: https://github.com/utilForever/focustime/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/utilForever/focustime/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/utilForever/focustime/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/utilForever/focustime/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/utilForever/focustime/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/utilForever/focustime/compare/v0.4.2...v0.5.0
[0.4.2]: https://github.com/utilForever/focustime/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/utilForever/focustime/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/utilForever/focustime/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/utilForever/focustime/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/utilForever/focustime/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/utilForever/focustime/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/utilForever/focustime/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/utilForever/focustime/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/utilForever/focustime/releases/tag/v0.1.0
