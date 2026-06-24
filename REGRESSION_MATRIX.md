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
| v0.14.3 | Retired encrypted sync flags stay unavailable as ordinary unknown options. | Removed command | `v014_removed_cli_surfaces_stay_retired_with_json_usage_errors` |
| v0.14.4 | Facade/submodule splits preserve public command, config, and runtime contracts. | Command/config/runtime | `cargo test --all` plus the focused matrix gate |
| v0.15.0 | Config migration previews preserve exact output while long-retired command paths stay unavailable. | Command/config | `v014_config_migration_preview_and_apply_cover_merged_profile_presets` plus `v014_removed_cli_surfaces_stay_retired_with_json_usage_errors` |
| v0.15.1 | Cleanup roadmap documentation names supported replacements before additional overlapping paths are merged or retired. | Release docs | `v015_cleanup_docs_keep_matrix_and_release_guidance_aligned` |
| v0.15.1 | Deprecated config compatibility fields stay visible in diagnostics with canonical replacement guidance. | Config diagnostics | `v015_deprecated_config_paths_report_supported_replacements` |
| v0.15.1 | Merged legacy profile names continue to migrate to canonical presets (`basic`, `standard`, `advanced`). | Config migration | `v015_merged_profile_paths_keep_migration_guidance` |
| v0.15.1 | Long-retired migration-window and encrypted sync flags stay unavailable as plain unknown options. | Removed command | `v015_removed_command_paths_follow_plain_unknown_option_json_baseline` |
| v0.15.2 | Setup diagnostics, config doctor, and migration preview guidance stay available from one canonical CLI diagnostics workflow. | Config diagnostics | `diagnostics_output_includes_config_health_and_migration_guidance` |
| v0.15.2 | Backend-selected blocking preview details are available from `--diagnostics`, while standalone preview access stays removed as a plain unknown option. | Blocking diagnostics | `diagnostics_json_includes_blocking_preview_payload` plus `blocking_preview_json_reports_plain_unknown_option` |
| v0.15.2 | Raw usage-signal summaries remain internal cleanup inputs while GitHub roadmap issues, release notes, and static docs are the cleanup planning source of truth. | Usage cleanup | committed feature inventory coverage plus `v015_cleanup_docs_keep_matrix_and_release_guidance_aligned` |
| v0.15.3 | Calendar busy-window sync stays a narrow opt-in schedule annotation cache, while disabled/absent calendar data leaves schedules deterministic. | Schedule/config cleanup | `load_with_env_reports_calendar_sync_annotation_cache_guidance` plus `recurring_schedule_text_omits_calendar_annotations_when_disabled_or_absent` |
| v0.15.9 | Standalone calendar sync command parsing is removed, and calendar data remains only optional schedule annotation context when a supported cache is present. | Schedule/config cleanup | `parse_calendar_sync_is_retired` plus `v015_removed_command_paths_follow_plain_unknown_option_json_baseline` |
| v0.15.9 | Calendar and daemon cleanup leave runtime `ureq` and auth `base64` owned by WakaTime heartbeats, remove daemon-only direct dependencies, and keep `chrono-tz` only test-owned until future cache refresh support is reintroduced. | Dependency cleanup | `v015_cleanup_docs_keep_matrix_and_release_guidance_aligned` plus `v015_dependency_ownership_keeps_daemon_only_crates_removed` |
| v0.16.0 | Daemon-owned runtime dependency cleanup stays locked after daemon API retirement, with WakaTime owning runtime HTTP and Basic auth while daemon-only local API server and direct random-token dependencies remain removed. | Dependency cleanup | `v015_cleanup_docs_keep_matrix_and_release_guidance_aligned` plus `v015_dependency_ownership_keeps_daemon_only_crates_removed` |
| v0.15.8 | Deprecated weekday profile rule shims are removed: `--weekday-rules*` is no longer parsed, config migration drops `weekday_profile_rules`, and schedules/session templates remain the replacement path. | Schedule/config cleanup | `parse_weekday_rules_is_retired` plus `migrate_deprecated_schedule_shims_removes_weekday_rules_and_automation_triggers` |
| v0.15.8 | Standalone automation trigger compatibility is removed: `--automation-triggers*` is no longer parsed, config migration drops `automation_triggers`, and schedules/session templates remain the replacement path. | Schedule/runtime cleanup | `parse_automation_triggers_is_retired` plus `parse_automation_triggers_set_is_retired` and `migrate_deprecated_schedule_shims_removes_weekday_rules_and_automation_triggers` |
| v0.15.8 | Retired blocklist category config no longer adds config-doctor warnings, while profile-level blocklist/allowlist site management remains the supported path. | Blocking cleanup | `config_doctor_omits_retired_blocklist_category_warnings` plus `parse_rejects_removed_blocklist_category_flags` |
| v0.15.4 | Core blocklist/allowlist site add, edit, delete, and list paths operate on profile-level rules without selected-category branching. | Blocking cleanup | `apply_site_add_command_uses_profile_sites_not_selected_category` plus `site_manager_uses_profile_sites_not_selected_category` |
| v0.15.8 | Temporary allowlist and break-glass workflows share one canonical temporary override status/recovery model without legacy status or recovery fields. | Blocking/runtime cleanup | `app_restores_temporary_overrides_from_canonical_snapshot` plus `build_status_output_includes_break_glass_temporary_override` and `build_status_output_includes_active_temporary_allowlist_overrides` |
| v0.15.5 | Focus History uses a stable default KPI layout while legacy dashboard customization config normalizes back to supported defaults. | Stats cleanup | `history_dashboard_uses_stable_default_layout_despite_legacy_customization` plus `apply_history_dashboard_show_uses_stable_default_layout` |
| v0.15.5 | Status comparison CLI flags stay absent from `--status`; export artifacts and Focus History remain the supported deeper comparison paths. | Stats cleanup | `parse_rejects_status_comparison_options_as_unknown_options` plus `parse_rejects_equals_status_comparison_options_as_unknown_options` |
| v0.15.5 | Backup and stats export artifact workflows share target-directory creation and JSON path contract behavior. | Artifact cleanup | `artifact_workflows_json_create_target_dirs_and_preserve_path_fields` plus `artifact_workflows_json_report_consistent_target_directory_errors` |
| v0.15.9 | Daemon local API lifecycle commands stay removed while CLI timer/session/workflow commands and the TUI remain supported replacement workflows. | Integration cleanup | `parse_rejects_retired_daemon_lifecycle_options` plus `retired_daemon_lifecycle_commands_emit_json_usage_errors` |
| v0.15.6 | WakaTime integration runtime exposes only supported tracking calls while generic lifecycle/capability extension hooks stay removed. | Integration cleanup | `poll_wakatime_events_applies_async_updates` plus `disabled_wakatime_runtime_ignores_supported_hooks` |
| v0.15.6 | Runtime dependency ownership stays documented with calendar and integration/WakaTime ownership before cleanup removals change `Cargo.toml`. | Dependency cleanup | `v015_cleanup_docs_keep_matrix_and_release_guidance_aligned` |
| v0.15.7 | Standalone usage-signal CLI access stays removed as a plain unknown option while GitHub roadmap issues and static release docs remain the cleanup planning workflow. | Usage cleanup | `v015_removed_command_paths_follow_plain_unknown_option_json_baseline` plus `v015_removed_command_text_errors_follow_plain_unknown_option_baseline` |
| v0.16.1 | Feature inventory CLI export stays retired; `--feature-inventory` is absent from help and command parsing while static cleanup docs point to GitHub roadmap issues. | Usage cleanup | `parse_feature_inventory_is_retired`, `feature_inventory_json_command_is_retired`, and `v015_cleanup_docs_keep_matrix_and_release_guidance_aligned` |
| v0.15.7 | Focus History dashboard customization CLI paths stay retired while `--history-dashboard` remains the supported stable KPI layout inspection command. | Stats cleanup | `parse_rejects_retired_history_dashboard_customization_flags` plus `classify_key_value_arg_ignores_retired_history_dashboard_customization` |
| v0.15.8 | Retired blocklist category config still migrates into profile-level `sites` and `allowlist_sites`, and category fields are not re-persisted. | Blocking cleanup | `config_migration_flattens_blocklist_categories_into_profile_rules` plus `normalize_merges_legacy_profile_lists_when_categories_exist` |

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
When cleanup work changes runtime dependencies, first confirm ownership with
`rg -n "ureq|chrono_tz|chrono-tz" src tests`, then run `cargo check --all`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --all`, and
`cargo audit` before release tagging.
