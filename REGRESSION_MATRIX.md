# Cleanup Regression Matrix

This matrix protects feature paths that were merged, deprecated, or removed during
the v0.14.x and v0.15.x cleanup cycles. The focused gate is:

```sh
cargo test --test v014_regression_matrix
```

The focused gate is also covered by `cargo test --all`, so it runs in CI and in
the normal release readiness command set.

## Coverage

| Release | Scenario | Path | Gate |
| --- | --- | --- | --- |
| v0.14.2 | Timer profile/options merged into canonical presets (`basic`, `standard`, `advanced`) while legacy aliases still migrate safely. | Config migration | `v014_config_migration_preview_and_apply_cover_merged_profile_presets` |
| v0.14.2 | Legacy `profile_automation` preset keys merge into existing canonical preset tables without losing nested values. | Config migration | `v014_config_migration_preview_and_apply_cover_merged_profile_presets` plus `migrate_config_toml_v1_to_v2_merges_legacy_profile_automation_into_existing_preset_key` |
| v0.14.2 | Canonical stats persistence remains authoritative after fallback cleanup. | Runtime fallback | `v014_runtime_stats_fallback_stays_removed_for_canonical_persistence` |
| v0.14.3 | Config migration assistant previews before writing and apply mode writes a backup plus migrated config. | Command/config | `v014_config_migration_preview_and_apply_cover_merged_profile_presets` |
| v0.14.3 | Retired migration-window flags stay unavailable. | Removed command | `v014_removed_cli_surfaces_stay_retired_with_json_usage_errors` |
| v0.14.3 | Retired encrypted sync flags stay unavailable and point users to local backup/restore guidance in release notes. | Removed command | `v014_removed_cli_surfaces_stay_retired_with_json_usage_errors` |
| v0.14.4 | Facade/submodule splits preserve public command, config, and runtime contracts. | Command/config/runtime | `cargo test --all` plus the focused matrix gate |
| v0.15.0 | Config migration previews preserve exact output and removed command guidance remains targeted to supported replacements. | Command/config | `v014_config_migration_preview_and_apply_cover_merged_profile_presets` plus `v014_removed_cli_surfaces_stay_retired_with_json_usage_errors` |
| v0.15.x | Cleanup roadmap documentation names supported replacements before additional overlapping paths are merged or retired. | Release docs | README roadmap, CHANGELOG entry, and contributor release checklist review plus the focused matrix gate when behavior changes |

## Release Readiness

Run the focused matrix when a release includes cleanup, deprecation, migration,
or facade/submodule work:

```sh
cargo test --test v014_regression_matrix
```

Then run the full CI-equivalent release checks:

```sh
cargo check --all
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

When a v0.14.x or v0.15.x feature is merged, deprecated, or removed, add a row
here and add or update the matching focused test before preparing the release
commit. Documentation-only roadmap updates should still keep README,
CHANGELOG.md, CONTRIBUTING.md, and this matrix aligned with supported
replacement behavior.
