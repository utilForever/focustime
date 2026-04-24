# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **CLI v3 core management commands (#172):** added `--goal`, `--strict`, `--schedule`, `--schedule-set`, and `--diagnostics` commands with text/JSON output for automation workflows.

### Changed

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

[Unreleased]: https://github.com/utilForever/focustime/compare/v0.5.2...HEAD
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
