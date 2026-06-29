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

`focustime` is a Rust TUI application that combines a Pomodoro timer, site blocking, and focus history.

- Facade modules at `src/app.rs`, `src/cli.rs`, `src/stats.rs`, and `src/ui.rs`.
- Focused domain submodules under `src/app/*.rs`, `src/cli/*.rs`, `src/stats/*.rs`, and `src/ui/*.rs`.
- Config path/environment helpers in `src/config/paths.rs`.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the module map, component interactions,
visibility rules, and file conventions.

## Code Style

- Run `cargo fmt --all` before committing.
- Treat Clippy warnings as errors: `cargo clippy --all-targets -- -D warnings`.
- Keep changes focused and avoid unrelated refactors.
- For `app`/`cli`/`stats`/`ui`, keep facade files focused and place domain logic in the matching submodule files.
- Prefer explicit imports over wildcard imports.
- Add or update tests when changing behavior.
- For split domains, place shared module tests in colocated `tests.rs` files.
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

### CI platform parity and caveats

- Pull request CI runs `cargo test --all` on Linux, Windows, and macOS.
- Test parity means each platform runs the same test command; it does not require
  identical test counts, because some tests are intentionally OS-gated with
  `#[cfg(unix)]`, `#[cfg(target_os = "windows")]`, and similar attributes.
- If a failure appears only on one platform, treat it as a platform-specific
  regression and include that platform in your local reproduction notes.

## Releasing

The project uses Conventional Commit-style release commits:

- Release commit format: `feat: vX.Y.Z — short summary`
- Hotfix format: `fix: description` (no version in the message)
- Update [CHANGELOG.md](CHANGELOG.md) with release notes before creating a release commit/tag.

Before preparing a release commit, make sure all CI jobs pass for the release changes.
For cleanup, deprecation, migration, or facade/submodule work, make sure the
affected current-surface module or integration tests are updated and covered by
`cargo test --all`.

Keep [REGRESSION_MATRIX.md](REGRESSION_MATRIX.md) aligned with any feature path
that is merged, deprecated, or removed during release preparation.
Also keep the README roadmap, changelog entry, and deprecation notices aligned
so every deprecated or retired path names supported replacement behavior before
the tag is created.

To publish a release artifact set, create and push a `v*` tag (for example, `v0.16.4`).
The release workflow will:

- run `cargo check --all --locked`, `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --all --locked`
- run dependency audit and typos checks
- build release binaries for Linux, macOS, and Windows
- upload those binaries to the GitHub Release for the tag

## Dependencies

Key dependencies are defined in `Cargo.toml`:

- `ratatui`: terminal UI rendering.
- `crossterm`: terminal input/output and screen control.
- `serde_json`: CLI/status/export JSON.
- `toml`: config, stats, and recovery persistence.
- `chrono`: timer/stat dates and schedule windows.

Dependency guidelines:

- Prefer minimal, well-maintained crates.
- Keep `Cargo.lock` committed.
- Run `cargo audit` when updating dependencies.
- When cleanup work removes daemon, calendar, or retired integration paths, update the
  README runtime dependency ownership table with the owning path and run the
  release readiness checks in `REGRESSION_MATRIX.md`.
- Before changing `Cargo.toml` for calendar-owned cleanup candidates, confirm
  no calendar runtime dependency ownership is being reintroduced and
  `chrono-tz` stays absent from the manifest and lockfile.
