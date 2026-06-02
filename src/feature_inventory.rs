use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
};

use serde::Serialize;

pub const FEATURE_INVENTORY_JSON_FILE_NAME: &str = "FEATURE_INVENTORY.json";
pub const FEATURE_INVENTORY_MARKDOWN_FILE_NAME: &str = "FEATURE_INVENTORY.md";

const SCHEMA_VERSION: u8 = 3;
const COMPLEXITY_WEIGHT: f64 = 0.40;
const SUPPORT_BURDEN_WEIGHT: f64 = 0.35;
const FAILURE_IMPACT_WEIGHT: f64 = 0.25;
const KEEP_MIN_VALUE: u8 = 4;
const KEEP_MIN_DELTA: f64 = 0.50;
const REMOVE_MAX_DELTA: f64 = -1.50;
const TIE_BREAK_BOUNDARY_EPSILON: f64 = 0.0001;
const TIE_BREAK_KEEP_SIGNAL_MIN: u8 = 4;
const TIE_BREAK_REMOVE_SIGNAL_MAX: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FeatureSurface {
    Timer,
    Schedule,
    Blocker,
    Stats,
    Integration,
}

impl FeatureSurface {
    #[cfg(test)]
    pub const ALL: [Self; 5] = [
        Self::Timer,
        Self::Schedule,
        Self::Blocker,
        Self::Stats,
        Self::Integration,
    ];
}

impl std::fmt::Display for FeatureSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Timer => "Timer",
            Self::Schedule => "Schedule",
            Self::Blocker => "Blocker",
            Self::Stats => "Stats",
            Self::Integration => "Integration",
        };
        write!(f, "{label}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureRecommendation {
    Keep,
    Merge,
    Remove,
}

impl FeatureRecommendation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Merge => "merge",
            Self::Remove => "remove",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeprecationStage {
    Warning,
    MigrationGuidance,
    Removal,
}

