# Feature Inventory Report

## Scoring model

- Score range: 1 (low) to 5 (high)
- Maintenance cost formula: `complexity * 0.40 + support_burden * 0.35 + failure_impact * 0.25`
- Keep: value >= 4 and (value - maintenance_cost) >= 0.50
- Remove: (value - maintenance_cost) <= -1.50
- Merge: all remaining cases
- Tie-break activation: only when delta equals keep/remove threshold within ±0.0001
- Keep tie-break (delta == 0.50): keep when any of safety/migration_risk/user_disruption >= 4
- Remove tie-break (delta == -1.50): remove when safety/migration_risk/user_disruption are all <= 2
- Tie-break dimensions: safety = failure_impact, migration_risk = complexity, user_disruption = support_burden

## Summary

- Total features: 28
- Keep: 13
- Merge: 14
- Remove: 1
- Surface coverage:
  - Timer: 6
  - Schedule: 5
  - Blocker: 5
  - Stats: 7
  - Integration: 5

## Cleanup signal support

- Usage-signal dimensions retained: commands, screens
- Usage-signal summary fields retained: total_events, unique_surfaces, top, rare
- Standalone usage-signal CLI access: removed
- Supported reporting workflow: `--feature-inventory`

## Release phase mapping (v0.14.x)

- keep: Phase 1: Stabilize — Preserve and harden high-confidence capabilities throughout v0.14.x.
- merge: Phase 2: Consolidate — Combine overlapping workflows behind unified UX/API surfaces in v0.14.x.
- remove: Phase 3: Retire — Plan sunset with migration guidance and minimal disruption by late v0.14.x.

## Feature inventory

| Feature ID | Surface | Value | Maintenance | Ratio | Recommendation | CLI flags |
| --- | --- | --- | --- | --- | --- | --- |
| `goal-management` | Timer | 4 | 3.00 | 1.33 | keep | --goal, --goal-weekly, --goal-monthly, --goal-carry, --goal-carry-weekly, --goal-carry-monthly |
| `profile-and-theme-controls` | Timer | 4 | 2.75 | 1.45 | keep | --profile, --theme |
| `session-template-workflows` | Timer | 4 | 3.75 | 1.07 | merge | --session-template, --session-template-apply, --session-template-create, --session-template-rename, --session-template-delete |
| `strict-mode-enforcement` | Timer | 3 | 3.25 | 0.92 | merge | --strict |
| `task-context-and-notes` | Timer | 5 | 3.00 | 1.67 | keep | --task, --task-goal, --focus-intention, --task-note |
| `timer-lifecycle-controls` | Timer | 5 | 2.90 | 1.72 | keep | --start, --pause, --resume, --stop, --next |
| `automation-trigger-rules` | Schedule | 3 | 4.00 | 0.75 | merge | --automation-triggers, --automation-triggers-set |
| `break-glass-workflow` | Schedule | 4 | 3.15 | 1.27 | keep | --break-glass-trigger, --break-glass-cancel |
| `schedule-definition-and-inspection` | Schedule | 5 | 3.65 | 1.37 | keep | --schedule, --schedule-set |
| `schedule-delay-controls` | Schedule | 3 | 2.25 | 1.33 | merge | --schedule-delay |
| `weekday-profile-rules` | Schedule | 2 | 2.95 | 0.68 | merge | --weekday-rules, --weekday-rules-set |
| `blocking-preview-diagnostics` | Blocker | 3 | 2.25 | 1.33 | merge | --diagnostics, --blocking-preview |
| `blocklist-category-management` | Blocker | 2 | 3.35 | 0.60 | merge | --blocklist-category, --blocklist-category-create, --blocklist-category-rename, --blocklist-category-delete |
| `blocklist-profile-management` | Blocker | 4 | 3.25 | 1.23 | keep | --blocklist-profile, --blocklist-profile-create, --blocklist-profile-rename, --blocklist-profile-delete |
| `site-rule-management` | Blocker | 5 | 4.00 | 1.25 | keep | --blocklist-sites, --allowlist-sites, --blocklist-site-add, --allowlist-site-add, --blocklist-site-edit, --allowlist-site-edit, --blocklist-site-delete, --allowlist-site-delete |
| `temporary-allowlist-overrides` | Blocker | 3 | 3.60 | 0.83 | merge | --allowlist-site-add-temporary |
| `backup-and-restore-workflows` | Stats | 4 | 4.25 | 0.94 | merge | --backup, --restore |
| `feature-inventory-reporting` | Stats | 4 | 2.00 | 2.00 | keep | --feature-inventory |
| `history-dashboard-curation` | Stats | 4 | 2.75 | 1.45 | keep | --history-dashboard, --history-dashboard-pin, --history-dashboard-unpin, --history-dashboard-order |
| `stats-export-artifacts` | Stats | 4 | 2.65 | 1.51 | keep | --export |
| `status-comparison-slicing` | Stats | 1 | 2.00 | 0.50 | merge | --compare-by, --compare-task, --compare-profile, --compare-time, --compare-limit |
| `status-snapshot-and-streaming` | Stats | 5 | 3.65 | 1.37 | keep | --status, --watch |
| `usage-signal-cleanup-support` | Stats | 3 | 1.40 | 2.14 | merge | (none) |
| `calendar-busy-window-sync` | Integration | 2 | 3.10 | 0.65 | merge | --calendar-sync |
| `daemon-api-lifecycle` | Integration | 3 | 4.25 | 0.71 | merge | --daemon-start, --daemon-status, --daemon-stop, --daemon-port |
| `retired-low-value-command-guidance` | Integration | 1 | 3.50 | 0.29 | remove | --migrate, --dry-run, --sync-backup, --sync-restore, --sync-passphrase |
| `setup-diagnostics-and-health-signals` | Integration | 3 | 2.75 | 1.09 | merge | --diagnostics, --config-doctor, --config-migrate, --config-migrate-apply |
| `wakatime-heartbeat-pipeline` | Integration | 4 | 3.00 | 1.33 | keep | (none) |
