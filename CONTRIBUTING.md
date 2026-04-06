# Contributing to focustime

Thanks for contributing to **focustime**. This guide explains the expected workflow and quality bar for pull requests.

## Getting Started

1. Install the stable Rust toolchain.
2. Clone the repository and open it in your terminal.
3. Run the core checks before opening a PR:

```sh
cargo check --all
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

Optional local build:

```sh
cargo build
```

Optional dependency security audit (requires `cargo-audit`):

```sh
cargo install cargo-audit
cargo audit
```

## Architecture

`focustime` is a Rust TUI application that combines a Pomodoro timer, site blocking, and WakaTime tracking.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the module map, component interactions,
visibility rules, and file conventions.

## Code Style

- Run `cargo fmt --all` before committing.
- Treat Clippy warnings as errors: `cargo clippy --all-targets -- -D warnings`.
- Keep changes focused and avoid unrelated refactors.
- Add or update tests when changing behavior.
- Prefer clear, small functions and explicit error handling.

## Pull Requests

- Open pull requests against the `main` branch.
- Keep PRs focused on one change set.
- Ensure CI-equivalent checks pass locally:

```sh
cargo check --all
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

- Use Conventional Commit prefixes in commit messages:
  - `feat:`, `fix:`, `refactor:`, `perf:`, `test:`, `docs:`, `chore:`

## Releasing

The project uses Conventional Commit-style release commits:

- Release commit format: `feat: vX.Y.Z — short summary`
- Hotfix format: `fix: description` (no version in the message)
- Update [CHANGELOG.md](CHANGELOG.md) with release notes before creating a release commit/tag.

Before preparing a release commit, make sure all CI jobs pass for the release changes.

To publish a release artifact set, create and push a `v*` tag (for example, `v0.2.0`).
The release workflow will:

- run `cargo check --all --locked`, `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --all --locked`
- run dependency audit and typos checks
- build release binaries for Linux, macOS, and Windows
- upload those binaries to the GitHub Release for the tag

## Dependencies

Key dependencies are defined in `Cargo.toml`:

- `ratatui`: terminal UI rendering.
- `crossterm`: terminal input/output and screen control.
- `ureq` + `serde`: HTTP and JSON support for WakaTime heartbeats.
- `base64`: API authorization header encoding.

Dependency guidelines:

- Prefer minimal, well-maintained crates.
- Keep `Cargo.lock` committed.
- Run `cargo audit` when updating dependencies.