impl DeprecationStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::MigrationGuidance => "migration_guidance",
            Self::Removal => "removal",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureInventoryReport {
    pub schema_version: u8,
    pub scoring_model: FeatureScoringModel,
    pub summary: FeatureInventorySummary,
    pub features: Vec<FeatureInventoryEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureScoringModel {
    pub score_min: u8,
    pub score_max: u8,
    pub complexity_weight: f64,
    pub support_burden_weight: f64,
    pub failure_impact_weight: f64,
    pub keep_min_value: u8,
    pub keep_min_delta: f64,
    pub remove_max_delta: f64,
    pub tie_break_model: TieBreakModel,
    pub release_phase_mapping: Vec<RecommendationReleasePhase>,
    pub deprecation_pipeline: Vec<DeprecationStagePolicy>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TieBreakModel {
    pub boundary_epsilon: f64,
    pub keep_signal_min: u8,
    pub remove_signal_max: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecommendationReleasePhase {
    pub recommendation: FeatureRecommendation,
    pub phase: &'static str,
    pub objective: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeprecationStagePolicy {
    pub stage: DeprecationStage,
    pub enforcement: &'static str,
    pub user_notice: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureInventorySummary {
    pub total_features: usize,
    pub keep_count: usize,
    pub merge_count: usize,
    pub remove_count: usize,
    pub by_surface: Vec<SurfaceSummary>,
    pub covered_cli_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceSummary {
    pub surface: FeatureSurface,
    pub feature_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureInventoryEntry {
    pub feature_id: String,
    pub name: String,
    pub surface: FeatureSurface,
    pub description: String,
    pub cli_flags: Vec<String>,
    pub value: u8,
    pub complexity: u8,
    pub support_burden: u8,
    pub failure_impact: u8,
    pub maintenance_cost: f64,
    pub value_to_maintenance_ratio: f64,
    pub recommendation: FeatureRecommendation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<FeatureDeprecationInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureDeprecationInfo {
    pub warning_from_version: String,
    pub migration_from_version: String,
    pub removal_from_version: String,
    pub migration_guidance: String,
    pub release_notes_hook: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_stage: Option<DeprecationStage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDeprecationNotice {
    pub feature_id: &'static str,
    pub feature_name: &'static str,
    pub stage: DeprecationStage,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct FeatureInventoryExportPaths {
    pub json_path: PathBuf,
    pub markdown_path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct FeatureSeed {
    feature_id: &'static str,
    name: &'static str,
    surface: FeatureSurface,
    description: &'static str,
    cli_flags: &'static [&'static str],
    value: u8,
    complexity: u8,
    support_burden: u8,
    failure_impact: u8,
}

#[derive(Debug, Clone, Copy)]
struct TieBreakSignals {
    safety: u8,
    migration_risk: u8,
    user_disruption: u8,
}

#[derive(Debug, Clone, Copy)]
struct FeatureDeprecationSeed {
    feature_id: &'static str,
    warning_from_version: &'static str,
    migration_from_version: &'static str,
    removal_from_version: &'static str,
    migration_guidance: &'static str,
    release_notes_hook: &'static str,
}

const FEATURE_SEEDS: &[FeatureSeed] = &[
    FeatureSeed {
        feature_id: "timer-lifecycle-controls",
        name: "Timer lifecycle controls",
        surface: FeatureSurface::Timer,
        description: "Start, pause, resume, stop, and skip timer phases from CLI and TUI workflows.",
        cli_flags: &["--start", "--pause", "--resume", "--stop", "--next"],
        value: 5,
        complexity: 3,
        support_burden: 2,
        failure_impact: 4,
    },
    FeatureSeed {
        feature_id: "task-context-and-notes",
        name: "Task context and notes",
        surface: FeatureSurface::Timer,
        description: "Attach task labels, goals, intentions, and notes to active focus sessions.",
        cli_flags: &["--task", "--task-goal", "--focus-intention", "--task-note"],
        value: 5,
        complexity: 3,
        support_burden: 3,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "profile-and-theme-controls",
        name: "Profile and theme controls",
        surface: FeatureSurface::Timer,
        description: "Switch duration profiles and UI theme presets for personal workflow tuning.",
        cli_flags: &["--profile", "--theme"],
        value: 4,
        complexity: 3,
        support_burden: 3,
        failure_impact: 2,
    },
    FeatureSeed {
        feature_id: "goal-management",
        name: "Goal management",
        surface: FeatureSurface::Timer,
        description: "Set and inspect daily, weekly, and monthly focus goals with carry-over options.",
        cli_flags: &[
            "--goal",
            "--goal-weekly",
            "--goal-monthly",
            "--goal-carry",
            "--goal-carry-weekly",
            "--goal-carry-monthly",
        ],
        value: 4,
        complexity: 3,
        support_burden: 3,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "strict-mode-enforcement",
        name: "Strict mode enforcement",
        surface: FeatureSurface::Timer,
        description: "Lock timer behavior to reduce drift and prevent accidental workflow bypasses.",
        cli_flags: &["--strict"],
        value: 3,
        complexity: 3,
        support_burden: 3,
        failure_impact: 4,
    },
    FeatureSeed {
        feature_id: "session-template-workflows",
        name: "Session template workflows",
        surface: FeatureSurface::Timer,
        description: "Capture and apply session templates that bundle task, profile, schedule, and blocklist defaults.",
        cli_flags: &[
            "--session-template",
            "--session-template-apply",
            "--session-template-create",
            "--session-template-rename",
            "--session-template-delete",
        ],
        value: 4,
        complexity: 4,
        support_burden: 4,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "schedule-definition-and-inspection",
        name: "Schedule definition and inspection",
        surface: FeatureSurface::Schedule,
        description: "Review and set recurring schedules used to drive automatic focus windows.",
        cli_flags: &["--schedule", "--schedule-set"],
        value: 5,
        complexity: 4,
        support_burden: 3,
        failure_impact: 4,
    },
    FeatureSeed {
        feature_id: "weekday-profile-rules",
        name: "Weekday profile rules",
        surface: FeatureSurface::Schedule,
        description: "Map weekdays to profile selection rules for context-sensitive defaults.",
        cli_flags: &["--weekday-rules", "--weekday-rules-set"],
        value: 4,
        complexity: 4,
        support_burden: 4,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "automation-trigger-rules",
        name: "Automation trigger rules",
        surface: FeatureSurface::Schedule,
        description: "Configure trigger conditions that launch automation around focus sessions.",
        cli_flags: &["--automation-triggers", "--automation-triggers-set"],
        value: 3,
        complexity: 4,
        support_burden: 4,
        failure_impact: 4,
    },
    FeatureSeed {
        feature_id: "schedule-delay-controls",
        name: "Schedule delay controls",
        surface: FeatureSurface::Schedule,
        description: "Delay upcoming schedule windows to handle ad-hoc interruptions.",
        cli_flags: &["--schedule-delay"],
        value: 3,
        complexity: 2,
        support_burden: 2,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "break-glass-workflow",
        name: "Break-glass workflow",
        surface: FeatureSurface::Schedule,
        description: "Temporarily suspend enforcement and explicitly cancel active break-glass windows.",
        cli_flags: &["--break-glass-trigger", "--break-glass-cancel"],
        value: 4,
        complexity: 3,
        support_burden: 2,
        failure_impact: 5,
    },
    FeatureSeed {
        feature_id: "blocklist-profile-management",
        name: "Blocklist profile management",
        surface: FeatureSurface::Blocker,
        description: "Create, rename, select, and delete blocker profiles per workflow context.",
        cli_flags: &[
            "--blocklist-profile",
            "--blocklist-profile-create",
            "--blocklist-profile-rename",
            "--blocklist-profile-delete",
        ],
        value: 4,
        complexity: 3,
        support_burden: 3,
        failure_impact: 4,
    },
    FeatureSeed {
        feature_id: "blocklist-category-management",
        name: "Blocklist category management",
        surface: FeatureSurface::Blocker,
        description: "Manage blocker categories nested under each blocklist profile.",
        cli_flags: &[
            "--blocklist-category",
            "--blocklist-category-create",
            "--blocklist-category-rename",
            "--blocklist-category-delete",
        ],
        value: 3,
        complexity: 3,
        support_burden: 3,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "site-rule-management",
        name: "Site rule management",
        surface: FeatureSurface::Blocker,
        description: "List, add, edit, and delete blocklist and allowlist host rules.",
        cli_flags: &[
            "--blocklist-sites",
            "--allowlist-sites",
            "--blocklist-site-add",
            "--allowlist-site-add",
            "--blocklist-site-edit",
            "--allowlist-site-edit",
            "--blocklist-site-delete",
            "--allowlist-site-delete",
        ],
        value: 5,
        complexity: 4,
        support_burden: 4,
        failure_impact: 4,
    },
    FeatureSeed {
        feature_id: "temporary-allowlist-overrides",
        name: "Temporary allowlist overrides",
        surface: FeatureSurface::Blocker,
        description: "Grant temporary unblock windows for selected hosts without removing baseline rules.",
        cli_flags: &["--allowlist-site-add-temporary"],
        value: 3,
        complexity: 3,
        support_burden: 4,
        failure_impact: 4,
    },
    FeatureSeed {
        feature_id: "blocking-preview-diagnostics",
        name: "Blocking preview diagnostics",
        surface: FeatureSurface::Blocker,
        description: "Preview effective blocker resolution and inspect command/backend actions.",
        cli_flags: &["--blocking-preview"],
        value: 3,
        complexity: 2,
        support_burden: 2,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "status-snapshot-and-streaming",
        name: "Status snapshot and streaming",
        surface: FeatureSurface::Stats,
        description: "Inspect current timer state and optional watch mode for live status updates.",
        cli_flags: &["--status", "--watch"],
        value: 5,
        complexity: 4,
        support_burden: 3,
        failure_impact: 4,
    },
    FeatureSeed {
        feature_id: "status-comparison-slicing",
        name: "Status comparison slicing",
        surface: FeatureSurface::Stats,
        description: "Compare status analytics across task, profile, and time-of-day dimensions.",
        cli_flags: &[
            "--compare-by",
            "--compare-task",
            "--compare-profile",
            "--compare-time",
            "--compare-limit",
        ],
        value: 3,
        complexity: 3,
        support_burden: 4,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "history-dashboard-curation",
        name: "History dashboard curation",
        surface: FeatureSurface::Stats,
        description: "Pin, unpin, and reorder history dashboard KPI cards to fit user priorities.",
        cli_flags: &[
            "--history-dashboard",
            "--history-dashboard-pin",
            "--history-dashboard-unpin",
            "--history-dashboard-order",
        ],
        value: 4,
        complexity: 3,
        support_burden: 3,
        failure_impact: 2,
    },
    FeatureSeed {
        feature_id: "usage-signal-inspection",
        name: "Usage signal inspection",
        surface: FeatureSurface::Stats,
        description: "Review command and screen frequency summaries to guide feature cleanup decisions.",
        cli_flags: &["--usage-signals"],
        value: 3,
        complexity: 2,
        support_burden: 2,
        failure_impact: 2,
    },
    FeatureSeed {
        feature_id: "stats-export-artifacts",
        name: "Stats export artifacts",
        surface: FeatureSurface::Stats,
        description: "Export machine-readable and spreadsheet-friendly focus history artifacts.",
        cli_flags: &["--export"],
        value: 4,
        complexity: 3,
        support_burden: 2,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "feature-inventory-reporting",
        name: "Feature inventory reporting",
        surface: FeatureSurface::Stats,
        description: "Generate scored feature inventory artifacts for keep, merge, and remove roadmap reviews.",
        cli_flags: &["--feature-inventory"],
        value: 4,
        complexity: 2,
        support_burden: 2,
        failure_impact: 2,
    },
    FeatureSeed {
        feature_id: "backup-and-restore-workflows",
        name: "Backup and restore workflows",
        surface: FeatureSurface::Stats,
        description: "Create and recover local backup bundles containing configuration and session history.",
        cli_flags: &["--backup", "--restore"],
        value: 4,
        complexity: 4,
        support_burden: 4,
        failure_impact: 5,
    },
    FeatureSeed {
        feature_id: "daemon-api-lifecycle",
        name: "Daemon API lifecycle",
        surface: FeatureSurface::Integration,
        description: "Run and manage daemon mode for automation and external tooling integration.",
        cli_flags: &[
            "--daemon-start",
            "--daemon-status",
            "--daemon-stop",
            "--daemon-port",
        ],
        value: 3,
        complexity: 4,
        support_burden: 4,
        failure_impact: 5,
    },
    FeatureSeed {
        feature_id: "calendar-busy-window-sync",
        name: "Calendar busy window sync",
        surface: FeatureSurface::Integration,
        description: "Sync active focus sessions into calendar busy windows to reduce interruptions.",
        cli_flags: &["--calendar-sync"],
        value: 3,
        complexity: 4,
        support_burden: 4,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "encrypted-sync-bundles",
        name: "Encrypted sync bundles",
        surface: FeatureSurface::Integration,
        description: "Create encrypted backup bundles and restore them with passphrase protection.",
        cli_flags: &["--sync-backup", "--sync-restore", "--sync-passphrase"],
        value: 2,
        complexity: 5,
        support_burden: 5,
        failure_impact: 5,
    },
    FeatureSeed {
        feature_id: "wakatime-heartbeat-pipeline",
        name: "WakaTime heartbeat pipeline",
        surface: FeatureSurface::Integration,
        description: "Publish coding heartbeats to WakaTime using active timer metadata.",
        cli_flags: &[],
        value: 4,
        complexity: 3,
        support_burden: 3,
        failure_impact: 3,
    },
    FeatureSeed {
        feature_id: "setup-diagnostics-and-health-signals",
        name: "Setup diagnostics and health signals",
        surface: FeatureSurface::Integration,
        description: "Provide system diagnostics and setup checks for blocker, notifications, and integrations.",
        cli_flags: &["--diagnostics"],
        value: 3,
        complexity: 3,
        support_burden: 3,
        failure_impact: 2,
    },
];

const FEATURE_DEPRECATION_SEEDS: &[FeatureDeprecationSeed] = &[FeatureDeprecationSeed {
    feature_id: "encrypted-sync-bundles",
    warning_from_version: "0.14.2",
    migration_from_version: "0.15.0",
    removal_from_version: "0.16.0",
    migration_guidance: "Switch to local backup/restore workflows (`--backup`, `--restore`) for portable recovery.",
    release_notes_hook: "Deprecated encrypted sync bundle commands (`--sync-backup`, `--sync-restore`) and directed users to backup/restore workflows.",
}];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SemanticVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

pub fn build_feature_inventory_report() -> FeatureInventoryReport {
    build_feature_inventory_report_for_version(env!("CARGO_PKG_VERSION"))
}

pub(crate) fn build_feature_inventory_report_for_version(
    current_version: &str,
) -> FeatureInventoryReport {
    let mut features = FEATURE_SEEDS
        .iter()
        .map(|seed| build_feature_entry(seed, current_version))
        .collect::<Vec<_>>();
    features.sort_by(|left, right| {
        left.surface
            .cmp(&right.surface)
            .then_with(|| left.feature_id.cmp(&right.feature_id))
    });

    let summary = build_summary(&features);

    FeatureInventoryReport {
        schema_version: SCHEMA_VERSION,
        scoring_model: FeatureScoringModel {
            score_min: 1,
            score_max: 5,
            complexity_weight: COMPLEXITY_WEIGHT,
            support_burden_weight: SUPPORT_BURDEN_WEIGHT,
            failure_impact_weight: FAILURE_IMPACT_WEIGHT,
            keep_min_value: KEEP_MIN_VALUE,
            keep_min_delta: KEEP_MIN_DELTA,
            remove_max_delta: REMOVE_MAX_DELTA,
            tie_break_model: TieBreakModel {
                boundary_epsilon: TIE_BREAK_BOUNDARY_EPSILON,
                keep_signal_min: TIE_BREAK_KEEP_SIGNAL_MIN,
                remove_signal_max: TIE_BREAK_REMOVE_SIGNAL_MAX,
            },
            release_phase_mapping: build_release_phase_mapping(),
            deprecation_pipeline: build_deprecation_pipeline(),
        },
        summary,
        features,
    }
}

pub fn export_feature_inventory_report(
    dir: &Path,
    report: &FeatureInventoryReport,
) -> io::Result<FeatureInventoryExportPaths> {
    fs::create_dir_all(dir)?;

    let json_path = dir.join(FEATURE_INVENTORY_JSON_FILE_NAME);
    let markdown_path = dir.join(FEATURE_INVENTORY_MARKDOWN_FILE_NAME);

    let json_payload = serde_json::to_string_pretty(report).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to serialize feature inventory report: {error}"),
        )
    })?;
    fs::write(&json_path, json_payload)?;
    fs::write(&markdown_path, render_markdown_report(report))?;

    Ok(FeatureInventoryExportPaths {
        json_path,
        markdown_path,
    })
}

pub fn render_markdown_report(report: &FeatureInventoryReport) -> String {
    let model = &report.scoring_model;
    let tie_break = &model.tie_break_model;
    let mut markdown = String::new();

    markdown.push_str("# Feature Inventory Report\n\n");
    markdown.push_str("## Scoring model\n\n");
    markdown.push_str("- Score range: 1 (low) to 5 (high)\n");
    markdown.push_str(&format!(
        "- Maintenance cost formula: `complexity * {:.2} + support_burden * {:.2} + failure_impact * {:.2}`\n",
        COMPLEXITY_WEIGHT, SUPPORT_BURDEN_WEIGHT, FAILURE_IMPACT_WEIGHT
    ));
    markdown.push_str(&format!(
        "- Keep: value >= {} and (value - maintenance_cost) >= {:.2}\n",
        model.keep_min_value, model.keep_min_delta
    ));
    markdown.push_str(&format!(
        "- Remove: (value - maintenance_cost) <= {:.2}\n",
        model.remove_max_delta
    ));
    markdown.push_str("- Merge: all remaining cases\n");
    markdown.push_str(&format!(
        "- Tie-break activation: only when delta equals keep/remove threshold within ±{:.4}\n",
        tie_break.boundary_epsilon
    ));
    markdown.push_str(&format!(
        "- Keep tie-break (delta == {:.2}): keep when any of safety/migration_risk/user_disruption >= {}\n",
        model.keep_min_delta, tie_break.keep_signal_min
    ));
    markdown.push_str(&format!(
        "- Remove tie-break (delta == {:.2}): remove when safety/migration_risk/user_disruption are all <= {}\n",
        model.remove_max_delta, tie_break.remove_signal_max
    ));
    markdown.push_str("- Tie-break dimensions: safety = failure_impact, migration_risk = complexity, user_disruption = support_burden\n\n");
    markdown.push_str("- Deprecation pipeline stages:\n");
    for policy in &model.deprecation_pipeline {
        markdown.push_str(&format!(
            "  - {}: {} ({})\n",
            policy.stage.as_str(),
            policy.user_notice,
            policy.enforcement
        ));
    }
    markdown.push('\n');

    markdown.push_str("## Summary\n\n");
    markdown.push_str(&format!(
        "- Total features: {}\n- Keep: {}\n- Merge: {}\n- Remove: {}\n",
        report.summary.total_features,
        report.summary.keep_count,
        report.summary.merge_count,
        report.summary.remove_count
    ));
    markdown.push_str("- Surface coverage:\n");
    for summary in &report.summary.by_surface {
        markdown.push_str(&format!(
            "  - {}: {}\n",
            summary.surface, summary.feature_count
        ));
    }
    markdown.push('\n');

    markdown.push_str("## Release phase mapping (v0.14.x)\n\n");
    for mapping in &report.scoring_model.release_phase_mapping {
        markdown.push_str(&format!(
            "- {}: {} — {}\n",
            mapping.recommendation.as_str(),
            mapping.phase,
            mapping.objective
        ));
    }
    markdown.push('\n');

    markdown.push_str("## Low-value deprecation schedule controls\n\n");
    for feature in report
        .features
        .iter()
        .filter(|entry| entry.deprecation.is_some())
    {
        let Some(deprecation) = feature.deprecation.as_ref() else {
            continue;
        };
        let active_stage = deprecation
            .active_stage
            .map(DeprecationStage::as_str)
            .unwrap_or("inactive");
        markdown.push_str(&format!(
            "- `{}`: warning {} -> migration {} -> removal {} (active: {})\n  - Migration guidance: {}\n  - Release notes hook: {}\n",
            feature.feature_id,
            deprecation.warning_from_version,
            deprecation.migration_from_version,
            deprecation.removal_from_version,
            active_stage,
            deprecation.migration_guidance,
            deprecation.release_notes_hook
        ));
    }
    markdown.push('\n');

    markdown.push_str("## Feature inventory\n\n");
    markdown.push_str(
        "| Feature ID | Surface | Value | Maintenance | Ratio | Recommendation | CLI flags |\n",
    );
    markdown.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");

    for feature in &report.features {
        let flags = if feature.cli_flags.is_empty() {
            "(none)".to_string()
        } else {
            feature.cli_flags.join(", ")
        };
        markdown.push_str(&format!(
            "| `{}` | {} | {} | {:.2} | {:.2} | {} | {} |\n",
            feature.feature_id,
            feature.surface,
            feature.value,
            feature.maintenance_cost,
            feature.value_to_maintenance_ratio,
            feature.recommendation.as_str(),
            flags
        ));
    }

    markdown
}

fn build_feature_entry(seed: &FeatureSeed, current_version: &str) -> FeatureInventoryEntry {
    let maintenance_cost = round_to_two_decimals(
        f64::from(seed.complexity) * COMPLEXITY_WEIGHT
            + f64::from(seed.support_burden) * SUPPORT_BURDEN_WEIGHT
            + f64::from(seed.failure_impact) * FAILURE_IMPACT_WEIGHT,
    );
    let ratio = round_to_two_decimals(f64::from(seed.value) / maintenance_cost);
    let recommendation = classify_recommendation(
        seed.value,
        maintenance_cost,
        tie_break_signals_from_seed(seed),
    );
    let deprecation = deprecation_info_for_feature(seed.feature_id, current_version);

    FeatureInventoryEntry {
        feature_id: seed.feature_id.to_string(),
        name: seed.name.to_string(),
        surface: seed.surface,
        description: seed.description.to_string(),
        cli_flags: seed
            .cli_flags
            .iter()
            .map(|flag| (*flag).to_string())
            .collect(),
        value: seed.value,
        complexity: seed.complexity,
        support_burden: seed.support_burden,
        failure_impact: seed.failure_impact,
        maintenance_cost,
        value_to_maintenance_ratio: ratio,
        recommendation,
        deprecation,
    }
}

fn build_summary(features: &[FeatureInventoryEntry]) -> FeatureInventorySummary {
    let mut by_surface_map = BTreeMap::new();
    let mut keep_count = 0;
    let mut merge_count = 0;
    let mut remove_count = 0;

    for feature in features {
        *by_surface_map.entry(feature.surface).or_insert(0_usize) += 1;
        match feature.recommendation {
            FeatureRecommendation::Keep => keep_count += 1,
            FeatureRecommendation::Merge => merge_count += 1,
            FeatureRecommendation::Remove => remove_count += 1,
        }
    }

    let mut covered_cli_flags = covered_cli_flags()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    covered_cli_flags.sort();

    FeatureInventorySummary {
        total_features: features.len(),
        keep_count,
        merge_count,
        remove_count,
        by_surface: by_surface_map
            .into_iter()
            .map(|(surface, feature_count)| SurfaceSummary {
                surface,
                feature_count,
            })
            .collect(),
        covered_cli_flags,
    }
}

fn tie_break_signals_from_seed(seed: &FeatureSeed) -> TieBreakSignals {
    TieBreakSignals {
        safety: seed.failure_impact,
        migration_risk: seed.complexity,
        user_disruption: seed.support_burden,
    }
}

fn build_release_phase_mapping() -> Vec<RecommendationReleasePhase> {
    vec![
        RecommendationReleasePhase {
            recommendation: FeatureRecommendation::Keep,
            phase: "Phase 1: Stabilize",
            objective: "Preserve and harden high-confidence capabilities throughout v0.14.x.",
        },
        RecommendationReleasePhase {
            recommendation: FeatureRecommendation::Merge,
            phase: "Phase 2: Consolidate",
            objective: "Combine overlapping workflows behind unified UX/API surfaces in v0.14.x.",
        },
        RecommendationReleasePhase {
            recommendation: FeatureRecommendation::Remove,
            phase: "Phase 3: Retire",
            objective: "Plan sunset with migration guidance and minimal disruption by late v0.14.x.",
        },
    ]
}

fn build_deprecation_pipeline() -> Vec<DeprecationStagePolicy> {
    vec![
        DeprecationStagePolicy {
            stage: DeprecationStage::Warning,
            enforcement: "allow_command",
            user_notice: "Show warning banner and migration reminder while command remains available.",
        },
        DeprecationStagePolicy {
            stage: DeprecationStage::MigrationGuidance,
            enforcement: "allow_command",
            user_notice: "Show migration-first notice with explicit replacement workflow guidance.",
        },
        DeprecationStagePolicy {
            stage: DeprecationStage::Removal,
            enforcement: "block_command",
            user_notice: "Reject command with explicit migration guidance and release-notes remediation text.",
        },
    ]
}

pub fn command_deprecation_notice_for_version(
    command_id: &str,
    current_version: &str,
) -> Option<CommandDeprecationNotice> {
    let normalized_command_id = normalize_surface_id(command_id)?;
    for &seed in FEATURE_DEPRECATION_SEEDS {
        let Some(stage) = active_stage_for_deprecation_seed(seed, current_version) else {
            continue;
        };
        let Some(feature_seed) = feature_seed_for_id(seed.feature_id) else {
            continue;
        };
        let matches_command = feature_seed
            .cli_flags
            .iter()
            .filter_map(|flag| flag.strip_prefix("--"))
            .any(|flag| flag == normalized_command_id);
        if !matches_command {
            continue;
        }
        let message =
            format_command_stage_message(feature_seed, seed, stage, normalized_command_id);
        return Some(CommandDeprecationNotice {
            feature_id: seed.feature_id,
            feature_name: feature_seed.name,
            stage,
            message,
        });
    }
    None
}

pub fn active_usage_deprecation_warnings(
    mut command_usage_count: impl FnMut(&str) -> u64,
) -> Vec<String> {
    active_usage_deprecation_warnings_for_version(env!("CARGO_PKG_VERSION"), |command_id| {
        command_usage_count(command_id)
    })
}

pub fn active_usage_deprecation_warnings_for_version(
    current_version: &str,
    mut command_usage_count: impl FnMut(&str) -> u64,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for &seed in FEATURE_DEPRECATION_SEEDS {
        let Some(stage) = active_stage_for_deprecation_seed(seed, current_version) else {
            continue;
        };
        let Some(feature_seed) = feature_seed_for_id(seed.feature_id) else {
            continue;
        };
        let used = feature_seed
            .cli_flags
            .iter()
            .filter_map(|flag| flag.strip_prefix("--"))
            .any(|command_id| command_usage_count(command_id) > 0);
        if !used {
            continue;
        }
        warnings.push(format_feature_stage_warning(feature_seed, seed, stage));
    }
    warnings.sort();
    warnings
}

fn deprecation_info_for_feature(
    feature_id: &str,
    current_version: &str,
) -> Option<FeatureDeprecationInfo> {
    let seed = deprecation_seed_for_feature(feature_id)?;
    Some(FeatureDeprecationInfo {
        warning_from_version: seed.warning_from_version.to_string(),
        migration_from_version: seed.migration_from_version.to_string(),
        removal_from_version: seed.removal_from_version.to_string(),
        migration_guidance: seed.migration_guidance.to_string(),
        release_notes_hook: seed.release_notes_hook.to_string(),
        active_stage: active_stage_for_deprecation_seed(seed, current_version),
    })
}

fn deprecation_seed_for_feature(feature_id: &str) -> Option<FeatureDeprecationSeed> {
    FEATURE_DEPRECATION_SEEDS
        .iter()
        .copied()
        .find(|seed| seed.feature_id == feature_id)
}

fn feature_seed_for_id(feature_id: &str) -> Option<&'static FeatureSeed> {
    FEATURE_SEEDS
        .iter()
        .find(|seed| seed.feature_id == feature_id)
}

fn active_stage_for_deprecation_seed(
    seed: FeatureDeprecationSeed,
    current_version: &str,
) -> Option<DeprecationStage> {
    let current = parse_semantic_version(current_version)?;
    let warning_from = parse_semantic_version(seed.warning_from_version)?;
    let migration_from = parse_semantic_version(seed.migration_from_version)?;
    let removal_from = parse_semantic_version(seed.removal_from_version)?;
    if current < warning_from {
        None
    } else if current >= removal_from {
        Some(DeprecationStage::Removal)
    } else if current >= migration_from {
        Some(DeprecationStage::MigrationGuidance)
    } else {
        Some(DeprecationStage::Warning)
    }
}

fn parse_semantic_version(raw: &str) -> Option<SemanticVersion> {
    let core = raw
        .trim()
        .split_once('-')
        .map_or(raw.trim(), |(version, _)| version);
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    Some(SemanticVersion {
        major,
        minor,
        patch,
    })
}

fn normalize_surface_id(value: &str) -> Option<&str> {
    let normalized = value.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn format_feature_stage_warning(
    feature_seed: &FeatureSeed,
    deprecation_seed: FeatureDeprecationSeed,
    stage: DeprecationStage,
) -> String {
    let commands = feature_seed.cli_flags.to_vec().join(", ");
    format!(
        "Low-value feature `{}` ({commands}) is in `{}` stage (warning {} -> migration {} -> removal {}). {} Release notes hook: {}",
        feature_seed.name,
        stage.as_str(),
        deprecation_seed.warning_from_version,
        deprecation_seed.migration_from_version,
        deprecation_seed.removal_from_version,
        deprecation_seed.migration_guidance,
        deprecation_seed.release_notes_hook
    )
}

fn format_command_stage_message(
    feature_seed: &FeatureSeed,
    deprecation_seed: FeatureDeprecationSeed,
    stage: DeprecationStage,
    command_id: &str,
) -> String {
    let flag = format!("--{command_id}");
    match stage {
        DeprecationStage::Warning => format!(
            "Warning: `{flag}` is in deprecation warning stage for low-value feature `{}`. Migration guidance: {} (planned removal {}).",
            feature_seed.name,
            deprecation_seed.migration_guidance,
            deprecation_seed.removal_from_version
        ),
        DeprecationStage::MigrationGuidance => format!(
            "Warning: `{flag}` is in migration guidance stage for low-value feature `{}`. {} (planned removal {}).",
            feature_seed.name,
            deprecation_seed.migration_guidance,
            deprecation_seed.removal_from_version
        ),
        DeprecationStage::Removal => format!(
            "`{flag}` has been removed for low-value feature `{}` (removal milestone {}). {}",
            feature_seed.name,
            deprecation_seed.removal_from_version,
            deprecation_seed.migration_guidance
        ),
    }
}

fn classify_recommendation(
    value: u8,
    maintenance_cost: f64,
    tie_break_signals: TieBreakSignals,
) -> FeatureRecommendation {
    let delta = f64::from(value) - maintenance_cost;
    let is_keep = value >= KEEP_MIN_VALUE
        && (delta > KEEP_MIN_DELTA
            || (approximately_equal(delta, KEEP_MIN_DELTA)
                && keep_boundary_tie_break_prefers_keep(tie_break_signals)));
    let is_remove = delta < REMOVE_MAX_DELTA
        || (approximately_equal(delta, REMOVE_MAX_DELTA)
            && remove_boundary_tie_break_prefers_remove(tie_break_signals));

    if is_keep {
        FeatureRecommendation::Keep
    } else if is_remove {
        FeatureRecommendation::Remove
    } else {
        FeatureRecommendation::Merge
    }
}

fn approximately_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= TIE_BREAK_BOUNDARY_EPSILON
}

fn keep_boundary_tie_break_prefers_keep(signals: TieBreakSignals) -> bool {
    signals.safety >= TIE_BREAK_KEEP_SIGNAL_MIN
        || signals.migration_risk >= TIE_BREAK_KEEP_SIGNAL_MIN
        || signals.user_disruption >= TIE_BREAK_KEEP_SIGNAL_MIN
}

fn remove_boundary_tie_break_prefers_remove(signals: TieBreakSignals) -> bool {
    signals.safety <= TIE_BREAK_REMOVE_SIGNAL_MAX
        && signals.migration_risk <= TIE_BREAK_REMOVE_SIGNAL_MAX
        && signals.user_disruption <= TIE_BREAK_REMOVE_SIGNAL_MAX
}

fn round_to_two_decimals(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn covered_cli_flags() -> HashSet<&'static str> {
    FEATURE_SEEDS
        .iter()
        .flat_map(|seed| seed.cli_flags.iter().copied())
        .collect::<HashSet<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_tracks_rubric_contract() {
        assert_eq!(SCHEMA_VERSION, 3);
    }

    #[test]
    fn inventory_covers_required_surfaces() {
        let report = build_feature_inventory_report();

        for surface in FeatureSurface::ALL {
            assert!(
                report.features.iter().any(|entry| entry.surface == surface),
                "missing inventory entries for surface: {surface}"
            );
        }
    }

    #[test]
    fn inventory_maps_all_cli_options() {
        let all_option_flags = collect_usage_option_flags();
        let covered_flags = covered_cli_flags()
            .into_iter()
            .map(str::to_string)
            .collect::<HashSet<_>>();
        let ignored_flags = ["--json", "--help"]
            .into_iter()
            .map(str::to_string)
            .collect::<HashSet<_>>();

        let missing = all_option_flags
            .difference(&ignored_flags)
            .filter(|flag| !covered_flags.contains(flag.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        assert!(
            missing.is_empty(),
            "feature inventory is missing CLI flag coverage for: {}",
            missing.join(", ")
        );
    }

    #[test]
    fn recommendation_buckets_are_populated() {
        let report = build_feature_inventory_report();

        assert!(
            report
                .features
                .iter()
                .any(|entry| entry.recommendation == FeatureRecommendation::Keep)
        );
        assert!(
            report
                .features
                .iter()
                .any(|entry| entry.recommendation == FeatureRecommendation::Merge)
        );
        assert!(
            report
                .features
                .iter()
                .any(|entry| entry.recommendation == FeatureRecommendation::Remove)
        );
    }

    #[test]
    fn release_phase_mapping_covers_all_recommendations() {
        let report = build_feature_inventory_report();
        let mapped = report
            .scoring_model
            .release_phase_mapping
            .iter()
            .map(|mapping| mapping.recommendation)
            .collect::<HashSet<_>>();

        for recommendation in [
            FeatureRecommendation::Keep,
            FeatureRecommendation::Merge,
            FeatureRecommendation::Remove,
        ] {
            assert!(
                mapped.contains(&recommendation),
                "missing release phase mapping for recommendation: {}",
                recommendation.as_str()
            );
        }
    }

    #[test]
    fn deprecation_pipeline_covers_all_stages() {
        let report = build_feature_inventory_report();
        let stages = report
            .scoring_model
            .deprecation_pipeline
            .iter()
            .map(|policy| policy.stage)
            .collect::<HashSet<_>>();
        for stage in [
            DeprecationStage::Warning,
            DeprecationStage::MigrationGuidance,
            DeprecationStage::Removal,
        ] {
            assert!(
                stages.contains(&stage),
                "missing deprecation stage policy: {}",
                stage.as_str()
            );
        }
    }

    #[test]
    fn deprecation_stage_transitions_follow_version_windows() {
        let warning_notice = command_deprecation_notice_for_version("sync-backup", "0.14.2")
            .expect("sync-backup should have warning-stage notice");
        assert_eq!(warning_notice.stage, DeprecationStage::Warning);

        let migration_notice = command_deprecation_notice_for_version("sync-backup", "0.15.0")
            .expect("sync-backup should have migration-stage notice");
        assert_eq!(migration_notice.stage, DeprecationStage::MigrationGuidance);

        let removal_notice = command_deprecation_notice_for_version("sync-backup", "0.16.0")
            .expect("sync-backup should have removal-stage notice");
        assert_eq!(removal_notice.stage, DeprecationStage::Removal);
    }

    #[test]
    fn usage_deprecation_warnings_require_prior_command_usage() {
        let no_warnings = active_usage_deprecation_warnings_for_version("0.14.2", |_command_id| 0);
        assert!(no_warnings.is_empty());

        let with_warnings = active_usage_deprecation_warnings_for_version("0.14.2", |command_id| {
            if command_id == "sync-backup" { 2 } else { 0 }
        });
        assert_eq!(with_warnings.len(), 1);
        assert!(with_warnings[0].contains("--sync-backup"));
    }

    #[test]
    fn report_includes_deprecation_metadata_for_low_value_features() {
        let report = build_feature_inventory_report_for_version("0.14.2");
        let encrypted_sync = report
            .features
            .iter()
            .find(|entry| entry.feature_id == "encrypted-sync-bundles")
            .expect("encrypted-sync-bundles should exist in report");
        let deprecation = encrypted_sync
            .deprecation
            .as_ref()
            .expect("encrypted-sync-bundles should include deprecation metadata");
        assert_eq!(deprecation.warning_from_version, "0.14.2");
        assert_eq!(deprecation.migration_from_version, "0.15.0");
        assert_eq!(deprecation.removal_from_version, "0.16.0");
        assert_eq!(deprecation.active_stage, Some(DeprecationStage::Warning));
        assert!(!deprecation.release_notes_hook.is_empty());
    }

    #[test]
    fn rubric_sample_candidates_are_deterministic() {
        let samples = [
            (
                "core-workflow-boundary-keep",
                4,
                3.50,
                TieBreakSignals {
                    safety: 4,
                    migration_risk: 3,
                    user_disruption: 2,
                },
                FeatureRecommendation::Keep,
            ),
            (
                "incremental-merge",
                3,
                3.20,
                TieBreakSignals {
                    safety: 3,
                    migration_risk: 3,
                    user_disruption: 3,
                },
                FeatureRecommendation::Merge,
            ),
            (
                "low-signal-boundary-remove",
                2,
                3.50,
                TieBreakSignals {
                    safety: 2,
                    migration_risk: 2,
                    user_disruption: 2,
                },
                FeatureRecommendation::Remove,
            ),
        ];

        for (sample_id, value, maintenance_cost, signals, expected) in samples {
            let recommendation = classify_recommendation(value, maintenance_cost, signals);
            assert_eq!(
                recommendation, expected,
                "unexpected rubric outcome for sample candidate `{sample_id}`"
            );
        }
    }

    #[test]
    fn keep_boundary_defaults_to_merge_without_tie_break_signal() {
        let recommendation = classify_recommendation(
            4,
            3.50,
            TieBreakSignals {
                safety: 3,
                migration_risk: 3,
                user_disruption: 3,
            },
        );

        assert_eq!(recommendation, FeatureRecommendation::Merge);
    }

    #[test]
    fn remove_boundary_defaults_to_merge_when_signal_exceeds_threshold() {
        let recommendation = classify_recommendation(
            2,
            3.50,
            TieBreakSignals {
                safety: 2,
                migration_risk: 4,
                user_disruption: 2,
            },
        );

        assert_eq!(recommendation, FeatureRecommendation::Merge);
    }

    #[test]
    fn committed_markdown_report_matches_generator() {
        let report = build_feature_inventory_report();
        let generated = render_markdown_report(&report);
        let committed = include_str!("../FEATURE_INVENTORY.md");

        assert_eq!(
            normalize_line_endings(&generated),
            normalize_line_endings(committed)
        );
    }

    #[test]
    fn committed_json_report_matches_generator() {
        let report = build_feature_inventory_report();
        let generated = serde_json::to_string_pretty(&report)
            .expect("feature inventory JSON serialization failed");
        let committed = include_str!("../FEATURE_INVENTORY.json");

        assert_eq!(
            normalize_line_endings(&generated),
            normalize_line_endings(committed)
        );
    }

    fn collect_usage_option_flags() -> HashSet<String> {
        crate::cli::usage_text()
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                if !trimmed.starts_with("--") {
                    return None;
                }

                let raw_flag = trimmed.split_whitespace().next()?;
                let normalized = raw_flag.split('=').next().unwrap_or(raw_flag);
                Some(normalized.to_string())
            })
            .collect::<HashSet<_>>()
    }

    fn normalize_line_endings(value: &str) -> String {
        value.replace("\r\n", "\n")
    }
}
