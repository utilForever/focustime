use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::Datelike;
use serde::{Deserialize, Serialize};

use crate::config::ProfileId;
use crate::task_labels::{canonical_task_label, normalize_task_label, task_label_index};

mod helpers;
use helpers::*;
mod analytics;
mod export;
mod persistence;
mod recording;
mod trends;

#[cfg_attr(test, allow(dead_code))]
const STATS_FILE_NAME: &str = "stats.toml";
const JSON_EXPORT_FILE_NAME: &str = "focustime-stats.json";
const CSV_EXPORT_FILE_NAME: &str = "focustime-stats.csv";
const EXPORT_SCHEMA_VERSION: u32 = 5;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

impl FocusStats {
    pub fn session(&self) -> SessionStats {
        self.session
    }

    pub fn daily_for(&self, day_key: &str) -> DailyStats {
        self.daily.get(day_key).copied().unwrap_or_default()
    }

    pub fn daily_entry(&self, day_key: &str) -> Option<DailyStats> {
        self.daily.get(day_key).copied()
    }

    pub fn task_planner_state(&self) -> (Vec<String>, Option<String>) {
        (self.task_labels.clone(), self.selected_task_label.clone())
    }

    pub fn task_label_favorites(&self) -> Vec<String> {
        planner_state_labels_for_keys(&self.task_label_favorites, &self.task_labels)
    }

    pub fn task_label_archived(&self) -> Vec<String> {
        planner_state_labels_for_keys(&self.task_label_archived, &self.task_labels)
    }

    pub fn set_task_goal_target(
        &mut self,
        label: &str,
        target: DailyGoalSnapshot,
    ) -> Result<String, String> {
        let Some(normalized) = normalize_task_label(label) else {
            return Err("Task label cannot be empty.".to_string());
        };
        let canonical = canonical_task_label(&self.task_labels, &normalized).unwrap_or(normalized);
        if task_label_index(&self.task_labels, &canonical).is_none() {
            self.task_labels.push(canonical.clone());
        }
        self.selected_task_label = Some(canonical.clone());
        let key = canonical.to_ascii_lowercase();
        if target.has_any_target() {
            self.task_goal_targets.insert(key, target);
        } else {
            self.task_goal_targets.remove(&key);
        }
        Ok(canonical)
    }

    pub fn remove_task_goal_target(&mut self, label: &str) -> bool {
        let Some(normalized) = normalize_task_label(label) else {
            return false;
        };
        let canonical = canonical_task_label(&self.task_labels, &normalized).unwrap_or(normalized);
        self.task_goal_targets
            .remove(&canonical.to_ascii_lowercase())
            .is_some()
    }

    pub fn rename_task_goal_target(&mut self, previous_label: &str, next_label: &str) -> bool {
        let Some(previous_normalized) = normalize_task_label(previous_label) else {
            return false;
        };
        let Some(next_normalized) = normalize_task_label(next_label) else {
            return false;
        };
        let previous_canonical = canonical_task_label(&self.task_labels, &previous_normalized)
            .unwrap_or(previous_normalized);
        let next_canonical =
            canonical_task_label(&self.task_labels, &next_normalized).unwrap_or(next_normalized);
        let previous_key = previous_canonical.to_ascii_lowercase();
        let next_key = next_canonical.to_ascii_lowercase();
        if previous_key == next_key {
            return false;
        }

        let Some(target) = self.task_goal_targets.remove(&previous_key) else {
            return false;
        };
        self.task_goal_targets.insert(next_key, target);
        true
    }

    pub fn task_goal_progress_for_label(&self, label: &str) -> Option<TaskGoalProgress> {
        let normalized = normalize_task_label(label)?;
        let task_label = canonical_task_label(&self.task_labels, &normalized).unwrap_or(normalized);
        let key = task_label.to_ascii_lowercase();
        let target = self
            .task_goal_targets
            .get(&key)
            .copied()
            .unwrap_or_default();
        let totals_by_key = self.task_totals_by_key();
        let (pomodoros_completed, focused_seconds) =
            totals_by_key.get(&key).copied().unwrap_or((0, 0));
        Some(TaskGoalProgress {
            task_label,
            target,
            pomodoros_completed,
            focused_seconds,
            met: target.is_met_by_totals(focused_seconds / 60, pomodoros_completed),
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn task_goal_progress(&self, limit: usize) -> Vec<TaskGoalProgress> {
        if limit == 0 {
            return Vec::new();
        }

        let totals_by_key = self.task_totals_by_key();
        let mut progress: Vec<TaskGoalProgress> = self
            .task_goal_targets
            .iter()
            .filter_map(|(key, target)| {
                if !target.has_any_target() {
                    return None;
                }
                let (pomodoros_completed, focused_seconds) =
                    totals_by_key.get(key).copied().unwrap_or((0, 0));
                let task_label = self
                    .task_labels
                    .iter()
                    .find(|label| label.eq_ignore_ascii_case(key))
                    .cloned()
                    .unwrap_or_else(|| key.clone());
                Some(TaskGoalProgress {
                    task_label,
                    target: *target,
                    pomodoros_completed,
                    focused_seconds,
                    met: target.is_met_by_totals(focused_seconds / 60, pomodoros_completed),
                })
            })
            .collect();
        progress.sort_by(|left, right| {
            left.met
                .cmp(&right.met)
                .then_with(|| right.focused_seconds.cmp(&left.focused_seconds))
                .then_with(|| right.pomodoros_completed.cmp(&left.pomodoros_completed))
                .then_with(|| left.task_label.cmp(&right.task_label))
        });
        progress.truncate(limit);
        progress
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn update_task_planner_state(
        &mut self,
        labels: Vec<String>,
        selected: Option<String>,
    ) -> bool {
        self.update_task_planner_state_with_label_states(
            labels,
            selected,
            self.task_label_favorites(),
            self.task_label_archived(),
        )
    }

    pub fn update_task_planner_state_with_label_states(
        &mut self,
        labels: Vec<String>,
        selected: Option<String>,
        favorites: Vec<String>,
        archived: Vec<String>,
    ) -> bool {
        let (task_labels, selected_task_label, task_label_favorites, task_label_archived) =
            normalize_task_planner_state(labels, selected, favorites, archived);
        if self.task_labels == task_labels
            && self.selected_task_label == selected_task_label
            && self.task_label_favorites == task_label_favorites
            && self.task_label_archived == task_label_archived
        {
            return false;
        }

        self.task_labels = task_labels;
        self.selected_task_label = selected_task_label;
        self.task_label_favorites = task_label_favorites;
        self.task_label_archived = task_label_archived;
        true
    }
}

pub fn current_day_key() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests;
