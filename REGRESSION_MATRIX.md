# Current Regression Matrix

This matrix tracks the supported behavior that should stay covered after cleanup,
deprecation, migration, and facade/submodule work. The old v0.14/v0.15
cleanup-specific integration gates have been archived; their still-relevant
contracts now live in normal module and integration tests that run with
`cargo test --all`.

## Coverage

| Area | Supported contract | Gate |
| --- | --- | --- |
| Config diagnostics and migration | `--diagnostics` reports current schema health, canonical profile-key guidance, deprecated field replacements, and migration guidance while retired focused config command surfaces point back to diagnostics. | `diagnostics_output_includes_config_health_and_migration_guidance`, `parse_rejects_retired_config_diagnostics_commands_with_guidance`, `retired_config_diagnostics_commands_emit_json_usage_guidance`, `config_doctor_reports_schema_and_legacy_profile_findings`, `config_doctor_detects_legacy_automation_fields_before_normalization`, `migrate_config_toml_v1_to_v2_maps_profile_ids_and_profile_automation_keys`, `migrate_config_toml_v1_to_v2_merges_legacy_profile_automation_into_existing_preset_key` |
| CLI JSON/error contract | JSON success and failure envelopes stay on stdout; unsupported options fail as usage errors without human text on stderr, and ordinary retired options do not grow replacement-only hints. | `parse_errors_in_json_mode_emit_usage_envelope`, `parse_errors_in_json_mode_preserve_contract_across_parser_stages`, `retired_options_emit_plain_json_usage_errors`, `feature_inventory_json_command_is_retired`, `retired_config_diagnostics_commands_emit_json_usage_guidance` |
| Stats persistence | Stats writes and reads use the canonical stats state path, without falling back to legacy config-dir stats files. | `task_json_writes_stats_to_canonical_path_only`, `status_json_uses_canonical_stats_without_legacy_fallback` |
| Blocking diagnostics and site rules | Blocking preview data is available through `--diagnostics`; site CRUD operates on profile-level blocklist/allowlist rules without retired category branching. | `diagnostics_json_includes_blocking_preview_payload`, `blocking_preview_json_reports_plain_unknown_option`, `apply_site_add_command_uses_profile_sites_not_selected_category`, `site_manager_uses_profile_sites_not_selected_category` |
| Schedule and calendar cleanup | Profile schedules remain the supported automation model while session template, schedule exception date, calendar annotation cache, standalone calendar refresh, weekday, and automation command paths stay absent. | `config_migration_removes_schedule_exception_dates`, `migrate_config_toml_removes_session_template_persistence`, `parse_session_template_commands_are_retired`, `parse_rejects_schedule_set_with_deprecated_exception_dates`, `load_with_env_ignores_legacy_calendar_sync_without_warning`, `save_with_env_omits_legacy_calendar_sync_section`, `recurring_schedule_text_has_no_calendar_annotations`, `parse_calendar_sync_is_retired`, `parse_weekday_rules_is_retired`, `parse_automation_triggers_is_retired`, `migrate_deprecated_schedule_shims_removes_weekday_rules_and_automation_triggers` |
| Temporary override cleanup | Break-glass and temporary allowlist workflows stay removed; status/recovery no longer emits temporary override runtime entries. | `parse_break_glass_commands_are_removed_with_guidance`, `usage_text_omits_break_glass_commands`, `temporary_allowlist_add_json_command_is_removed` |
| History and stats surfaces | Focus History keeps the stable default KPI layout; comparison and customization paths stay in supported export/history workflows; task notes, focus intentions, and task-specific goals stay retired while task labels remain available for grouping. | `history_dashboard_uses_stable_default_layout_despite_legacy_customization`, `apply_history_dashboard_show_uses_stable_default_layout`, `parse_rejects_status_comparison_options_as_unknown_options`, `parse_rejects_retired_history_dashboard_customization_flags`, `session_export_omits_task_note_metadata_fields`, `build_status_output_keeps_selected_task_label_without_task_goal`, `task_goal_json_command_is_removed`, `task_note_json_command_is_removed` |
| Artifacts | Backup and stats export artifact workflows share target-directory creation and JSON path/error contracts. | `artifact_workflows_json_create_target_dirs_and_preserve_path_fields`, `artifact_workflows_json_report_consistent_target_directory_errors` |
| Integration runtime | Daemon local API lifecycle paths remain removed; CLI timer/session/workflow commands and the TUI remain the supported automation/runtime surface. | `parse_rejects_retired_daemon_lifecycle_options`, `retired_daemon_lifecycle_commands_emit_json_usage_errors`, `usage_text_keeps_supported_cli_automation_replacements` |
| WakaTime runtime | WakaTime exposes only supported runtime calls for heartbeat polling, focus-running sync, elapsed focus tracking, and metadata updates. | `poll_wakatime_events_applies_async_updates`, `disabled_wakatime_runtime_ignores_supported_hooks` |
| Dependency ownership | Runtime HTTP and Basic auth stay owned by WakaTime, while removed daemon paths do not reintroduce direct runtime ownership and retired calendar timezone parsing stays absent. | `Cargo.toml`, `Cargo.lock`, `src/wakatime/transport.rs`, plus `rg -n "chrono_tz\|chrono-tz" src tests Cargo.toml Cargo.lock` and `rg -n "ureq" src tests` during dependency cleanup |

## Release Readiness

Run the CI-equivalent release checks for release readiness:

```sh
cargo check --all
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

When a feature path is merged, deprecated, or removed, update the relevant row
here and add or update the matching normal module or integration test before
preparing the release commit. Documentation-only roadmap updates should still
keep README, CHANGELOG.md, CONTRIBUTING.md, and this matrix aligned with
supported replacement behavior.
When cleanup work changes runtime dependencies, first confirm retired calendar
timezone parsing stays absent with `rg -n "chrono_tz|chrono-tz" src tests Cargo.toml Cargo.lock`
and confirm WakaTime HTTP ownership with `rg -n "ureq" src tests`, then run
`cargo check --all`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all`, and `cargo audit` before release tagging.
