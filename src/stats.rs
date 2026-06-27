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
    format_week_label, month_key_for_day, normalize_task_planner_state, normalize_usage_counts,
    parse_week_label, percentage_round_nearest, planner_state_labels_for_keys, profile_bucket_for,
    week_key_for_day, weekly_completion_score_pct, write_atomic_bytes,
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
const EXPORT_SCHEMA_VERSION: u32 = 9;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StatsLoadOptions {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StatsSaveOptions {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SessionStats {
    pub(crate) pomodoros_completed: u32,
    pub(crate) focused_seconds: u64,
}

impl SessionStats {
    pub(crate) fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct DailyGoalSnapshot {
    #[serde(default)]
    pub(crate) minutes: u64,
    #[serde(default)]
    pub(crate) pomodoros: u32,
}

impl DailyGoalSnapshot {
    pub(crate) fn has_any_target(self) -> bool {
        self.minutes > 0 || self.pomodoros > 0
    }

    pub(crate) fn is_met_by_totals(self, focused_minutes: u64, pomodoros_completed: u32) -> bool {
        self.has_any_target()
            && (self.minutes == 0 || focused_minutes >= self.minutes)
            && (self.pomodoros == 0 || pomodoros_completed >= self.pomodoros)
    }

    pub(crate) fn is_met_by(self, stats: DailyStats) -> bool {
        self.is_met_by_totals(stats.focused_minutes(), stats.pomodoros_completed)
    }
}

pub(crate) fn carry_over_goal_target(
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
pub(crate) struct GoalStreak {
    pub(crate) current: u32,
    pub(crate) best: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WeeklyStats {
    pub(crate) year: i32,
    pub(crate) week: u32,
    pub(crate) pomodoros_completed: u32,
    pub(crate) focused_seconds: u64,
}

impl WeeklyStats {
    pub(crate) fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeeklyConsistency {
    pub(crate) year: i32,
    pub(crate) week: u32,
    pub(crate) week_label: String,
    pub(crate) active_days: u8,
    pub(crate) consistency_score_pct: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeeklyFocusScore {
    pub(crate) year: i32,
    pub(crate) week: u32,
    pub(crate) week_label: String,
    pub(crate) active_days: u8,
    pub(crate) consistency_score_pct: u8,
    pub(crate) completion_score_pct: Option<u8>,
    pub(crate) focus_score_pct: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FocusRiskLevel {
    Low,
    Medium,
    High,
}

impl FocusRiskLevel {
    pub(crate) fn from_score(score_pct: u8) -> Self {
        if score_pct >= 70 {
            Self::High
        } else if score_pct >= 40 {
            Self::Medium
        } else {
            Self::Low
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub(crate) fn triggers_alert(self) -> bool {
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
pub(crate) enum GoalPeriod {
    Daily,
    Weekly,
    Monthly,
}

impl GoalPeriod {
    pub(crate) fn short_label(self) -> &'static str {
        match self {
            Self::Daily => "D",
            Self::Weekly => "W",
            Self::Monthly => "M",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FocusRiskSignal {
    pub(crate) label: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct GoalRiskForecast {
    pub(crate) period: GoalPeriod,
    pub(crate) configured: bool,
    pub(crate) met: bool,
    pub(crate) completion_pct: Option<u8>,
    pub(crate) risk_score_pct: u8,
    pub(crate) risk_level: FocusRiskLevel,
    pub(crate) signals: Vec<FocusRiskSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StreakRiskForecast {
    pub(crate) configured: bool,
    pub(crate) current_streak: u32,
    pub(crate) best_streak: u32,
    pub(crate) today_goal_met: bool,
    pub(crate) recent_goal_reliability_pct: u8,
    pub(crate) risk_score_pct: u8,
    pub(crate) risk_level: FocusRiskLevel,
    pub(crate) signals: Vec<FocusRiskSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FocusRiskForecast {
    pub(crate) daily_goal: GoalRiskForecast,
    pub(crate) weekly_goal: GoalRiskForecast,
    pub(crate) monthly_goal: GoalRiskForecast,
    pub(crate) streak: StreakRiskForecast,
}

impl FocusRiskForecast {
    pub(crate) fn highest_risk_level(&self) -> FocusRiskLevel {
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

    pub(crate) fn alert_active(&self) -> bool {
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
pub(crate) struct FocusRiskCalibrationMetrics {
    pub(crate) sample_count: u32,
    pub(crate) alert_count: u32,
    pub(crate) true_positive_alerts: u32,
    pub(crate) false_positive_alerts: u32,
    pub(crate) precision_pct: u8,
    pub(crate) missed_warning_count: u32,
    pub(crate) missed_warning_rate_pct: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MonthlyStats {
    pub(crate) year: i32,
    pub(crate) month: u32,
    pub(crate) pomodoros_completed: u32,
    pub(crate) focused_seconds: u64,
}

impl MonthlyStats {
    pub(crate) fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct HeatmapDayStats {
    pub(crate) day: u32,
    pub(crate) pomodoros_completed: u32,
    pub(crate) focused_seconds: u64,
}

impl HeatmapDayStats {
    pub(crate) fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonthlyHeatmap {
    pub(crate) year: i32,
    pub(crate) month: u32,
    pub(crate) first_weekday_monday0: u32,
    pub(crate) days_in_month: u32,
    pub(crate) max_focused_minutes: u64,
    pub(crate) days: Vec<HeatmapDayStats>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct DailyStats {
    #[serde(default)]
    pub(crate) pomodoros_completed: u32,
    #[serde(default)]
    pub(crate) focused_seconds: u64,
    #[serde(default)]
    pub(crate) goal: Option<DailyGoalSnapshot>,
}

impl DailyStats {
    pub(crate) fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportedStatsFiles {
    pub(crate) json_path: PathBuf,
    pub(crate) csv_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryKpiExportContext {
    pub(crate) reference_day: NaiveDate,
    pub(crate) daily_goal: DailyGoalSnapshot,
    pub(crate) weekly_goal: DailyGoalSnapshot,
    pub(crate) monthly_goal: DailyGoalSnapshot,
    pub(crate) carry_over_daily: bool,
    pub(crate) carry_over_weekly: bool,
    pub(crate) carry_over_monthly: bool,
    pub(crate) recurring_schedule: RecurringScheduleConfig,
    pub(crate) stats_retention: StatsRetentionConfig,
    pub(crate) comparison_dimension: ComparisonDimension,
    pub(crate) comparison_task_filter: Option<String>,
    pub(crate) comparison_profile_filter: Option<ProfileBucket>,
    pub(crate) comparison_time_of_day_filter: Option<TimeOfDayBucket>,
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
pub(crate) struct StatsGrowthSection {
    pub(crate) name: String,
    pub(crate) record_count: usize,
    pub(crate) estimated_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StatsGrowthSummary {
    pub(crate) total_record_count: usize,
    pub(crate) estimated_bytes: u64,
    pub(crate) sections: Vec<StatsGrowthSection>,
    pub(crate) high_volume_sections: Vec<StatsGrowthSection>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub(crate) struct StatsRetentionPruneResult {
    pub(crate) daily_removed: usize,
    pub(crate) focus_sessions_removed: usize,
    pub(crate) session_interruptions_removed: usize,
    pub(crate) weekly_goal_snapshots_removed: usize,
    pub(crate) monthly_goal_snapshots_removed: usize,
}

impl StatsRetentionPruneResult {
    pub(crate) fn total_removed(self) -> usize {
        self.daily_removed
            .saturating_add(self.focus_sessions_removed)
            .saturating_add(self.session_interruptions_removed)
            .saturating_add(self.weekly_goal_snapshots_removed)
            .saturating_add(self.monthly_goal_snapshots_removed)
    }

    pub(crate) fn any_removed(self) -> bool {
        self.total_removed() > 0
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct UsageSignalEntry {
    pub(crate) surface: String,
    pub(crate) count: u64,
    pub(crate) share_pct: u8,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct UsageSignalSummary {
    pub(crate) total_events: u64,
    pub(crate) unique_surfaces: usize,
    pub(crate) top: Vec<UsageSignalEntry>,
    pub(crate) rare: Vec<UsageSignalEntry>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct UsageSignalsSummary {
    pub(crate) commands: UsageSignalSummary,
    pub(crate) screens: UsageSignalSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FocusSessionRecord {
    pub(crate) date: String,
    pub(crate) task_label: String,
    pub(crate) focused_seconds: u64,
    #[serde(default)]
    pub(crate) profile: Option<ProfileId>,
    #[serde(default)]
    pub(crate) completion_timestamp_epoch_secs: Option<u64>,
    #[serde(default)]
    pub(crate) completion_time_of_day_bucket: Option<TimeOfDayBucket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusSessionMetadata<'a> {
    pub(crate) task_label: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionInterruptionReason {
    ManualStop,
    ManualSkip,
}

impl SessionInterruptionReason {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ManualStop => "stop/reset",
            Self::ManualSkip => "skip/next",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SessionInterruptionEvent {
    pub(crate) timestamp_epoch_secs: u64,
    pub(crate) date: String,
    pub(crate) reason: SessionInterruptionReason,
    #[serde(default)]
    pub(crate) task_label: Option<String>,
    #[serde(default)]
    pub(crate) remaining_secs: u64,
    #[serde(default)]
    pub(crate) profile: Option<ProfileId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum ProfileBucket {
    #[serde(rename = "basic", alias = "classic")]
    Classic,
    #[serde(
        rename = "standard",
        alias = "deep_work",
        alias = "deep-work",
        alias = "deepwork"
    )]
    DeepWork,
    #[serde(rename = "advanced", alias = "custom")]
    Custom,
    #[serde(rename = "unknown")]
    Unknown,
}

impl ProfileBucket {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Classic => "Basic",
            Self::DeepWork => "Standard",
            Self::Custom => "Advanced",
            Self::Unknown => "Unknown",
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Classic => "basic",
            Self::DeepWork => "standard",
            Self::Custom => "advanced",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TimeOfDayBucket {
    Morning,
    Afternoon,
    Evening,
    Night,
    Unknown,
}

impl TimeOfDayBucket {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Morning => "Morning",
            Self::Afternoon => "Afternoon",
            Self::Evening => "Evening",
            Self::Night => "Night",
            Self::Unknown => "Unknown",
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Morning => "morning",
            Self::Afternoon => "afternoon",
            Self::Evening => "evening",
            Self::Night => "night",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn from_hour(hour: u32) -> Self {
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
pub(crate) enum ComparisonDimension {
    TaskLabel,
    Profile,
    TimeOfDay,
}

impl ComparisonDimension {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::TaskLabel => "Task",
            Self::Profile => "Profile",
            Self::TimeOfDay => "Time of Day",
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::TaskLabel => "task_label",
            Self::Profile => "profile",
            Self::TimeOfDay => "time_of_day",
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ProductivityComparisonFilter {
    pub(crate) task_label: Option<String>,
    pub(crate) profile: Option<ProfileBucket>,
    pub(crate) time_of_day: Option<TimeOfDayBucket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProductivityComparisonRow {
    pub(crate) dimension: ComparisonDimension,
    pub(crate) label: String,
    pub(crate) task_label: Option<String>,
    pub(crate) profile: Option<ProfileBucket>,
    pub(crate) time_of_day: Option<TimeOfDayBucket>,
    pub(crate) sessions_completed: u32,
    pub(crate) focused_seconds: u64,
    pub(crate) focus_share_pct: u8,
}

impl ProductivityComparisonRow {
    pub(crate) fn focused_minutes(&self) -> u64 {
        self.focused_seconds / 60
    }

    pub(crate) fn average_focused_minutes_per_session(&self) -> u64 {
        if self.sessions_completed == 0 {
            return 0;
        }
        (self.focused_seconds / u64::from(self.sessions_completed)) / 60
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProfileTotals {
    pub(crate) profile: ProfileBucket,
    pub(crate) pomodoros_completed: u32,
    pub(crate) focused_seconds: u64,
}

impl ProfileTotals {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProfileEffectiveness {
    pub(crate) profile: ProfileBucket,
    pub(crate) sessions_completed: u32,
    pub(crate) focused_seconds: u64,
    pub(crate) active_days: u32,
    pub(crate) focus_share_pct: u8,
}

impl ProfileEffectiveness {
    pub(crate) fn focused_minutes(self) -> u64 {
        self.focused_seconds / 60
    }

    pub(crate) fn average_focused_minutes_per_session(self) -> u64 {
        if self.sessions_completed == 0 {
            return 0;
        }
        (self.focused_seconds / u64::from(self.sessions_completed)) / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskTotals {
    pub(crate) task_label: String,
    pub(crate) pomodoros_completed: u32,
    pub(crate) focused_seconds: u64,
}

impl TaskTotals {
    pub(crate) fn focused_minutes(&self) -> u64 {
        self.focused_seconds / 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskTrend {
    pub(crate) task_label: String,
    pub(crate) recent_pomodoros_completed: u32,
    pub(crate) recent_focused_seconds: u64,
    pub(crate) previous_pomodoros_completed: u32,
    pub(crate) previous_focused_seconds: u64,
}

impl TaskTrend {
    pub(crate) fn recent_focused_minutes(&self) -> u64 {
        self.recent_focused_seconds / 60
    }

    pub(crate) fn previous_focused_minutes(&self) -> u64 {
        self.previous_focused_seconds / 60
    }

    pub(crate) fn delta_focused_seconds(&self) -> i64 {
        let recent = i128::from(self.recent_focused_seconds);
        let previous = i128::from(self.previous_focused_seconds);
        (recent - previous).clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }

    pub(crate) fn delta_focused_minutes(&self) -> i64 {
        self.delta_focused_seconds() / 60
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct StatsExport {
    schema_version: u32,
    daily: Vec<DailyExportRow>,
    weekly: Vec<WeeklyExportRow>,
    sessions: Vec<SessionExportRow>,
    interruptions: Vec<SessionInterruptionExportRow>,
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
    focused_seconds: u64,
    focused_minutes: u64,
    profile: Option<ProfileId>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct SessionInterruptionExportRow {
    timestamp_epoch_secs: u64,
    date: String,
    reason: SessionInterruptionReason,
    task_label: Option<String>,
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
    interruption_timestamp_epoch_secs: Option<u64>,
    interruption_reason: Option<SessionInterruptionReason>,
    interruption_remaining_secs: Option<u64>,
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
    command_usage_counts: BTreeMap<String, u64>,
    #[serde(default)]
    screen_usage_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FocusStats {
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
    command_usage_counts: BTreeMap<String, u64>,
    screen_usage_counts: BTreeMap<String, u64>,
}

pub(crate) fn current_day_key() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests;
