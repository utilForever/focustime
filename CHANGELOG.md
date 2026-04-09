# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Phase completion notifications for natural `00:00` transitions between Focus and Break phases.
- Optional sound alerts controlled by `[notifications] sound` in `config.toml`.
- Cross-platform best-effort desktop notification dispatch (`msg` on Windows, `osascript` on macOS, `notify-send` on Linux) without blocking the TUI loop.

## [0.1.0] - 2026-04-06

### Added
- Initial release of `focustime` as a Rust TUI application.
- Pomodoro timer with focus, short break, and long break session flow.
- Distraction website blocking through hosts file updates during focus sessions.
- Optional WakaTime heartbeat integration for focus activity tracking.
- Release automation for tagged builds across Linux, macOS, and Windows.

[Unreleased]: https://github.com/utilForever/focustime/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/utilForever/focustime/releases/tag/v0.1.0
