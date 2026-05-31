# Feature Inventory Report

## Scoring model

- Score range: 1 (low) to 5 (high)
- Maintenance cost formula: `complexity * 0.40 + support_burden * 0.35 + failure_impact * 0.25`
- Keep: value >= 4 and (value - maintenance_cost) >= 0.50
- Remove: (value - maintenance_cost) <= -1.50
- Merge: all remaining cases

## Summary

- Total features: 27
- Keep: 13
- Merge: 13
- Remove: 1
- Surface coverage:
  - Timer: 6
  - Schedule: 5
  - Blocker: 5
  - Stats: 6
  - Integration: 5

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
| `weekday-profile-rules` | Schedule | 4 | 3.75 | 1.07 | merge | --weekday-rules, --weekday-rules-set |
| `blocking-preview-diagnostics` | Blocker | 3 | 2.25 | 1.33 | merge | --blocking-preview |
| `blocklist-category-management` | Blocker | 3 | 3.00 | 1.00 | merge | --blocklist-category, --blocklist-category-create, --blocklist-category-rename, --blocklist-category-delete |
| `blocklist-profile-management` | Blocker | 4 | 3.25 | 1.23 | keep | --blocklist-profile, --blocklist-profile-create, --blocklist-profile-rename, --blocklist-profile-delete |
| `site-rule-management` | Blocker | 5 | 4.00 | 1.25 | keep | --blocklist-sites, --allowlist-sites, --blocklist-site-add, --allowlist-site-add, --blocklist-site-edit, --allowlist-site-edit, --blocklist-site-delete, --allowlist-site-delete |
| `temporary-allowlist-overrides` | Blocker | 3 | 3.60 | 0.83 | merge | --allowlist-site-add-temporary |
| `backup-and-restore-workflows` | Stats | 4 | 4.25 | 0.94 | merge | --backup, --restore |
| `feature-inventory-reporting` | Stats | 4 | 2.00 | 2.00 | keep | --feature-inventory |
| `history-dashboard-curation` | Stats | 4 | 2.75 | 1.45 | keep | --history-dashboard, --history-dashboard-pin, --history-dashboard-unpin, --history-dashboard-order |
| `stats-export-artifacts` | Stats | 4 | 2.65 | 1.51 | keep | --export |
| `status-comparison-slicing` | Stats | 3 | 3.35 | 0.90 | merge | --compare-by, --compare-task, --compare-profile, --compare-time, --compare-limit |
| `status-snapshot-and-streaming` | Stats | 5 | 3.65 | 1.37 | keep | --status, --watch |
| `calendar-busy-window-sync` | Integration | 3 | 3.75 | 0.80 | merge | --calendar-sync |
| `daemon-api-lifecycle` | Integration | 3 | 4.25 | 0.71 | merge | --daemon-start, --daemon-status, --daemon-stop, --daemon-port |
| `encrypted-sync-bundles` | Integration | 2 | 5.00 | 0.40 | remove | --sync-backup, --sync-restore, --sync-passphrase |
| `setup-diagnostics-and-health-signals` | Integration | 3 | 2.75 | 1.09 | merge | --diagnostics |
| `wakatime-heartbeat-pipeline` | Integration | 4 | 3.00 | 1.33 | keep | (none) |
