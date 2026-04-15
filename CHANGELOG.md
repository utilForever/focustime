# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1] - 2026-04-15

### Changed
- **v0.3.1 release metadata refresh (#103):** updated crate version metadata, README release references, and changelog links for the `v0.3.1` release.

## [0.3.0] - 2026-04-15

### Added
- **Daily focus goals and live progress (#92):** introduced configurable daily minute/pomodoro goals with live progress shown in timer and history views.
- **Streak tracking (#85):** added current-streak and best-streak tracking based on completed daily focus goals.
- **Weekly history summaries (#86):** added weekly aggregation of focused minutes and completed Pomodoros in the History view.
- **Auto-start phase options (#93):** added opt-in auto-start controls for focus-to-break and break-to-focus transitions on natural (non-catchup) completions.
- **Setup diagnostics screen (#96):** added in-app diagnostics for blocking permissions, hosts file write access, and WakaTime configuration readiness.
- **WakaTime success timestamp visibility (#95):** surfaced the last successful heartbeat time in the timer status line when tracking is configured.

### Changed
- **Windows keyboard input handling (#94):** fixed duplicate key processing on Windows and added regression coverage for key filtering behavior.
- **v0.3.0 visual documentation refresh (#99):** updated README screenshots to reflect current UI and workflows.

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

[Unreleased]: https://github.com/utilForever/focustime/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/utilForever/focustime/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/utilForever/focustime/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/utilForever/focustime/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/utilForever/focustime/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/utilForever/focustime/releases/tag/v0.1.0
