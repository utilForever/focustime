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
| v0.15.2 | Setup diagnostics, config doctor, and migration preview guidance stay available from one canonical CLI diagnostics workflow. | Config diagnostics | `diagnostics_output_includes_config_health_and_migration_guidance` |
| v0.15.2 | Backend-selected blocking preview details are available from `--diagnostics`, while standalone preview output points to the canonical replacement. | Blocking diagnostics | `diagnostics_json_includes_blocking_preview_payload` plus `blocking_preview_json_emits_payload_on_stdout` |
| v0.15.2 | Raw usage-signal inspection stays deprecated and points cleanup/reporting workflows to feature inventory output. | Usage cleanup | `usage_signals_json_emits_deprecated_replacement_payload` plus committed feature inventory coverage |
| v0.15.3 | Calendar busy-window sync stays a narrow opt-in schedule annotation cache, while standalone `--calendar-sync` reports deprecated replacement guidance and disabled/absent calendar data leaves schedules deterministic. | Schedule/config cleanup | `calendar_sync_json_emits_deprecated_schedule_annotation_guidance` plus `load_with_env_reports_calendar_sync_annotation_cache_guidance` and `recurring_schedule_text_omits_calendar_annotations_when_disabled_or_absent` |
| v0.15.3 | Deprecated weekday profile rules migrate into compatibility automation time triggers, while `--weekday-rules*` keeps replacement guidance and avoids persisted orphan config paths. | Schedule/config cleanup | `normalize_moves_weekday_profile_rules_to_canonical_automation_triggers` plus `weekday_rules_json_emits_deprecated_replacement_payload` |
| v0.15.3 | Standalone automation trigger rules stay readable as deprecated compatibility data, while runtime focus behavior uses schedule windows, schedule delay, and session templates. | Schedule/runtime cleanup | `automation_triggers_json_emits_deprecated_replacement_payload` plus `deprecated_schedule_window_end_trigger_does_not_delay_schedule_runtime` |
| v0.15.3 | Blocklist category workflows stay compatibility-only with replacement guidance, while profile-level blocklist/allowlist site management remains the supported path. | Blocking cleanup | `blocklist_category_json_emits_deprecated_replacement_payload` plus `config_doctor_reports_deprecated_blocklist_categories` |
| v0.15.4 | Core blocklist/allowlist site add, edit, delete, and list paths operate on profile-level rules without selected-category branching. | Blocking cleanup | `apply_site_add_command_uses_profile_sites_not_selected_category` plus `site_manager_uses_profile_sites_not_selected_category` |
| v0.15.4 | Temporary allowlist and break-glass workflows share one temporary override runtime model while legacy fields remain readable for compatibility. | Blocking/runtime cleanup | `app_restores_temporary_overrides_from_canonical_snapshot` plus `build_status_output_includes_break_glass_temporary_override` |
| v0.15.4 | Focus History uses a stable default KPI layout while deprecated dashboard pin, unpin, and order commands report guidance without persisting customization state. | Stats cleanup | `history_dashboard_uses_stable_default_layout_despite_legacy_customization` plus `apply_history_dashboard_pin_is_deprecated_and_keeps_default_layout` |
| v0.15.4 | Status comparison CLI flags stay retired from `--status` and point users to export artifacts or Focus History reports. | Stats cleanup | `parse_rejects_status_comparison_options_with_export_replacement` plus `parse_rejects_equals_status_comparison_options_with_export_replacement` |
| v0.15.4 | Backup, stats export, and feature-inventory artifact workflows share target-directory creation and JSON path contract behavior. | Artifact cleanup | `artifact_workflows_json_create_target_dirs_and_preserve_path_fields` plus `artifact_workflows_json_report_consistent_target_directory_errors` |

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
