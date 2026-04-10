# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/utilForever/focustime/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/utilForever/focustime/releases/tag/v0.1.0
