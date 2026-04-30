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
    pub fn record_focus_elapsed(
        &mut self,
        day_key: &str,
        elapsed_secs: u64,
        goal: DailyGoalSnapshot,
    ) {
        if elapsed_secs == 0 {
            return;
        }

        self.session.focused_seconds = self.session.focused_seconds.saturating_add(elapsed_secs);
        let daily = self.daily.entry(day_key.to_string()).or_default();
        daily.focused_seconds = daily.focused_seconds.saturating_add(elapsed_secs);
        daily.goal = Some(goal);
    }

    pub fn record_completed_pomodoro(&mut self, day_key: &str, goal: DailyGoalSnapshot) {
        self.record_completed_pomodoro_with_task(day_key, goal, None, 0, None);
    }

    pub fn record_completed_pomodoro_with_task(
        &mut self,
        day_key: &str,
        goal: DailyGoalSnapshot,
        task_label: Option<&str>,
        focused_seconds: u64,
        profile: Option<ProfileId>,
    ) {
        self.record_completed_pomodoro_with_metadata(
            day_key,
            goal,
            FocusSessionMetadata {
                task_label,
                focus_intention: task_label,
                task_note: task_label,
            },
            focused_seconds,
            profile,
        );
    }

    pub fn record_completed_pomodoro_with_metadata(
        &mut self,
        day_key: &str,
        goal: DailyGoalSnapshot,
        metadata: FocusSessionMetadata<'_>,
        focused_seconds: u64,
        profile: Option<ProfileId>,
    ) {
        self.session.pomodoros_completed = self.session.pomodoros_completed.saturating_add(1);
        let daily = self.daily.entry(day_key.to_string()).or_default();
        daily.pomodoros_completed = daily.pomodoros_completed.saturating_add(1);
        daily.goal = Some(goal);

        if let Some(task_label) = metadata.task_label.and_then(normalize_task_label) {
            if task_label_index(&self.task_labels, &task_label).is_none() {
                self.task_labels.push(task_label.clone());
            }
            self.selected_task_label = Some(task_label.clone());
            let focus_intention = metadata
                .focus_intention
                .and_then(normalize_session_metadata_text)
                .unwrap_or_else(|| task_label.clone());
            let task_note = metadata
                .task_note
                .and_then(normalize_session_metadata_text)
                .unwrap_or_else(|| task_label.clone());
            self.focus_sessions.push(FocusSessionRecord {
                date: day_key.to_string(),
                task_label,
                focus_intention,
                task_note,
                focused_seconds,
                profile,
            });
        }
    }

    pub fn record_break_glass_override_event(
        &mut self,
        day_key: &str,
        timestamp_epoch_secs: u64,
        task_label: Option<&str>,
        duration_seconds: u64,
    ) {
        if duration_seconds == 0 {
            return;
        }

        let normalized_task_label = task_label.and_then(normalize_task_label);
        if let Some(task_label) = normalized_task_label.as_ref()
            && task_label_index(&self.task_labels, task_label).is_none()
        {
            self.task_labels.push(task_label.clone());
        }

        self.break_glass_overrides.push(BreakGlassOverrideEvent {
            timestamp_epoch_secs,
            date: day_key.to_string(),
            task_label: normalized_task_label,
            duration_seconds,
        });
    }

    pub fn record_session_interruption_event(
        &mut self,
        day_key: &str,
        timestamp_epoch_secs: u64,
        reason: SessionInterruptionReason,
        metadata: FocusSessionMetadata<'_>,
        remaining_secs: u64,
        profile: Option<ProfileId>,
    ) {
        let normalized_task_label = metadata.task_label.and_then(normalize_task_label);
        if let Some(task_label) = normalized_task_label.as_ref()
            && task_label_index(&self.task_labels, task_label).is_none()
        {
            self.task_labels.push(task_label.clone());
        }

        self.session_interruptions.push(SessionInterruptionEvent {
            timestamp_epoch_secs,
            date: day_key.to_string(),
            reason,
            task_label: normalized_task_label,
            focus_intention: metadata
                .focus_intention
                .and_then(normalize_session_metadata_text),
            task_note: metadata.task_note.and_then(normalize_session_metadata_text),
            remaining_secs,
            profile,
        });
    }

    pub fn sync_goal_snapshot(&mut self, day_key: &str, goal: DailyGoalSnapshot) -> bool {
        let daily = self.daily.entry(day_key.to_string()).or_default();

        if daily.goal == Some(goal) {
            return false;
        }

        daily.goal = Some(goal);
        true
    }

    pub fn sync_weekly_goal_snapshot(
        &mut self,
        day: chrono::NaiveDate,
        goal: DailyGoalSnapshot,
    ) -> bool {
        let key = week_key_for_day(day);
        if self.weekly_goal_snapshots.get(&key) == Some(&goal) {
            return false;
        }
        self.weekly_goal_snapshots.insert(key, goal);
        true
    }

    pub fn sync_monthly_goal_snapshot(
        &mut self,
        day: chrono::NaiveDate,
        goal: DailyGoalSnapshot,
    ) -> bool {
        let key = month_key_for_day(day);
        if self.monthly_goal_snapshots.get(&key) == Some(&goal) {
            return false;
        }
        self.monthly_goal_snapshots.insert(key, goal);
        true
    }

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

    pub fn recent_task_labels(&self, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }

        let mut recent = Vec::new();
        let mut seen = BTreeSet::new();
        for session in self.focus_sessions.iter().rev() {
            let Some(task_label) = normalize_task_label(&session.task_label) else {
                continue;
            };
            let key = task_label.to_ascii_lowercase();
            if !seen.insert(key) {
                continue;
            }
            recent.push(task_label);
            if recent.len() >= limit {
                break;
            }
        }
        recent
    }

    fn task_totals_by_key(&self) -> BTreeMap<String, (u32, u64)> {
        let mut by_task: BTreeMap<String, (u32, u64)> = BTreeMap::new();
        for session in &self.focus_sessions {
            let Some(task_label) = normalize_task_label(&session.task_label) else {
                continue;
            };
            let key = task_label.to_ascii_lowercase();
            let entry = by_task.entry(key).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(1);
            entry.1 = entry.1.saturating_add(session.focused_seconds);
        }
        by_task
    }

    pub fn recent_daily(&self, limit: usize) -> Vec<(String, DailyStats)> {
        self.daily
            .iter()
            .rev()
            .take(limit)
            .map(|(day, stats)| (day.clone(), *stats))
            .collect()
    }

    pub fn task_totals(&self, limit: usize) -> Vec<TaskTotals> {
        if limit == 0 {
            return Vec::new();
        }

        let mut by_task: BTreeMap<String, TaskTotals> = BTreeMap::new();
        for session in &self.focus_sessions {
            let Some(task_label) = normalize_task_label(&session.task_label) else {
                continue;
            };
            let task_label =
                canonical_task_label(&self.task_labels, &task_label).unwrap_or(task_label);
            let key = task_label.to_ascii_lowercase();
            let entry = by_task.entry(key).or_insert(TaskTotals {
                task_label,
                pomodoros_completed: 0,
                focused_seconds: 0,
            });
            entry.pomodoros_completed = entry.pomodoros_completed.saturating_add(1);
            entry.focused_seconds = entry
                .focused_seconds
                .saturating_add(session.focused_seconds);
        }

        let mut totals: Vec<TaskTotals> = by_task.into_values().collect();
        totals.sort_by(|left, right| {
            right
                .focused_seconds
                .cmp(&left.focused_seconds)
                .then_with(|| right.pomodoros_completed.cmp(&left.pomodoros_completed))
                .then_with(|| left.task_label.cmp(&right.task_label))
        });
        totals.truncate(limit);
        totals
    }

    pub fn recent_task_trends(&self, limit: usize) -> Vec<TaskTrend> {
        if limit == 0 {
            return Vec::new();
        }

        let Some(window) = self.task_trend_window() else {
            return Vec::new();
        };
        let mut trends = self.task_trends_for_window(window);
        trends.truncate(limit);
        trends
    }

    pub fn recent_break_glass_overrides(&self, limit: usize) -> Vec<BreakGlassOverrideEvent> {
        self.break_glass_overrides
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    #[cfg(test)]
    pub fn recent_session_interruptions(&self, limit: usize) -> Vec<SessionInterruptionEvent> {
        self.session_interruptions
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn latest_session_interruption(&self) -> Option<SessionInterruptionEvent> {
        self.session_interruptions
            .iter()
            .max_by_key(|event| event.timestamp_epoch_secs)
            .cloned()
    }

    fn task_trend_window(&self) -> Option<TaskTrendWindow> {
        if self.focus_sessions.is_empty() {
            return None;
        }

        let recent_end = chrono::Local::now().date_naive();
        let recent_start = recent_end.checked_sub_signed(chrono::Duration::days(6))?;
        let previous_end = recent_start.checked_sub_signed(chrono::Duration::days(1))?;
        let previous_start = previous_end.checked_sub_signed(chrono::Duration::days(6))?;

        Some(TaskTrendWindow {
            recent_start,
            recent_end,
            previous_start,
            previous_end,
        })
    }

    fn task_trends_for_window(&self, window: TaskTrendWindow) -> Vec<TaskTrend> {
        let mut by_task: BTreeMap<String, (String, TaskTrendAccumulator)> = BTreeMap::new();

        for session in &self.focus_sessions {
            let Ok(day) = chrono::NaiveDate::parse_from_str(&session.date, "%Y-%m-%d") else {
                continue;
            };
            if day < window.previous_start || day > window.recent_end {
                continue;
            }

            let Some(task_label) = normalize_task_label(&session.task_label) else {
                continue;
            };
            let task_label =
                canonical_task_label(&self.task_labels, &task_label).unwrap_or(task_label);
            let key = task_label.to_ascii_lowercase();
            let (_, entry) = by_task
                .entry(key)
                .or_insert_with(|| (task_label, TaskTrendAccumulator::default()));
            if day >= window.recent_start {
                entry.recent_pomodoros_completed =
                    entry.recent_pomodoros_completed.saturating_add(1);
                entry.recent_focused_seconds = entry
                    .recent_focused_seconds
                    .saturating_add(session.focused_seconds);
            } else if day <= window.previous_end {
                entry.previous_pomodoros_completed =
                    entry.previous_pomodoros_completed.saturating_add(1);
                entry.previous_focused_seconds = entry
                    .previous_focused_seconds
                    .saturating_add(session.focused_seconds);
            }
        }

        let mut trends: Vec<TaskTrend> = by_task
            .into_iter()
            .filter_map(|(_, (task_label, totals))| {
                let has_data = totals.recent_pomodoros_completed > 0
                    || totals.previous_pomodoros_completed > 0
                    || totals.recent_focused_seconds > 0
                    || totals.previous_focused_seconds > 0;
                has_data.then_some(TaskTrend {
                    task_label,
                    recent_pomodoros_completed: totals.recent_pomodoros_completed,
                    recent_focused_seconds: totals.recent_focused_seconds,
                    previous_pomodoros_completed: totals.previous_pomodoros_completed,
                    previous_focused_seconds: totals.previous_focused_seconds,
                })
            })
            .collect();

        trends.sort_by(|left, right| {
            right
                .recent_focused_seconds
                .cmp(&left.recent_focused_seconds)
                .then_with(|| {
                    right
                        .previous_focused_seconds
                        .cmp(&left.previous_focused_seconds)
                })
                .then_with(|| {
                    right
                        .recent_pomodoros_completed
                        .cmp(&left.recent_pomodoros_completed)
                })
                .then_with(|| left.task_label.cmp(&right.task_label))
        });
        trends
    }

    #[cfg(test)]
    pub fn insert_daily_for_tests(&mut self, day_key: &str, stats: DailyStats) {
        self.daily.insert(day_key.to_string(), stats);
    }

    pub fn goal_streak(
        &self,
        today: chrono::NaiveDate,
        current_goal: DailyGoalSnapshot,
        today_stats: DailyStats,
    ) -> GoalStreak {
        let completed_days = self.completed_goal_days(today, current_goal, today_stats);
        GoalStreak {
            current: current_goal_streak(&completed_days, today, current_goal, today_stats),
            best: best_goal_streak(&completed_days),
        }
    }

    fn completed_goal_days(
        &self,
        today: chrono::NaiveDate,
        current_goal: DailyGoalSnapshot,
        today_stats: DailyStats,
    ) -> BTreeSet<chrono::NaiveDate> {
        let mut completed_days = BTreeSet::new();

        for (day_key, stats) in &self.daily {
            let Ok(day) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            if day == today {
                continue;
            }

            let snapshot = stats
                .goal
                .or(current_goal.has_any_target().then_some(current_goal));
            if snapshot.is_some_and(|goal| goal.is_met_by(*stats)) {
                completed_days.insert(day);
            }
        }

        if current_goal.is_met_by(today_stats) {
            completed_days.insert(today);
        }

        completed_days
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
