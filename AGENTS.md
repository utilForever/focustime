# AGENTS.md

This file provides context and instructions for AI coding agents working on the **focustime** project.

## Project Overview

**focustime** is a TUI (Terminal User Interface) application built in Rust that combines:

- ⏱ **Pomodoro Timer** – structured focus/break intervals
- 🚫 **Site Blocking** – block distracting websites during focus sessions
- 📊 **Wakatime Tracking** – integrates with Wakatime to log coding activity

The project is in early-stage development.

## Tech Stack

| Component | Details |
|-----------|---------|
| Language  | Rust (edition 2024) |
| Build tool | Cargo |
| Toolchain | stable |

## Repository Structure

```
focustime/
├── src/
│   └── main.rs        # Application entry point
├── Cargo.toml         # Package manifest and dependencies
├── Cargo.lock         # Locked dependency versions
├── .github/
│   └── workflows/
│       └── rust.yml   # CI pipeline (check, lint, test, audit)
└── README.md
```

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
- Add tests for new functionality in the relevant module or in `#[cfg(test)]` blocks.
- Keep commits focused and write clear commit messages.
- Open a pull request targeting the `main` branch.
