# AGENTS.md

This file provides context and instructions for AI coding agents working on the **focustime** project.

## Project Overview

**focustime** is a TUI (Terminal User Interface) application built in Rust that combines:

- ⏱ **Pomodoro Timer** – structured focus/break intervals
- 🚫 **Site Blocking** – block distracting websites during focus sessions
- 📊 **Wakatime Tracking** – integrates with Wakatime to log coding activity

The project is actively developed with frequent incremental releases.

## Tech Stack

| Component  | Details             |
| ---------- | ------------------- |
| Language   | Rust (edition 2024) |
| Build tool | Cargo               |
| Toolchain  | stable              |

## Repository Structure

```text
focustime/
├── src/
│   ├── main.rs                 # Composition root and TUI runtime loop
│   ├── app.rs                  # App facade
│   ├── app/                    # App domain modules (timer flow, planner, schedule, etc.)
│   ├── cli.rs                  # CLI facade
│   ├── cli/                    # CLI args/parsing/execution/output modules
│   ├── stats.rs                # Stats facade
│   ├── stats/                  # Stats persistence/analytics/export modules
│   ├── ui.rs                   # UI facade
│   ├── ui/                     # Screen render modules (timer/history/setup/etc.)
│   ├── config.rs               # Config facade and schema
│   ├── config/                 # Config helpers (including paths.rs)
│   ├── timer.rs                # Pomodoro timer state machine
│   ├── blocker.rs              # Hosts-file blocking integration
│   ├── schedule.rs             # Schedule compile/occurrence logic
│   ├── session_recovery.rs     # Runtime recovery snapshot helpers
│   ├── task_labels.rs          # Task-label normalization/indexing
│   ├── wakatime.rs             # WakaTime tracking integration
│   └── notifications.rs        # Desktop notifications and sound
├── Cargo.toml                  # Package manifest and dependencies
├── Cargo.lock                  # Locked dependency versions
├── .github/workflows/rust.yml  # CI pipeline (check, lint, test, audit)
├── ARCHITECTURE.md             # Detailed module map and interaction diagrams
├── CONTRIBUTING.md             # Contributor workflow and quality bar
└── README.md
```

## Architecture Conventions

- Use a **facade + submodule** pattern for large domains:
  - `src/app.rs` + `src/app/*.rs`
  - `src/cli.rs` + `src/cli/*.rs`
  - `src/stats.rs` + `src/stats/*.rs`
  - `src/ui.rs` + `src/ui/*.rs`
- Keep facade modules focused on shared types/API boundaries; place feature logic in domain submodules.
- Prefer explicit imports over wildcard imports.
- For split domains, place shared module tests in colocated `tests.rs` files.

## Common Commands

### Build & Check

```sh
# Build the project
cargo build

# Check for compilation errors without producing a binary
cargo check --all
```

### Test

```sh
cargo test --all
```

### Lint & Format

```sh
# Check formatting (do not auto-fix in CI)
cargo fmt --all -- --check

# Apply formatting locally
cargo fmt --all

# Run Clippy (treat warnings as errors)
cargo clippy --all-targets -- -D warnings
```

### Security Audit

```sh
# Requires cargo-audit: `cargo install cargo-audit`
cargo audit
```

## Commit Style

- Conventional Commits: `feat:`, `fix:`, `refactor:`, `perf:`, `test:`, `docs:`, `chore:`
- Split commits by behavior or another meaningful unit of change.
- Release commits: `feat: vX.Y.Z — short summary` (for example, `feat: v0.15.1 — short summary`)
- Hotfix: `fix: description` (no version in message)

## Release Documentation Checklist

When preparing a release update, keep these files aligned:

- `Cargo.toml` package version
- `Cargo.lock` root `focustime` package version
- `README.md` release automation, cleanup roadmap, deprecation notices, and latest-release references
- `CHANGELOG.md` release section and compare links
- `CONTRIBUTING.md` release workflow/tag examples
- `REGRESSION_MATRIX.md` merged/deprecated/removed feature-path coverage and supported replacement behavior
- Release tag examples in docs use the current target release version (for example, `v0.15.1`)
- `AGENTS.md` release checklist and process guidance
- `ARCHITECTURE.md` when release-facing guidance changes

## CI Pipeline

The GitHub Actions workflow (`.github/workflows/rust.yml`) runs on every push/PR to `main`:

1. **Check** – `cargo check --all`
2. **Lint** – `cargo fmt` check + `cargo clippy` (warnings = errors)
3. **Test** – `cargo test --all`
4. **Audit** – dependency security audit via `actions-rust-lang/audit`

All CI jobs must pass before merging a pull request.

## Contribution Guidelines

- Keep code formatted with `cargo fmt` before committing.
- Fix all `cargo clippy` warnings — the CI enforces `-D warnings`.
- Add tests for new functionality in the relevant module; for split domains, prefer colocated `tests.rs`.
- Keep commits focused and write clear commit messages.
- Open a pull request targeting the `main` branch.
