# Cleanup Regression Matrix

This matrix protects feature paths that were merged, deprecated, or removed during
the v0.14.x and v0.15.x cleanup cycles. The focused gates are:

```sh
cargo test --test v014_regression_matrix
cargo test --test v015_cleanup_regression
```

The focused gates are also covered by `cargo test --all`, so they run in CI and
in the normal release readiness command set.

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
| v0.15.1 | Cleanup roadmap documentation names supported replacements before additional overlapping paths are merged or retired. | Release docs | `v015_cleanup_docs_keep_matrix_and_release_guidance_aligned` |
| v0.15.1 | Deprecated config compatibility fields stay visible in diagnostics with canonical replacement guidance. | Config diagnostics | `v015_deprecated_config_paths_report_supported_replacements` |
| v0.15.1 | Merged legacy profile names continue to migrate to canonical presets (`basic`, `standard`, `advanced`). | Config migration | `v015_merged_profile_paths_keep_migration_guidance` |
| v0.15.1 | Removed migration-window and encrypted sync flags stay unavailable and emit targeted JSON `error.hint` replacement guidance. | Removed command | `v015_removed_command_paths_keep_targeted_json_guidance` |
| Unreleased | Setup diagnostics, config doctor, and migration preview guidance stay available from one canonical CLI diagnostics workflow. | Config diagnostics | `diagnostics_output_includes_config_health_and_migration_guidance` |
| Unreleased | Backend-selected blocking preview details are available from `--diagnostics`, while standalone preview output points to the canonical replacement. | Blocking diagnostics | `diagnostics_json_includes_blocking_preview_payload` plus `blocking_preview_json_emits_payload_on_stdout` |
| Unreleased | Raw usage-signal inspection stays deprecated and points cleanup/reporting workflows to feature inventory output. | Usage cleanup | `usage_signals_json_emits_deprecated_replacement_payload` plus committed feature inventory coverage |

## Release Readiness

Run the focused matrix when a release includes cleanup, deprecation, migration,
or facade/submodule work:

```sh
cargo test --test v014_regression_matrix
cargo test --test v015_cleanup_regression
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
