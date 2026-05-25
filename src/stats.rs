use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::config::{ProfileId, RecurringScheduleConfig, StatsRetentionConfig};
use crate::task_labels::{canonical_task_label, normalize_task_label, task_label_index};

mod helpers;
#[cfg(test)]
use helpers::create_unique_temp_path;
use helpers::{
    average_two_percentages, backfilled_time_of_day_bucket, best_goal_streak,
    consistency_score_from_active_days, current_goal_streak, daily_has_activity, days_in_month,
    format_week_label, month_key_for_day, normalize_session_metadata_text,
    normalize_task_goal_targets, normalize_task_planner_state, parse_week_label,
    percentage_round_nearest, planner_state_labels_for_keys, profile_bucket_for, week_key_for_day,
    weekly_completion_score_pct, write_atomic_bytes,
};
mod analytics;
mod export;
mod persistence;
mod planner;
mod recording;
mod trends;

#[cfg_attr(test, allow(dead_code))]
const STATS_FILE_NAME: &str = "stats.toml";
const JSON_EXPORT_FILE_NAME: &str = "focustime-stats.json";
const CSV_EXPORT_FILE_NAME: &str = "focustime-stats.csv";
const EXPORT_SCHEMA_VERSION: u32 = 7;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatsLoadOptions {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatsSaveOptions {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionStats {
    pub pomodoros_completed: u32,
    pub focused_seconds: u64,
}

impl SessionStats {
    pub fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DailyGoalSnapshot {
    #[serde(default)]
    pub minutes: u64,
    #[serde(default)]
    pub pomodoros: u32,
}

impl DailyGoalSnapshot {
    pub fn has_any_target(self) -> bool {
        self.minutes > 0 || self.pomodoros > 0
    }

    pub fn is_met_by_totals(self, focused_minutes: u64, pomodoros_completed: u32) -> bool {
        self.has_any_target()
            && (self.minutes == 0 || focused_minutes >= self.minutes)
            && (self.pomodoros == 0 || pomodoros_completed >= self.pomodoros)
    }

    pub fn is_met_by(self, stats: DailyStats) -> bool {
        self.is_met_by_totals(stats.focused_minutes(), stats.pomodoros_completed)
    }
}

pub fn carry_over_goal_target(
    base: DailyGoalSnapshot,
    carry_enabled: bool,
    previous: Option<(DailyGoalSnapshot, u64, u32)>,
) -> DailyGoalSnapshot {
    if !carry_enabled {
        return base;
    }
    let Some((previous_target, previous_minutes, previous_pomodoros)) = previous else {
        return base;
    };
    DailyGoalSnapshot {
        minutes: if base.minutes == 0 {
            0
        } else {
            base.minutes
                .saturating_add(previous_target.minutes.saturating_sub(previous_minutes))
        },
        pomodoros: if base.pomodoros == 0 {
            0
        } else {
            base.pomodoros
                .saturating_add(previous_target.pomodoros.saturating_sub(previous_pomodoros))
        },
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GoalStreak {
    pub current: u32,
    pub best: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WeeklyStats {
    pub year: i32,
    pub week: u32,
    pub pomodoros_completed: u32,
    pub focused_seconds: u64,
}

impl WeeklyStats {
    pub fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklyConsistency {
    pub year: i32,
    pub week: u32,
    pub week_label: String,
    pub active_days: u8,
    pub consistency_score_pct: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklyFocusScore {
    pub year: i32,
    pub week: u32,
    pub week_label: String,
    pub active_days: u8,
    pub consistency_score_pct: u8,
    pub completion_score_pct: Option<u8>,
    pub focus_score_pct: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusRiskLevel {
    Low,
    Medium,
    High,
}

impl FocusRiskLevel {
    pub fn from_score(score_pct: u8) -> Self {
        if score_pct >= 70 {
            Self::High
        } else if score_pct >= 40 {
            Self::Medium
        } else {
            Self::Low
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn triggers_alert(self) -> bool {
        matches!(self, Self::Medium | Self::High)
    }

    fn rank(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalPeriod {
    Daily,
    Weekly,
    Monthly,
}

impl GoalPeriod {
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Daily => "D",
            Self::Weekly => "W",
            Self::Monthly => "M",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusRiskSignal {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoalRiskForecast {
    pub period: GoalPeriod,
    pub configured: bool,
    pub met: bool,
    pub completion_pct: Option<u8>,
    pub risk_score_pct: u8,
    pub risk_level: FocusRiskLevel,
    pub signals: Vec<FocusRiskSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StreakRiskForecast {
    pub configured: bool,
    pub current_streak: u32,
    pub best_streak: u32,
    pub today_goal_met: bool,
    pub recent_goal_reliability_pct: u8,
    pub risk_score_pct: u8,
    pub risk_level: FocusRiskLevel,
    pub signals: Vec<FocusRiskSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusRiskForecast {
    pub daily_goal: GoalRiskForecast,
    pub weekly_goal: GoalRiskForecast,
    pub monthly_goal: GoalRiskForecast,
    pub streak: StreakRiskForecast,
}

impl FocusRiskForecast {
    pub fn highest_risk_level(&self) -> FocusRiskLevel {
        let mut level = self.daily_goal.risk_level;
        for candidate in [
            self.weekly_goal.risk_level,
            self.monthly_goal.risk_level,
            self.streak.risk_level,
        ] {
            if candidate.rank() > level.rank() {
                level = candidate;
            }
        }
        level
    }

    pub fn alert_active(&self) -> bool {
        let highest_level = self.highest_risk_level();
        if highest_level.triggers_alert() && matches!(highest_level, FocusRiskLevel::High) {
            return true;
        }

        let medium_count = [
            self.daily_goal.risk_level,
            self.weekly_goal.risk_level,
            self.monthly_goal.risk_level,
            self.streak.risk_level,
        ]
        .into_iter()
        .filter(|level| matches!(level, FocusRiskLevel::Medium))
        .count();
        if medium_count >= 2 {
            return true;
        }

        [
            (self.daily_goal.risk_level, self.daily_goal.risk_score_pct),
            (self.weekly_goal.risk_level, self.weekly_goal.risk_score_pct),
            (
                self.monthly_goal.risk_level,
                self.monthly_goal.risk_score_pct,
            ),
            (self.streak.risk_level, self.streak.risk_score_pct),
        ]
        .into_iter()
        .any(|(level, score)| matches!(level, FocusRiskLevel::Medium) && score >= 55)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusRiskCalibrationMetrics {
    pub sample_count: u32,
    pub alert_count: u32,
    pub true_positive_alerts: u32,
    pub false_positive_alerts: u32,
    pub precision_pct: u8,
    pub missed_warning_count: u32,
    pub missed_warning_rate_pct: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MonthlyStats {
    pub year: i32,
    pub month: u32,
    pub pomodoros_completed: u32,
    pub focused_seconds: u64,
}

impl MonthlyStats {
    pub fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeatmapDayStats {
    pub day: u32,
    pub pomodoros_completed: u32,
    pub focused_seconds: u64,
}

impl HeatmapDayStats {
    pub fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthlyHeatmap {
    pub year: i32,
    pub month: u32,
    pub first_weekday_monday0: u32,
    pub days_in_month: u32,
    pub max_focused_minutes: u64,
    pub days: Vec<HeatmapDayStats>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DailyStats {
    #[serde(default)]
    pub pomodoros_completed: u32,
    #[serde(default)]
    pub focused_seconds: u64,
    #[serde(default)]
    pub goal: Option<DailyGoalSnapshot>,
}

impl DailyStats {
    pub fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedStatsFiles {
    pub json_path: PathBuf,
    pub csv_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryKpiExportContext {
    pub reference_day: NaiveDate,
    pub daily_goal: DailyGoalSnapshot,
    pub weekly_goal: DailyGoalSnapshot,
    pub monthly_goal: DailyGoalSnapshot,
    pub carry_over_daily: bool,
    pub carry_over_weekly: bool,
    pub carry_over_monthly: bool,
    pub recurring_schedule: RecurringScheduleConfig,
    pub stats_retention: StatsRetentionConfig,
    pub comparison_dimension: ComparisonDimension,
    pub comparison_task_filter: Option<String>,
    pub comparison_profile_filter: Option<ProfileBucket>,
    pub comparison_time_of_day_filter: Option<TimeOfDayBucket>,
}

impl Default for HistoryKpiExportContext {
    fn default() -> Self {
        Self {
            reference_day: chrono::Local::now().date_naive(),
            daily_goal: DailyGoalSnapshot::default(),
            weekly_goal: DailyGoalSnapshot::default(),
            monthly_goal: DailyGoalSnapshot::default(),
            carry_over_daily: false,
            carry_over_weekly: false,
            carry_over_monthly: false,
            recurring_schedule: RecurringScheduleConfig::default(),
            stats_retention: StatsRetentionConfig::default(),
            comparison_dimension: ComparisonDimension::TaskLabel,
            comparison_task_filter: None,
            comparison_profile_filter: None,
            comparison_time_of_day_filter: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsGrowthSection {
    pub name: String,
    pub record_count: usize,
    pub estimated_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsGrowthSummary {
    pub total_record_count: usize,
    pub estimated_bytes: u64,
    pub sections: Vec<StatsGrowthSection>,
    pub high_volume_sections: Vec<StatsGrowthSection>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct StatsRetentionPruneResult {
    pub daily_removed: usize,
    pub focus_sessions_removed: usize,
    pub session_interruptions_removed: usize,
    pub break_glass_overrides_removed: usize,
    pub weekly_goal_snapshots_removed: usize,
    pub monthly_goal_snapshots_removed: usize,
}

impl StatsRetentionPruneResult {
    pub fn total_removed(self) -> usize {
        self.daily_removed
            .saturating_add(self.focus_sessions_removed)
            .saturating_add(self.session_interruptions_removed)
            .saturating_add(self.break_glass_overrides_removed)
            .saturating_add(self.weekly_goal_snapshots_removed)
            .saturating_add(self.monthly_goal_snapshots_removed)
    }

    pub fn any_removed(self) -> bool {
        self.total_removed() > 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusSessionRecord {
    pub date: String,
    pub task_label: String,
    #[serde(default)]
    pub focus_intention: String,
    #[serde(default)]
    pub task_note: String,
    pub focused_seconds: u64,
    #[serde(default)]
    pub profile: Option<ProfileId>,
    #[serde(default)]
    pub completion_timestamp_epoch_secs: Option<u64>,
    #[serde(default)]
    pub completion_time_of_day_bucket: Option<TimeOfDayBucket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusSessionMetadata<'a> {
    pub task_label: Option<&'a str>,
    pub focus_intention: Option<&'a str>,
    pub task_note: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionInterruptionReason {
    ManualStop,
    ManualSkip,
}

impl SessionInterruptionReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::ManualStop => "stop/reset",
            Self::ManualSkip => "skip/next",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionInterruptionEvent {
    pub timestamp_epoch_secs: u64,
    pub date: String,
    pub reason: SessionInterruptionReason,
    #[serde(default)]
    pub task_label: Option<String>,
    #[serde(default)]
    pub focus_intention: Option<String>,
    #[serde(default)]
    pub task_note: Option<String>,
    #[serde(default)]
    pub remaining_secs: u64,
    #[serde(default)]
    pub profile: Option<ProfileId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileBucket {
    Classic,
    DeepWork,
    Custom,
    Unknown,
}

impl ProfileBucket {
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::DeepWork => "Deep Work",
            Self::Custom => "Custom",
            Self::Unknown => "Unknown",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::DeepWork => "deep_work",
            Self::Custom => "custom",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDayBucket {
    Morning,
    Afternoon,
    Evening,
    Night,
    Unknown,
}

impl TimeOfDayBucket {
    pub fn label(self) -> &'static str {
        match self {
            Self::Morning => "Morning",
            Self::Afternoon => "Afternoon",
            Self::Evening => "Evening",
            Self::Night => "Night",
            Self::Unknown => "Unknown",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Morning => "morning",
            Self::Afternoon => "afternoon",
            Self::Evening => "evening",
            Self::Night => "night",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_hour(hour: u32) -> Self {
        match hour {
            5..=11 => Self::Morning,
            12..=16 => Self::Afternoon,
            17..=21 => Self::Evening,
            0..=4 | 22..=23 => Self::Night,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonDimension {
    TaskLabel,
    Profile,
    TimeOfDay,
}

impl ComparisonDimension {
    pub fn label(self) -> &'static str {
        match self {
            Self::TaskLabel => "Task",
            Self::Profile => "Profile",
            Self::TimeOfDay => "Time of Day",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::TaskLabel => "task_label",
            Self::Profile => "profile",
            Self::TimeOfDay => "time_of_day",
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProductivityComparisonFilter {
    pub task_label: Option<String>,
    pub profile: Option<ProfileBucket>,
    pub time_of_day: Option<TimeOfDayBucket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductivityComparisonRow {
    pub dimension: ComparisonDimension,
    pub label: String,
    pub task_label: Option<String>,
    pub profile: Option<ProfileBucket>,
    pub time_of_day: Option<TimeOfDayBucket>,
    pub sessions_completed: u32,
    pub focused_seconds: u64,
    pub focus_share_pct: u8,
}

impl ProductivityComparisonRow {
    pub fn focused_minutes(&self) -> u64 {
        self.focused_seconds / 60
    }

    pub fn average_focused_minutes_per_session(&self) -> u64 {
        if self.sessions_completed == 0 {
            return 0;
        }
        (self.focused_seconds / u64::from(self.sessions_completed)) / 60
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileTotals {
    pub profile: ProfileBucket,
    pub pomodoros_completed: u32,
    pub focused_seconds: u64,
}

impl ProfileTotals {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileEffectiveness {
    pub profile: ProfileBucket,
    pub sessions_completed: u32,
    pub focused_seconds: u64,
    pub active_days: u32,
    pub focus_share_pct: u8,
}

impl ProfileEffectiveness {
    pub fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }

    pub fn average_focused_minutes_per_session(self) -> u64 {
        if self.sessions_completed == 0 {
            return 0;
        }
        (self.focused_seconds / u64::from(self.sessions_completed)) / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTotals {
    pub task_label: String,
    pub pomodoros_completed: u32,
    pub focused_seconds: u64,
}

impl TaskTotals {
    pub fn focused_minutes(&self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTrend {
    pub task_label: String,
    pub recent_pomodoros_completed: u32,
    pub recent_focused_seconds: u64,
    pub previous_pomodoros_completed: u32,
    pub previous_focused_seconds: u64,
}

impl TaskTrend {
    pub fn recent_focused_minutes(&self) -> u64 {
        self.recent_focused_seconds / 60
    }

    pub fn previous_focused_minutes(&self) -> u64 {
        self.previous_focused_seconds / 60
    }

    pub fn delta_focused_seconds(&self) -> i64 {
        let recent = i128::from(self.recent_focused_seconds);
        let previous = i128::from(self.previous_focused_seconds);
        (recent - previous).clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }

    pub fn delta_focused_minutes(&self) -> i64 {
        self.delta_focused_seconds() / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGoalProgress {
    pub task_label: String,
    pub target: DailyGoalSnapshot,
    pub pomodoros_completed: u32,
    pub focused_seconds: u64,
    pub met: bool,
}

impl TaskGoalProgress {
    pub fn focused_minutes(&self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TaskTrendWindow {
    recent_start: chrono::NaiveDate,
    recent_end: chrono::NaiveDate,
    previous_start: chrono::NaiveDate,
    previous_end: chrono::NaiveDate,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct TaskTrendAccumulator {
    recent_pomodoros_completed: u32,
    recent_focused_seconds: u64,
    previous_pomodoros_completed: u32,
    previous_focused_seconds: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ProfileEffectivenessAccumulator {
    sessions_completed: u32,
    focused_seconds: u64,
    active_days: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BreakGlassOverrideEvent {
    pub timestamp_epoch_secs: u64,
    pub date: String,
    pub task_label: Option<String>,
    pub duration_seconds: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct StatsExport {
    schema_version: u32,
    daily: Vec<DailyExportRow>,
    weekly: Vec<WeeklyExportRow>,
    sessions: Vec<SessionExportRow>,
    interruptions: Vec<SessionInterruptionExportRow>,
    overrides: Vec<BreakGlassOverrideExportRow>,
    task_totals: Vec<TaskTotalsExportRow>,
    task_trends: Vec<TaskTrendExportRow>,
    weekly_consistency: Vec<WeeklyConsistencyExportRow>,
    focus_scores: Vec<FocusScoreExportRow>,
    profile_effectiveness: Vec<ProfileEffectivenessExportRow>,
    productivity_comparisons: Vec<ProductivityComparisonExportRow>,
    history_kpis: HistoryKpiExport,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct DailyExportRow {
    date: String,
    pomodoros_completed: u32,
    focused_seconds: u64,
    focused_minutes: u64,
    goal: Option<DailyGoalSnapshot>,
    goal_met: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct WeeklyExportRow {
    year: i32,
    week: u32,
    week_label: String,
    pomodoros_completed: u32,
    focused_seconds: u64,
    focused_minutes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct SessionExportRow {
    date: String,
    task_label: String,
    focus_intention: String,
    task_note: String,
    focused_seconds: u64,
    focused_minutes: u64,
    profile: Option<ProfileId>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct BreakGlassOverrideExportRow {
    timestamp_epoch_secs: u64,
    date: String,
    task_label: Option<String>,
    duration_seconds: u64,
    duration_minutes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct SessionInterruptionExportRow {
    timestamp_epoch_secs: u64,
    date: String,
    reason: SessionInterruptionReason,
    task_label: Option<String>,
    focus_intention: Option<String>,
    task_note: Option<String>,
    remaining_secs: u64,
    profile: Option<ProfileId>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct TaskTotalsExportRow {
    task_label: String,
    pomodoros_completed: u32,
    focused_seconds: u64,
    focused_minutes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct TaskTrendExportRow {
    task_label: String,
    recent_window_start: String,
    recent_window_end: String,
    previous_window_start: String,
    previous_window_end: String,
    recent_pomodoros_completed: u32,
    recent_focused_seconds: u64,
    recent_focused_minutes: u64,
    previous_pomodoros_completed: u32,
    previous_focused_seconds: u64,
    previous_focused_minutes: u64,
    delta_focused_seconds: i64,
    delta_focused_minutes: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct WeeklyConsistencyExportRow {
    year: i32,
    week: u32,
    week_label: String,
    active_days: u8,
    consistency_score_pct: u8,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct FocusScoreExportRow {
    year: i32,
    week: u32,
    week_label: String,
    active_days: u8,
    consistency_score_pct: u8,
    completion_score_pct: Option<u8>,
    focus_score_pct: Option<u8>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProfileEffectivenessExportRow {
    profile: String,
    sessions_completed: u32,
    active_days: u32,
    focused_seconds: u64,
    focused_minutes: u64,
    average_focused_minutes_per_session: u64,
    focus_share_pct: u8,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProductivityComparisonExportRow {
    dimension: ComparisonDimension,
    label: String,
    task_label: Option<String>,
    profile: Option<String>,
    time_of_day: Option<TimeOfDayBucket>,
    sessions_completed: u32,
    focused_seconds: u64,
    focused_minutes: u64,
    average_focused_minutes_per_session: u64,
    focus_share_pct: u8,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiExport {
    session_summary: HistoryKpiSessionSummary,
    focus_score: HistoryKpiFocusScore,
    goal_streak: HistoryKpiGoalStreak,
    focus_risk: HistoryKpiFocusRisk,
    weekly_allocation: HistoryKpiWeeklyAllocation,
    last_interruption: HistoryKpiLastInterruption,
    stats_growth: HistoryKpiStatsGrowth,
    retention: HistoryKpiRetention,
    comparison_filters: HistoryKpiComparisonFilters,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiSessionSummary {
    session_pomodoros_completed: u32,
    session_focused_minutes: u64,
    today_pomodoros_completed: u32,
    today_focused_minutes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiFocusScore {
    week_label: Option<String>,
    active_days: Option<u8>,
    consistency_score_pct: Option<u8>,
    completion_score_pct: Option<u8>,
    focus_score_pct: Option<u8>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiGoalPeriodProgress {
    focused_minutes_completed: u64,
    focused_minutes_target: u64,
    pomodoros_completed: u32,
    pomodoros_target: u32,
    target_configured: bool,
    met: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiGoalStreak {
    daily: HistoryKpiGoalPeriodProgress,
    weekly: HistoryKpiGoalPeriodProgress,
    monthly: HistoryKpiGoalPeriodProgress,
    current_days: u32,
    best_days: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiFocusRisk {
    alert_active: bool,
    highest_risk_level: FocusRiskLevel,
    highest_signal_scope: Option<String>,
    highest_signal_label: Option<String>,
    highest_signal_value: Option<String>,
    daily_risk_level: FocusRiskLevel,
    daily_risk_score_pct: u8,
    weekly_risk_level: FocusRiskLevel,
    weekly_risk_score_pct: u8,
    monthly_risk_level: FocusRiskLevel,
    monthly_risk_score_pct: u8,
    streak_risk_level: FocusRiskLevel,
    streak_risk_score_pct: u8,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiWeeklyAllocation {
    week_target_minutes: u64,
    week_target_pomodoros: u32,
    completed_minutes: u64,
    completed_pomodoros: u32,
    remaining_minutes: u64,
    remaining_pomodoros: u32,
    remaining_days_in_week: usize,
    allocatable_days: usize,
    uses_schedule_weights: bool,
    today_target_minutes: u64,
    today_target_pomodoros: u32,
    daily_targets: Vec<HistoryKpiWeeklyAllocationDay>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiWeeklyAllocationDay {
    day: String,
    minutes_target: u64,
    pomodoros_target: u32,
    allocatable: bool,
    weight_minutes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiLastInterruption {
    timestamp_epoch_secs: Option<u64>,
    reason: Option<SessionInterruptionReason>,
    task_label: Option<String>,
    focus_intention: Option<String>,
    task_note: Option<String>,
    remaining_secs: Option<u64>,
    profile_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiStatsGrowth {
    total_record_count: usize,
    estimated_bytes: u64,
    sections: Vec<StatsGrowthSection>,
    high_volume_sections: Vec<StatsGrowthSection>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiRetention {
    preset_id: String,
    preview: StatsRetentionPruneResult,
    pending_prune: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HistoryKpiComparisonFilters {
    dimension: ComparisonDimension,
    task_filter: Option<String>,
    profile_filter: Option<ProfileBucket>,
    time_of_day_filter: Option<TimeOfDayBucket>,
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
struct CsvExportRow {
    schema_version: u32,
    record_type: &'static str,
    date: Option<String>,
    week_label: Option<String>,
    year: Option<i32>,
    week: Option<u32>,
    pomodoros_completed: u32,
    focused_seconds: u64,
    focused_minutes: u64,
    goal_minutes: Option<u64>,
    goal_pomodoros: Option<u32>,
    goal_met: Option<bool>,
    task_label: Option<String>,
    break_glass_timestamp_epoch_secs: Option<u64>,
    break_glass_duration_seconds: Option<u64>,
    interruption_timestamp_epoch_secs: Option<u64>,
    interruption_reason: Option<SessionInterruptionReason>,
    interruption_remaining_secs: Option<u64>,
    focus_intention: Option<String>,
    task_note: Option<String>,
    recent_window_start: Option<String>,
    recent_window_end: Option<String>,
    previous_window_start: Option<String>,
    previous_window_end: Option<String>,
    previous_pomodoros_completed: Option<u32>,
    previous_focused_seconds: Option<u64>,
    previous_focused_minutes: Option<u64>,
    delta_focused_seconds: Option<i64>,
    delta_focused_minutes: Option<i64>,
    profile_name: Option<String>,
    sessions_completed: Option<u32>,
    active_days: Option<u32>,
    consistency_score_pct: Option<u8>,
    completion_score_pct: Option<u8>,
    focus_score_pct: Option<u8>,
    average_focused_minutes_per_session: Option<u64>,
    focus_share_pct: Option<u8>,
    comparison_dimension: Option<String>,
    comparison_label: Option<String>,
    time_of_day_bucket: Option<String>,
    kpi_card_id: Option<String>,
    kpi_payload_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct PersistedStats {
    #[serde(default)]
    daily: BTreeMap<String, DailyStats>,
    #[serde(default)]
    weekly_goal_snapshots: BTreeMap<String, DailyGoalSnapshot>,
    #[serde(default)]
    monthly_goal_snapshots: BTreeMap<String, DailyGoalSnapshot>,
    #[serde(default)]
    task_labels: Vec<String>,
    #[serde(default)]
    selected_task_label: Option<String>,
    #[serde(default)]
    task_label_favorites: Vec<String>,
    #[serde(default)]
    task_label_archived: Vec<String>,
    #[serde(default)]
    focus_sessions: Vec<FocusSessionRecord>,
    #[serde(default)]
    session_interruptions: Vec<SessionInterruptionEvent>,
    #[serde(default)]
    break_glass_overrides: Vec<BreakGlassOverrideEvent>,
    #[serde(default)]
    task_goal_targets: BTreeMap<String, DailyGoalSnapshot>,
}

#[derive(Debug, Clone, Default)]
pub struct FocusStats {
    session: SessionStats,
    daily: BTreeMap<String, DailyStats>,
    weekly_goal_snapshots: BTreeMap<String, DailyGoalSnapshot>,
    monthly_goal_snapshots: BTreeMap<String, DailyGoalSnapshot>,
    task_labels: Vec<String>,
    selected_task_label: Option<String>,
    task_label_favorites: BTreeSet<String>,
    task_label_archived: BTreeSet<String>,
    focus_sessions: Vec<FocusSessionRecord>,
    session_interruptions: Vec<SessionInterruptionEvent>,
    break_glass_overrides: Vec<BreakGlassOverrideEvent>,
    task_goal_targets: BTreeMap<String, DailyGoalSnapshot>,
}

pub fn current_day_key() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests;
