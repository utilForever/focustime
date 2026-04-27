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

#[cfg_attr(test, allow(dead_code))]
const STATS_FILE_NAME: &str = "stats.toml";
const JSON_EXPORT_FILE_NAME: &str = "focustime-stats.json";
const CSV_EXPORT_FILE_NAME: &str = "focustime-stats.csv";
const EXPORT_SCHEMA_VERSION: u32 = 3;
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
    overrides: Vec<BreakGlassOverrideExportRow>,
    task_totals: Vec<TaskTotalsExportRow>,
    task_trends: Vec<TaskTrendExportRow>,
    weekly_consistency: Vec<WeeklyConsistencyExportRow>,
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
    average_focused_minutes_per_session: Option<u64>,
    focus_share_pct: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct PersistedStats {
    #[serde(default)]
    daily: BTreeMap<String, DailyStats>,
    #[serde(default)]
    task_labels: Vec<String>,
    #[serde(default)]
    selected_task_label: Option<String>,
    #[serde(default)]
    focus_sessions: Vec<FocusSessionRecord>,
    #[serde(default)]
    break_glass_overrides: Vec<BreakGlassOverrideEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct FocusStats {
    session: SessionStats,
    daily: BTreeMap<String, DailyStats>,
    task_labels: Vec<String>,
    selected_task_label: Option<String>,
    focus_sessions: Vec<FocusSessionRecord>,
    break_glass_overrides: Vec<BreakGlassOverrideEvent>,
}

impl FocusStats {
    #[cfg(test)]
    pub fn load() -> Result<Self, String> {
        Ok(Self::default())
    }

    #[cfg(not(test))]
    pub fn load() -> Result<Self, String> {
        Self::try_load()
    }

    #[cfg(not(test))]
    fn try_load() -> Result<Self, String> {
        let path = crate::config::app_data_path(STATS_FILE_NAME)
            .ok_or_else(|| "cannot determine stats directory".to_string())?;
        match fs::read_to_string(path) {
            Ok(content) => Self::try_from_toml(&content),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("stats read failed: {e}")),
        }
    }

    fn try_from_toml(content: &str) -> Result<Self, String> {
        let persisted: PersistedStats =
            toml::from_str(content).map_err(|e| format!("stats parse failed: {e}"))?;
        Ok(Self::from_persisted(persisted))
    }

    fn from_persisted(persisted: PersistedStats) -> Self {
        let (task_labels, selected_task_label) =
            normalize_task_planner_state(persisted.task_labels, persisted.selected_task_label);
        let mut focus_sessions = Vec::new();
        for session in persisted.focus_sessions {
            if let Some(task_label) = normalize_task_label(&session.task_label) {
                let focus_intention = normalize_session_metadata_text(&session.focus_intention)
                    .unwrap_or_else(|| task_label.clone());
                let task_note = normalize_session_metadata_text(&session.task_note)
                    .unwrap_or_else(|| task_label.clone());
                focus_sessions.push(FocusSessionRecord {
                    date: session.date.trim().to_string(),
                    task_label,
                    focus_intention,
                    task_note,
                    focused_seconds: session.focused_seconds,
                    profile: session.profile,
                });
            }
        }
        let mut break_glass_overrides = Vec::new();
        for event in persisted.break_glass_overrides {
            if event.duration_seconds == 0 {
                continue;
            }
            break_glass_overrides.push(BreakGlassOverrideEvent {
                timestamp_epoch_secs: event.timestamp_epoch_secs,
                date: event.date.trim().to_string(),
                task_label: event
                    .task_label
                    .and_then(|label| normalize_task_label(&label)),
                duration_seconds: event.duration_seconds,
            });
        }
        Self {
            session: SessionStats::default(),
            daily: persisted.daily,
            task_labels,
            selected_task_label,
            focus_sessions,
            break_glass_overrides,
        }
    }

    fn to_persisted(&self) -> PersistedStats {
        PersistedStats {
            daily: self.daily.clone(),
            task_labels: self.task_labels.clone(),
            selected_task_label: self.selected_task_label.clone(),
            focus_sessions: self.focus_sessions.clone(),
            break_glass_overrides: self.break_glass_overrides.clone(),
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn save(&self) -> io::Result<()> {
        let path = crate::config::app_data_path(STATS_FILE_NAME).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "cannot determine stats directory")
        })?;

        let content = toml::to_string_pretty(&self.to_persisted())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        write_atomic_bytes(&path, content.as_bytes())
    }

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

    pub fn sync_goal_snapshot(&mut self, day_key: &str, goal: DailyGoalSnapshot) -> bool {
        let Some(daily) = self.daily.get_mut(day_key) else {
            return false;
        };

        if daily.goal == Some(goal) {
            return false;
        }

        daily.goal = Some(goal);
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

    pub fn weekly_for_day(&self, day: chrono::NaiveDate) -> WeeklyStats {
        let week = day.iso_week();
        let mut totals = WeeklyStats {
            year: week.year(),
            week: week.week(),
            ..WeeklyStats::default()
        };
        for (day_key, stats) in &self.daily {
            let Ok(candidate) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            let candidate_week = candidate.iso_week();
            if candidate_week.year() != totals.year || candidate_week.week() != totals.week {
                continue;
            }
            totals.pomodoros_completed = totals
                .pomodoros_completed
                .saturating_add(stats.pomodoros_completed);
            totals.focused_seconds = totals.focused_seconds.saturating_add(stats.focused_seconds);
        }
        totals
    }

    pub fn weekly_for_day_if_present(&self, day: chrono::NaiveDate) -> Option<WeeklyStats> {
        let week = self.weekly_for_day(day);
        (week.focused_seconds > 0 || week.pomodoros_completed > 0).then_some(week)
    }

    pub fn monthly_for_day(&self, day: chrono::NaiveDate) -> MonthlyStats {
        let mut totals = MonthlyStats {
            year: day.year(),
            month: day.month(),
            ..MonthlyStats::default()
        };
        for (day_key, stats) in &self.daily {
            let Ok(candidate) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            if candidate.year() != totals.year || candidate.month() != totals.month {
                continue;
            }
            totals.pomodoros_completed = totals
                .pomodoros_completed
                .saturating_add(stats.pomodoros_completed);
            totals.focused_seconds = totals.focused_seconds.saturating_add(stats.focused_seconds);
        }
        totals
    }

    pub fn monthly_for_day_if_present(&self, day: chrono::NaiveDate) -> Option<MonthlyStats> {
        let month = self.monthly_for_day(day);
        (month.focused_seconds > 0 || month.pomodoros_completed > 0).then_some(month)
    }

    pub fn task_planner_state(&self) -> (Vec<String>, Option<String>) {
        (self.task_labels.clone(), self.selected_task_label.clone())
    }

    pub fn update_task_planner_state(
        &mut self,
        labels: Vec<String>,
        selected: Option<String>,
    ) -> bool {
        let (task_labels, selected_task_label) = normalize_task_planner_state(labels, selected);
        if self.task_labels == task_labels && self.selected_task_label == selected_task_label {
            return false;
        }

        self.task_labels = task_labels;
        self.selected_task_label = selected_task_label;
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

    pub fn recent_daily(&self, limit: usize) -> Vec<(String, DailyStats)> {
        self.daily
            .iter()
            .rev()
            .take(limit)
            .map(|(day, stats)| (day.clone(), *stats))
            .collect()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn recent_weekly(&self, limit: usize) -> Vec<WeeklyStats> {
        let mut weekly = self.weekly_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
    }

    pub fn recent_weekly_consistency(&self, limit: usize) -> Vec<WeeklyConsistency> {
        let mut weekly = self.weekly_consistency_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
    }

    pub fn latest_weekly_consistency(&self) -> Option<WeeklyConsistency> {
        self.recent_weekly_consistency(1).into_iter().next()
    }

    pub fn recent_monthly(&self, limit: usize) -> Vec<MonthlyStats> {
        let mut monthly = self.monthly_stats();
        monthly.reverse();
        monthly.truncate(limit);
        monthly
    }

    pub fn latest_monthly_heatmap(&self) -> MonthlyHeatmap {
        let (year, month) = self.latest_recorded_month_key().unwrap_or_else(|| {
            let now = chrono::Local::now().date_naive();
            (now.year(), now.month())
        });
        self.monthly_heatmap(year, month)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn profile_totals(&self) -> Vec<ProfileTotals> {
        let mut by_profile: BTreeMap<ProfileBucket, ProfileTotals> = BTreeMap::new();
        for session in &self.focus_sessions {
            let profile = profile_bucket_for(session.profile);
            let entry = by_profile.entry(profile).or_insert(ProfileTotals {
                profile,
                pomodoros_completed: 0,
                focused_seconds: 0,
            });
            entry.pomodoros_completed = entry.pomodoros_completed.saturating_add(1);
            entry.focused_seconds = entry
                .focused_seconds
                .saturating_add(session.focused_seconds);
        }

        let mut totals: Vec<ProfileTotals> = by_profile.into_values().collect();
        totals.sort_by(|left, right| {
            right
                .focused_seconds
                .cmp(&left.focused_seconds)
                .then_with(|| right.pomodoros_completed.cmp(&left.pomodoros_completed))
                .then_with(|| left.profile.cmp(&right.profile))
        });
        totals
    }

    pub fn profile_effectiveness(&self) -> Vec<ProfileEffectiveness> {
        let mut by_profile: BTreeMap<ProfileBucket, ProfileEffectivenessAccumulator> =
            BTreeMap::new();
        let mut total_focused_seconds: u64 = 0;
        for session in &self.focus_sessions {
            let profile = profile_bucket_for(session.profile);
            let entry = by_profile.entry(profile).or_default();
            entry.sessions_completed = entry.sessions_completed.saturating_add(1);
            entry.focused_seconds = entry
                .focused_seconds
                .saturating_add(session.focused_seconds);
            entry.active_days.insert(session.date.clone());
            total_focused_seconds = total_focused_seconds.saturating_add(session.focused_seconds);
        }

        let mut effectiveness: Vec<ProfileEffectiveness> = by_profile
            .into_iter()
            .map(|(profile, totals)| ProfileEffectiveness {
                profile,
                sessions_completed: totals.sessions_completed,
                focused_seconds: totals.focused_seconds,
                active_days: totals.active_days.len() as u32,
                focus_share_pct: percentage_round_nearest(
                    totals.focused_seconds,
                    total_focused_seconds,
                ),
            })
            .collect();
        effectiveness.sort_by(|left, right| {
            right
                .focus_share_pct
                .cmp(&left.focus_share_pct)
                .then_with(|| {
                    right
                        .average_focused_minutes_per_session()
                        .cmp(&left.average_focused_minutes_per_session())
                })
                .then_with(|| right.sessions_completed.cmp(&left.sessions_completed))
                .then_with(|| left.profile.cmp(&right.profile))
        });
        effectiveness
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

    pub fn export_to_dir(&self, dir: &Path) -> io::Result<ExportedStatsFiles> {
        fs::create_dir_all(dir)?;
        let export = self.export_data();
        let json_path = dir.join(JSON_EXPORT_FILE_NAME);
        let csv_path = dir.join(CSV_EXPORT_FILE_NAME);

        let json_content = serde_json::to_vec_pretty(&export)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let csv_content = export.to_csv_bytes()?;

        write_atomic_bytes(&json_path, &json_content)?;
        write_atomic_bytes(&csv_path, &csv_content)?;

        Ok(ExportedStatsFiles {
            json_path,
            csv_path,
        })
    }

    fn export_data(&self) -> StatsExport {
        StatsExport {
            schema_version: EXPORT_SCHEMA_VERSION,
            daily: self.export_daily_rows(),
            weekly: self.export_weekly_rows(),
            sessions: self.export_session_rows(),
            overrides: self.export_break_glass_override_rows(),
            task_totals: self.export_task_totals_rows(),
            task_trends: self.export_task_trend_rows(),
            weekly_consistency: self.export_weekly_consistency_rows(),
            profile_effectiveness: self.export_profile_effectiveness_rows(),
        }
    }

    fn export_daily_rows(&self) -> Vec<DailyExportRow> {
        self.daily
            .iter()
            .map(|(date, stats)| {
                let goal_met = stats.goal.is_some_and(|goal| goal.is_met_by(*stats));
                DailyExportRow {
                    date: date.clone(),
                    pomodoros_completed: stats.pomodoros_completed,
                    focused_seconds: stats.focused_seconds,
                    focused_minutes: stats.focused_minutes(),
                    goal: stats.goal,
                    goal_met,
                }
            })
            .collect()
    }

    fn export_weekly_rows(&self) -> Vec<WeeklyExportRow> {
        self.weekly_stats()
            .into_iter()
            .map(|stats| WeeklyExportRow {
                year: stats.year,
                week: stats.week,
                week_label: format_week_label(stats.year, stats.week),
                pomodoros_completed: stats.pomodoros_completed,
                focused_seconds: stats.focused_seconds,
                focused_minutes: stats.focused_minutes(),
            })
            .collect()
    }

    fn export_session_rows(&self) -> Vec<SessionExportRow> {
        self.focus_sessions
            .iter()
            .map(|session| SessionExportRow {
                date: session.date.clone(),
                task_label: session.task_label.clone(),
                focus_intention: session.focus_intention.clone(),
                task_note: session.task_note.clone(),
                focused_seconds: session.focused_seconds,
                focused_minutes: session.focused_seconds / 60,
                profile: session.profile,
            })
            .collect()
    }

    fn export_break_glass_override_rows(&self) -> Vec<BreakGlassOverrideExportRow> {
        self.break_glass_overrides
            .iter()
            .map(|event| BreakGlassOverrideExportRow {
                timestamp_epoch_secs: event.timestamp_epoch_secs,
                date: event.date.clone(),
                task_label: event.task_label.clone(),
                duration_seconds: event.duration_seconds,
                duration_minutes: event.duration_seconds / 60,
            })
            .collect()
    }

    fn export_task_totals_rows(&self) -> Vec<TaskTotalsExportRow> {
        self.task_totals(usize::MAX)
            .into_iter()
            .map(|totals| {
                let focused_minutes = totals.focused_minutes();
                TaskTotalsExportRow {
                    task_label: totals.task_label,
                    pomodoros_completed: totals.pomodoros_completed,
                    focused_seconds: totals.focused_seconds,
                    focused_minutes,
                }
            })
            .collect()
    }

    fn export_task_trend_rows(&self) -> Vec<TaskTrendExportRow> {
        let Some(window) = self.task_trend_window() else {
            return Vec::new();
        };

        let recent_window_start = window.recent_start.format("%Y-%m-%d").to_string();
        let recent_window_end = window.recent_end.format("%Y-%m-%d").to_string();
        let previous_window_start = window.previous_start.format("%Y-%m-%d").to_string();
        let previous_window_end = window.previous_end.format("%Y-%m-%d").to_string();

        self.task_trends_for_window(window)
            .into_iter()
            .map(|trend| {
                let recent_focused_minutes = trend.recent_focused_minutes();
                let previous_focused_minutes = trend.previous_focused_minutes();
                let delta_focused_seconds = trend.delta_focused_seconds();
                let delta_focused_minutes = trend.delta_focused_minutes();
                TaskTrendExportRow {
                    task_label: trend.task_label,
                    recent_window_start: recent_window_start.clone(),
                    recent_window_end: recent_window_end.clone(),
                    previous_window_start: previous_window_start.clone(),
                    previous_window_end: previous_window_end.clone(),
                    recent_pomodoros_completed: trend.recent_pomodoros_completed,
                    recent_focused_seconds: trend.recent_focused_seconds,
                    recent_focused_minutes,
                    previous_pomodoros_completed: trend.previous_pomodoros_completed,
                    previous_focused_seconds: trend.previous_focused_seconds,
                    previous_focused_minutes,
                    delta_focused_seconds,
                    delta_focused_minutes,
                }
            })
            .collect()
    }

    fn export_weekly_consistency_rows(&self) -> Vec<WeeklyConsistencyExportRow> {
        self.weekly_consistency_stats()
            .into_iter()
            .map(|entry| WeeklyConsistencyExportRow {
                year: entry.year,
                week: entry.week,
                week_label: entry.week_label,
                active_days: entry.active_days,
                consistency_score_pct: entry.consistency_score_pct,
            })
            .collect()
    }

    fn export_profile_effectiveness_rows(&self) -> Vec<ProfileEffectivenessExportRow> {
        self.profile_effectiveness()
            .into_iter()
            .map(|entry| ProfileEffectivenessExportRow {
                profile: entry.profile.label().to_string(),
                sessions_completed: entry.sessions_completed,
                active_days: entry.active_days,
                focused_seconds: entry.focused_seconds,
                focused_minutes: entry.focused_minutes(),
                average_focused_minutes_per_session: entry.average_focused_minutes_per_session(),
                focus_share_pct: entry.focus_share_pct,
            })
            .collect()
    }

    fn weekly_consistency_stats(&self) -> Vec<WeeklyConsistency> {
        let mut weekly = BTreeMap::new();
        for (day_key, stats) in &self.daily {
            if !daily_has_activity(*stats) {
                continue;
            }
            let Ok(day) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            let iso_week = day.iso_week();
            let active_days = weekly
                .entry((iso_week.year(), iso_week.week()))
                .or_insert(0_u8);
            *active_days = active_days.saturating_add(1).min(7);
        }

        weekly
            .into_iter()
            .map(|((year, week), active_days)| WeeklyConsistency {
                year,
                week,
                week_label: format_week_label(year, week),
                active_days,
                consistency_score_pct: consistency_score_from_active_days(active_days),
            })
            .collect()
    }

    fn weekly_stats(&self) -> Vec<WeeklyStats> {
        let mut weekly = BTreeMap::new();

        for (day_key, stats) in &self.daily {
            let Ok(day) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            let iso_week = day.iso_week();
            let entry = weekly
                .entry((iso_week.year(), iso_week.week()))
                .or_insert_with(|| WeeklyStats {
                    year: iso_week.year(),
                    week: iso_week.week(),
                    ..WeeklyStats::default()
                });
            entry.pomodoros_completed = entry
                .pomodoros_completed
                .saturating_add(stats.pomodoros_completed);
            entry.focused_seconds = entry.focused_seconds.saturating_add(stats.focused_seconds);
        }

        weekly.into_values().collect()
    }

    fn monthly_stats(&self) -> Vec<MonthlyStats> {
        let mut monthly = BTreeMap::new();

        for (day_key, stats) in &self.daily {
            let Ok(day) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            let entry = monthly
                .entry((day.year(), day.month()))
                .or_insert_with(|| MonthlyStats {
                    year: day.year(),
                    month: day.month(),
                    ..MonthlyStats::default()
                });
            entry.pomodoros_completed = entry
                .pomodoros_completed
                .saturating_add(stats.pomodoros_completed);
            entry.focused_seconds = entry.focused_seconds.saturating_add(stats.focused_seconds);
        }

        monthly.into_values().collect()
    }

    fn latest_recorded_month_key(&self) -> Option<(i32, u32)> {
        self.daily
            .keys()
            .rev()
            .find_map(|day_key| chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d").ok())
            .map(|day| (day.year(), day.month()))
    }

    fn monthly_heatmap(&self, year: i32, month: u32) -> MonthlyHeatmap {
        let (year, month) = if chrono::NaiveDate::from_ymd_opt(year, month, 1).is_some() {
            (year, month)
        } else {
            let now = chrono::Local::now().date_naive();
            (now.year(), now.month())
        };

        let month_start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .expect("validated month/year should produce valid first day");
        let days_in_month = days_in_month(year, month);
        let mut max_focused_minutes = 0;
        let mut days = Vec::with_capacity(days_in_month as usize);
        for day in 1..=days_in_month {
            let day_key = format!("{year:04}-{month:02}-{day:02}");
            let stats = self.daily_for(&day_key);
            let focused_minutes = stats.focused_minutes();
            max_focused_minutes = max_focused_minutes.max(focused_minutes);
            days.push(HeatmapDayStats {
                day,
                pomodoros_completed: stats.pomodoros_completed,
                focused_seconds: stats.focused_seconds,
            });
        }

        MonthlyHeatmap {
            year,
            month,
            first_weekday_monday0: month_start.weekday().num_days_from_monday(),
            days_in_month,
            max_focused_minutes,
            days,
        }
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

impl StatsExport {
    fn to_csv_bytes(&self) -> io::Result<Vec<u8>> {
        let mut writer = csv::Writer::from_writer(Vec::new());

        for row in self.csv_rows() {
            writer
                .serialize(row)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        }

        writer.flush()?;
        writer
            .into_inner()
            .map_err(|e| io::Error::other(format!("csv export finalize failed: {e}")))
    }

    fn csv_row_defaults(record_type: &'static str) -> CsvExportRow {
        CsvExportRow {
            schema_version: EXPORT_SCHEMA_VERSION,
            record_type,
            date: None,
            week_label: None,
            year: None,
            week: None,
            pomodoros_completed: 0,
            focused_seconds: 0,
            focused_minutes: 0,
            goal_minutes: None,
            goal_pomodoros: None,
            goal_met: None,
            task_label: None,
            break_glass_timestamp_epoch_secs: None,
            break_glass_duration_seconds: None,
            focus_intention: None,
            task_note: None,
            recent_window_start: None,
            recent_window_end: None,
            previous_window_start: None,
            previous_window_end: None,
            previous_pomodoros_completed: None,
            previous_focused_seconds: None,
            previous_focused_minutes: None,
            delta_focused_seconds: None,
            delta_focused_minutes: None,
            profile_name: None,
            sessions_completed: None,
            active_days: None,
            consistency_score_pct: None,
            average_focused_minutes_per_session: None,
            focus_share_pct: None,
        }
    }

    fn csv_rows(&self) -> Vec<CsvExportRow> {
        let mut rows = Vec::with_capacity(
            self.daily.len()
                + self.weekly.len()
                + self.sessions.len()
                + self.overrides.len()
                + self.task_totals.len()
                + self.task_trends.len()
                + self.weekly_consistency.len()
                + self.profile_effectiveness.len(),
        );

        for daily in &self.daily {
            rows.push(CsvExportRow {
                date: Some(daily.date.clone()),
                pomodoros_completed: daily.pomodoros_completed,
                focused_seconds: daily.focused_seconds,
                focused_minutes: daily.focused_minutes,
                goal_minutes: daily.goal.map(|goal| goal.minutes),
                goal_pomodoros: daily.goal.map(|goal| goal.pomodoros),
                goal_met: daily.goal.map(|_| daily.goal_met),
                ..Self::csv_row_defaults("daily")
            });
        }

        for weekly in &self.weekly {
            rows.push(CsvExportRow {
                week_label: Some(weekly.week_label.clone()),
                year: Some(weekly.year),
                week: Some(weekly.week),
                pomodoros_completed: weekly.pomodoros_completed,
                focused_seconds: weekly.focused_seconds,
                focused_minutes: weekly.focused_minutes,
                ..Self::csv_row_defaults("weekly")
            });
        }

        for session in &self.sessions {
            rows.push(CsvExportRow {
                date: Some(session.date.clone()),
                pomodoros_completed: 1,
                focused_seconds: session.focused_seconds,
                focused_minutes: session.focused_minutes,
                task_label: Some(session.task_label.clone()),
                focus_intention: Some(session.focus_intention.clone()),
                task_note: Some(session.task_note.clone()),
                ..Self::csv_row_defaults("focus_session")
            });
        }

        for override_event in &self.overrides {
            rows.push(CsvExportRow {
                date: Some(override_event.date.clone()),
                task_label: override_event.task_label.clone(),
                break_glass_timestamp_epoch_secs: Some(override_event.timestamp_epoch_secs),
                break_glass_duration_seconds: Some(override_event.duration_seconds),
                ..Self::csv_row_defaults("break_glass_override")
            });
        }

        for task_total in &self.task_totals {
            rows.push(CsvExportRow {
                pomodoros_completed: task_total.pomodoros_completed,
                focused_seconds: task_total.focused_seconds,
                focused_minutes: task_total.focused_minutes,
                task_label: Some(task_total.task_label.clone()),
                ..Self::csv_row_defaults("task_summary")
            });
        }

        for task_trend in &self.task_trends {
            rows.push(CsvExportRow {
                pomodoros_completed: task_trend.recent_pomodoros_completed,
                focused_seconds: task_trend.recent_focused_seconds,
                focused_minutes: task_trend.recent_focused_minutes,
                task_label: Some(task_trend.task_label.clone()),
                recent_window_start: Some(task_trend.recent_window_start.clone()),
                recent_window_end: Some(task_trend.recent_window_end.clone()),
                previous_window_start: Some(task_trend.previous_window_start.clone()),
                previous_window_end: Some(task_trend.previous_window_end.clone()),
                previous_pomodoros_completed: Some(task_trend.previous_pomodoros_completed),
                previous_focused_seconds: Some(task_trend.previous_focused_seconds),
                previous_focused_minutes: Some(task_trend.previous_focused_minutes),
                delta_focused_seconds: Some(task_trend.delta_focused_seconds),
                delta_focused_minutes: Some(task_trend.delta_focused_minutes),
                ..Self::csv_row_defaults("task_trend")
            });
        }

        for consistency in &self.weekly_consistency {
            rows.push(CsvExportRow {
                week_label: Some(consistency.week_label.clone()),
                year: Some(consistency.year),
                week: Some(consistency.week),
                active_days: Some(u32::from(consistency.active_days)),
                consistency_score_pct: Some(consistency.consistency_score_pct),
                ..Self::csv_row_defaults("weekly_consistency")
            });
        }

        for profile in &self.profile_effectiveness {
            rows.push(CsvExportRow {
                pomodoros_completed: profile.sessions_completed,
                focused_seconds: profile.focused_seconds,
                focused_minutes: profile.focused_minutes,
                profile_name: Some(profile.profile.clone()),
                sessions_completed: Some(profile.sessions_completed),
                active_days: Some(profile.active_days),
                average_focused_minutes_per_session: Some(
                    profile.average_focused_minutes_per_session,
                ),
                focus_share_pct: Some(profile.focus_share_pct),
                ..Self::csv_row_defaults("profile_effectiveness")
            });
        }

        rows
    }
}

fn normalize_task_planner_state(
    labels: Vec<String>,
    selected: Option<String>,
) -> (Vec<String>, Option<String>) {
    let mut normalized_labels = Vec::new();
    let mut seen = BTreeSet::new();
    for label in labels {
        let Some(label) = normalize_task_label(&label) else {
            continue;
        };
        let key = label.to_ascii_lowercase();
        if seen.insert(key) {
            normalized_labels.push(label);
        }
    }

    let normalized_selected = selected
        .and_then(|value| normalize_task_label(&value))
        .map(|value| canonical_task_label(&normalized_labels, &value).unwrap_or(value));
    if let Some(selected_label) = normalized_selected.as_ref() {
        let key = selected_label.to_ascii_lowercase();
        if seen.insert(key) {
            normalized_labels.push(selected_label.clone());
        }
    }

    (normalized_labels, normalized_selected)
}

fn normalize_session_metadata_text(input: &str) -> Option<String> {
    let trimmed = input.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub fn current_day_key() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

fn current_goal_streak(
    completed_days: &BTreeSet<chrono::NaiveDate>,
    today: chrono::NaiveDate,
    current_goal: DailyGoalSnapshot,
    today_stats: DailyStats,
) -> u32 {
    if !current_goal.has_any_target() {
        return 0;
    }

    let mut streak = 0;
    let mut cursor = if current_goal.is_met_by(today_stats) {
        Some(today)
    } else {
        today.pred_opt()
    };

    while let Some(day) = cursor {
        if !completed_days.contains(&day) {
            break;
        }
        streak += 1;
        cursor = day.pred_opt();
    }

    streak
}

fn best_goal_streak(completed_days: &BTreeSet<chrono::NaiveDate>) -> u32 {
    let mut best = 0;
    let mut streak = 0;
    let mut previous_day: Option<chrono::NaiveDate> = None;

    for day in completed_days {
        if previous_day.is_some_and(|previous| previous.succ_opt() == Some(*day)) {
            streak += 1;
        } else {
            streak = 1;
        }

        best = best.max(streak);
        previous_day = Some(*day);
    }

    best
}

fn profile_bucket_for(profile: Option<ProfileId>) -> ProfileBucket {
    match profile {
        Some(ProfileId::Classic) => ProfileBucket::Classic,
        Some(ProfileId::DeepWork) => ProfileBucket::DeepWork,
        Some(ProfileId::Custom) => ProfileBucket::Custom,
        None => ProfileBucket::Unknown,
    }
}

fn daily_has_activity(stats: DailyStats) -> bool {
    stats.focused_seconds > 0 || stats.pomodoros_completed > 0
}

fn percentage_round_nearest(part: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    let rounded = (u128::from(part) * 100 + (u128::from(total) / 2)) / u128::from(total);
    rounded.min(u128::from(u8::MAX)) as u8
}

fn consistency_score_from_active_days(active_days: u8) -> u8 {
    let capped_days = active_days.min(7);
    let rounded = (u32::from(capped_days) * 100 + 3) / 7;
    rounded.min(u32::from(u8::MAX)) as u8
}

fn format_week_label(year: i32, week: u32) -> String {
    format!("{year:04}-W{week:02}")
}

fn write_atomic_bytes(path: &Path, content: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let (tmp_path, mut tmp_file) = create_unique_temp_file(path)?;
    tmp_file.write_all(content)?;
    tmp_file.flush()?;
    drop(tmp_file);

    // Best-effort atomic write: temp file + rename.
    // On Windows, rename cannot replace an existing file; fall back to
    // remove+rename when destination already exists.
    #[cfg(target_os = "windows")]
    {
        match fs::rename(&tmp_path, path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                // Windows std::fs::rename cannot replace an existing file.
                // This fallback has a small data-loss window: if remove_file
                // succeeds and rename then fails, the old destination is gone
                // while the new content remains only at tmp_path.
                fs::remove_file(path)?;
                fs::rename(&tmp_path, path)
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                Err(e)
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        match fs::rename(&tmp_path, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                Err(e)
            }
        }
    }
}

fn create_unique_temp_file(path: &Path) -> io::Result<(PathBuf, fs::File)> {
    const MAX_ATTEMPTS: usize = 32;

    for _ in 0..MAX_ATTEMPTS {
        let candidate = create_unique_temp_path(path);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "failed to allocate unique temporary export path",
    ))
}

fn create_unique_temp_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let target_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("focustime-export");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let seq = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".{target_name}.{pid}.{nanos}.{seq}.tmp"))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year.saturating_add(1), 1)
    } else {
        (year, month.saturating_add(1))
    };
    let next_month_start = chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("validated month rollover should produce next month start");
    next_month_start
        .pred_opt()
        .expect("month start should have a predecessor")
        .day()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn carry_over_goal_target_returns_base_when_disabled() {
        let base = DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 2,
        };
        let carried = carry_over_goal_target(base, false, Some((base, 0, 0)));
        assert_eq!(carried, base);
    }

    #[test]
    fn carry_over_goal_target_adds_previous_period_deficit() {
        let base = DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 2,
        };
        let previous_target = DailyGoalSnapshot {
            minutes: 50,
            pomodoros: 3,
        };
        let carried = carry_over_goal_target(base, true, Some((previous_target, 30, 1)));
        assert_eq!(
            carried,
            DailyGoalSnapshot {
                minutes: 80,
                pomodoros: 4,
            }
        );
    }

    #[test]
    fn carry_over_goal_target_keeps_disabled_metrics_off() {
        let base = DailyGoalSnapshot {
            minutes: 0,
            pomodoros: 2,
        };
        let previous_target = DailyGoalSnapshot {
            minutes: 120,
            pomodoros: 5,
        };
        let carried = carry_over_goal_target(base, true, Some((previous_target, 0, 1)));
        assert_eq!(
            carried,
            DailyGoalSnapshot {
                minutes: 0,
                pomodoros: 6,
            }
        );
    }

    #[test]
    fn recording_updates_session_and_daily_totals() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };

        stats.record_focus_elapsed("2026-04-09", 125, goal);
        stats.record_completed_pomodoro("2026-04-09", goal);

        let session = stats.session();
        assert_eq!(session.pomodoros_completed, 1);
        assert_eq!(session.focused_seconds, 125);
        assert_eq!(session.focused_minutes(), 2);

        let day = stats.daily_for("2026-04-09");
        assert_eq!(day.pomodoros_completed, 1);
        assert_eq!(day.focused_seconds, 125);
        assert_eq!(day.focused_minutes(), 2);
        assert_eq!(day.goal, Some(goal));
    }

    #[test]
    fn task_planner_state_normalizes_and_deduplicates_labels() {
        let mut stats = FocusStats::default();
        let changed = stats.update_task_planner_state(
            vec![
                "  Docs  ".to_string(),
                "docs".to_string(),
                "".to_string(),
                "Bugfix".to_string(),
            ],
            Some("  docs ".to_string()),
        );
        assert!(changed);

        let (labels, selected) = stats.task_planner_state();
        assert_eq!(labels, vec!["Docs".to_string(), "Bugfix".to_string()]);
        assert_eq!(selected, Some("Docs".to_string()));
    }

    #[test]
    fn recent_task_labels_returns_newest_first_unique_labels() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        stats.record_completed_pomodoro_with_task("2026-04-07", goal, Some("Docs"), 25 * 60, None);
        stats.record_completed_pomodoro_with_task(
            "2026-04-08",
            goal,
            Some("Bugfix"),
            25 * 60,
            None,
        );
        stats.record_completed_pomodoro_with_task("2026-04-09", goal, Some("docs"), 25 * 60, None);
        stats.record_completed_pomodoro_with_task(
            "2026-04-10",
            goal,
            Some("Planning"),
            25 * 60,
            None,
        );

        let recent = stats.recent_task_labels(3);
        assert_eq!(
            recent,
            vec![
                "Planning".to_string(),
                "docs".to_string(),
                "Bugfix".to_string()
            ]
        );
    }

    #[test]
    fn recent_task_labels_respects_zero_limit() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        stats.record_completed_pomodoro_with_task("2026-04-10", goal, Some("Docs"), 25 * 60, None);

        assert!(stats.recent_task_labels(0).is_empty());
    }

    #[test]
    fn task_totals_aggregate_focus_sessions_by_label() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        stats.record_completed_pomodoro_with_task(
            "2026-04-01",
            goal,
            Some("Project A"),
            30 * 60,
            Some(ProfileId::Classic),
        );
        stats.record_completed_pomodoro_with_task(
            "2026-04-02",
            goal,
            Some("project a"),
            20 * 60,
            Some(ProfileId::Classic),
        );
        stats.record_completed_pomodoro_with_task(
            "2026-04-03",
            goal,
            Some("Project B"),
            40 * 60,
            Some(ProfileId::DeepWork),
        );

        let totals = stats.task_totals(10);
        assert_eq!(totals.len(), 2);
        assert_eq!(totals[0].task_label, "Project A");
        assert_eq!(totals[0].pomodoros_completed, 2);
        assert_eq!(totals[0].focused_minutes(), 50);
        assert_eq!(totals[1].task_label, "Project B");
        assert_eq!(totals[1].pomodoros_completed, 1);
        assert_eq!(totals[1].focused_minutes(), 40);
    }

    #[test]
    fn recent_task_trends_compare_last_seven_days_vs_previous_window() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        let today = chrono::Local::now().date_naive();
        let day_prev_a_1 = today
            .checked_sub_signed(chrono::Duration::days(13))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        let day_prev_a_2 = today
            .checked_sub_signed(chrono::Duration::days(10))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        let day_recent_a_1 = today
            .checked_sub_signed(chrono::Duration::days(3))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        let day_recent_a_2 = today.format("%Y-%m-%d").to_string();
        let day_prev_b = today
            .checked_sub_signed(chrono::Duration::days(11))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        let day_recent_b = today
            .checked_sub_signed(chrono::Duration::days(2))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();

        stats.record_completed_pomodoro_with_task(
            &day_prev_a_1,
            goal,
            Some("Project A"),
            10 * 60,
            None,
        );
        stats.record_completed_pomodoro_with_task(
            &day_prev_a_2,
            goal,
            Some("Project A"),
            20 * 60,
            None,
        );
        stats.record_completed_pomodoro_with_task(
            &day_recent_a_1,
            goal,
            Some("Project A"),
            30 * 60,
            None,
        );
        stats.record_completed_pomodoro_with_task(
            &day_recent_a_2,
            goal,
            Some("Project A"),
            40 * 60,
            None,
        );

        stats.record_completed_pomodoro_with_task(
            &day_prev_b,
            goal,
            Some("Project B"),
            30 * 60,
            None,
        );
        stats.record_completed_pomodoro_with_task(
            &day_recent_b,
            goal,
            Some("Project B"),
            10 * 60,
            None,
        );

        let trends = stats.recent_task_trends(10);
        assert_eq!(trends.len(), 2);
        assert_eq!(trends[0].task_label, "Project A");
        assert_eq!(trends[0].recent_pomodoros_completed, 2);
        assert_eq!(trends[0].previous_pomodoros_completed, 2);
        assert_eq!(trends[0].recent_focused_minutes(), 70);
        assert_eq!(trends[0].previous_focused_minutes(), 30);
        assert_eq!(trends[0].delta_focused_minutes(), 40);

        assert_eq!(trends[1].task_label, "Project B");
        assert_eq!(trends[1].recent_focused_minutes(), 10);
        assert_eq!(trends[1].previous_focused_minutes(), 30);
        assert_eq!(trends[1].delta_focused_minutes(), -20);
    }

    #[test]
    fn recent_task_trends_handles_sparse_data() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        let today_key = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        stats.record_completed_pomodoro_with_task(
            &today_key,
            goal,
            Some("Project A"),
            25 * 60,
            None,
        );

        let trends = stats.recent_task_trends(5);
        assert_eq!(trends.len(), 1);
        assert_eq!(trends[0].task_label, "Project A");
        assert_eq!(trends[0].previous_focused_minutes(), 0);
        assert_eq!(trends[0].delta_focused_minutes(), 25);
    }

    #[test]
    fn recent_daily_is_sorted_newest_first() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 30,
            pomodoros: 1,
        };
        stats.record_focus_elapsed("2026-04-08", 60, goal);
        stats.record_focus_elapsed("2026-04-09", 120, goal);

        let recent = stats.recent_daily(2);
        assert_eq!(recent[0].0, "2026-04-09");
        assert_eq!(recent[1].0, "2026-04-08");
    }

    #[test]
    fn recent_weekly_aggregates_days_in_same_iso_week() {
        let mut stats = FocusStats::default();
        stats.insert_daily_for_tests(
            "2026-04-06",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 30 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-08",
            DailyStats {
                pomodoros_completed: 2,
                focused_seconds: 45 * 60,
                goal: None,
            },
        );

        let recent = stats.recent_weekly(1);
        let iso_week = chrono::NaiveDate::from_ymd_opt(2026, 4, 6)
            .unwrap()
            .iso_week();

        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].year, iso_week.year());
        assert_eq!(recent[0].week, iso_week.week());
        assert_eq!(recent[0].pomodoros_completed, 3);
        assert_eq!(recent[0].focused_minutes(), 75);
    }

    #[test]
    fn weekly_for_day_aggregates_selected_iso_week_only() {
        let mut stats = FocusStats::default();
        stats.insert_daily_for_tests(
            "2026-04-06",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 30 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-08",
            DailyStats {
                pomodoros_completed: 2,
                focused_seconds: 45 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-14",
            DailyStats {
                pomodoros_completed: 3,
                focused_seconds: 90 * 60,
                goal: None,
            },
        );

        let week = stats.weekly_for_day(chrono::NaiveDate::from_ymd_opt(2026, 4, 7).unwrap());
        let iso_week = chrono::NaiveDate::from_ymd_opt(2026, 4, 7)
            .unwrap()
            .iso_week();
        assert_eq!(week.year, iso_week.year());
        assert_eq!(week.week, iso_week.week());
        assert_eq!(week.pomodoros_completed, 3);
        assert_eq!(week.focused_minutes(), 75);
    }

    #[test]
    fn recent_weekly_is_sorted_newest_first_across_iso_year_boundaries() {
        let mut stats = FocusStats::default();
        stats.insert_daily_for_tests(
            "2020-12-31",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 30 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2021-01-01",
            DailyStats {
                pomodoros_completed: 2,
                focused_seconds: 60 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2021-01-04",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 15 * 60,
                goal: None,
            },
        );

        let recent = stats.recent_weekly(2);

        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].year, 2021);
        assert_eq!(recent[0].week, 1);
        assert_eq!(recent[0].pomodoros_completed, 1);
        assert_eq!(recent[0].focused_minutes(), 15);
        assert_eq!(recent[1].year, 2020);
        assert_eq!(recent[1].week, 53);
        assert_eq!(recent[1].pomodoros_completed, 3);
        assert_eq!(recent[1].focused_minutes(), 90);
    }

    #[test]
    fn recent_weekly_consistency_scores_active_days() {
        let mut stats = FocusStats::default();
        stats.insert_daily_for_tests(
            "2026-04-06",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 25 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-07",
            DailyStats {
                pomodoros_completed: 0,
                focused_seconds: 15 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-08",
            DailyStats {
                pomodoros_completed: 0,
                focused_seconds: 0,
                goal: None,
            },
        );

        let consistency = stats.recent_weekly_consistency(1);
        let iso_week = chrono::NaiveDate::from_ymd_opt(2026, 4, 6)
            .unwrap()
            .iso_week();
        assert_eq!(consistency.len(), 1);
        assert_eq!(consistency[0].year, iso_week.year());
        assert_eq!(consistency[0].week, iso_week.week());
        assert_eq!(consistency[0].active_days, 2);
        assert_eq!(consistency[0].consistency_score_pct, 29);
    }

    #[test]
    fn persisted_stats_round_trip_preserves_daily_history() {
        let mut original = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        original.record_focus_elapsed("2026-04-09", 300, goal);
        original.record_completed_pomodoro("2026-04-09", goal);

        let persisted = original.to_persisted();
        let toml_str = toml::to_string_pretty(&persisted).unwrap();
        let restored = FocusStats::try_from_toml(&toml_str).unwrap();

        // Session stats are intentionally runtime-only and reset on startup.
        assert_eq!(restored.session(), SessionStats::default());
        let day = restored.daily_for("2026-04-09");
        assert_eq!(day.pomodoros_completed, 1);
        assert_eq!(day.focused_seconds, 300);
        assert_eq!(day.goal, Some(goal));
    }

    #[test]
    fn persisted_stats_round_trip_preserves_break_glass_overrides() {
        let mut original = FocusStats::default();
        original.record_break_glass_override_event(
            "2026-04-09",
            1_711_000_000,
            Some("Project A"),
            300,
        );

        let persisted = original.to_persisted();
        let toml_str = toml::to_string_pretty(&persisted).unwrap();
        let restored = FocusStats::try_from_toml(&toml_str).unwrap();
        let recent = restored.recent_break_glass_overrides(1);

        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].date, "2026-04-09");
        assert_eq!(recent[0].timestamp_epoch_secs, 1_711_000_000);
        assert_eq!(recent[0].task_label.as_deref(), Some("Project A"));
        assert_eq!(recent[0].duration_seconds, 300);
    }

    #[test]
    fn persisted_stats_round_trip_preserves_focus_session_profile() {
        let mut original = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        original.record_focus_elapsed("2026-04-09", 25 * 60, goal);
        original.record_completed_pomodoro_with_task(
            "2026-04-09",
            goal,
            Some("Project A"),
            25 * 60,
            Some(ProfileId::DeepWork),
        );

        let persisted = original.to_persisted();
        let toml_str = toml::to_string_pretty(&persisted).unwrap();
        let restored = FocusStats::try_from_toml(&toml_str).unwrap();
        let profile_totals = restored.profile_totals();

        assert_eq!(profile_totals.len(), 1);
        assert_eq!(profile_totals[0].profile, ProfileBucket::DeepWork);
        assert_eq!(profile_totals[0].pomodoros_completed, 1);
        assert_eq!(profile_totals[0].focused_minutes(), 25);
    }

    #[test]
    fn legacy_focus_sessions_default_metadata_from_task_label() {
        let legacy_toml = r#"
            [[focus_sessions]]
            date = "2026-04-09"
            task_label = "Project A"
            focused_seconds = 1500
        "#;
        let restored = FocusStats::try_from_toml(legacy_toml).unwrap();

        let export = restored.export_data();
        assert_eq!(export.sessions.len(), 1);
        assert_eq!(export.sessions[0].task_label, "Project A");
        assert_eq!(export.sessions[0].focus_intention, "Project A");
        assert_eq!(export.sessions[0].task_note, "Project A");
    }

    #[test]
    fn session_export_preserves_persisted_metadata_fields() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        stats.record_focus_elapsed("2026-04-09", 30 * 60, goal);
        stats.record_completed_pomodoro_with_metadata(
            "2026-04-09",
            goal,
            FocusSessionMetadata {
                task_label: Some("Project A"),
                focus_intention: Some("Write release notes"),
                task_note: Some("Capture blockers for follow-up"),
            },
            30 * 60,
            Some(ProfileId::Classic),
        );

        let export = stats.export_data();
        assert_eq!(export.sessions.len(), 1);
        assert_eq!(export.sessions[0].task_label, "Project A");
        assert_eq!(export.sessions[0].focus_intention, "Write release notes");
        assert_eq!(
            export.sessions[0].task_note,
            "Capture blockers for follow-up"
        );
    }

    #[test]
    fn invalid_toml_returns_parse_error() {
        assert!(FocusStats::try_from_toml("this is not valid toml").is_err());
    }

    #[test]
    fn current_day_key_uses_iso_date_format() {
        let key = current_day_key();
        assert_eq!(key.len(), 10);
        assert_eq!(&key[4..5], "-");
        assert_eq!(&key[7..8], "-");
    }

    #[test]
    fn goal_streak_counts_consecutive_completed_days() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 30,
            pomodoros: 1,
        };

        for day in ["2026-04-07", "2026-04-08", "2026-04-09"] {
            stats.record_focus_elapsed(day, 30 * 60, goal);
            stats.record_completed_pomodoro(day, goal);
        }

        let streak = stats.goal_streak(
            chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap(),
            goal,
            stats.daily_for("2026-04-09"),
        );

        assert_eq!(streak.current, 3);
        assert_eq!(streak.best, 3);
    }

    #[test]
    fn goal_streak_keeps_running_until_today_is_missed() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 1,
        };

        for day in ["2026-04-07", "2026-04-08"] {
            stats.record_focus_elapsed(day, 60 * 60, goal);
            stats.record_completed_pomodoro(day, goal);
        }

        let streak = stats.goal_streak(
            chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap(),
            goal,
            DailyStats::default(),
        );

        assert_eq!(streak.current, 2);
        assert_eq!(streak.best, 2);
    }

    #[test]
    fn goal_streak_uses_current_goal_as_legacy_fallback() {
        let mut stats = FocusStats::default();
        {
            let day = stats.daily.entry("2026-04-09".to_string()).or_default();
            day.focused_seconds = 45 * 60;
            day.pomodoros_completed = 2;
        }

        let goal = DailyGoalSnapshot {
            minutes: 30,
            pomodoros: 1,
        };
        let today_stats = stats.daily_for("2026-04-09");
        let streak = stats.goal_streak(
            chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap(),
            goal,
            today_stats,
        );

        assert_eq!(streak.current, 1);
        assert_eq!(streak.best, 1);
    }

    #[test]
    fn recent_monthly_aggregates_and_orders_newest_first() {
        let mut stats = FocusStats::default();
        stats.insert_daily_for_tests(
            "2026-03-31",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 15 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-01",
            DailyStats {
                pomodoros_completed: 2,
                focused_seconds: 60 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-08",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 30 * 60,
                goal: None,
            },
        );

        let recent = stats.recent_monthly(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].year, 2026);
        assert_eq!(recent[0].month, 4);
        assert_eq!(recent[0].pomodoros_completed, 3);
        assert_eq!(recent[0].focused_minutes(), 90);
        assert_eq!(recent[1].year, 2026);
        assert_eq!(recent[1].month, 3);
        assert_eq!(recent[1].pomodoros_completed, 1);
        assert_eq!(recent[1].focused_minutes(), 15);
    }

    #[test]
    fn monthly_for_day_aggregates_selected_calendar_month_only() {
        let mut stats = FocusStats::default();
        stats.insert_daily_for_tests(
            "2026-04-06",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 30 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-08",
            DailyStats {
                pomodoros_completed: 2,
                focused_seconds: 45 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-05-01",
            DailyStats {
                pomodoros_completed: 4,
                focused_seconds: 120 * 60,
                goal: None,
            },
        );

        let month = stats.monthly_for_day(chrono::NaiveDate::from_ymd_opt(2026, 4, 10).unwrap());
        assert_eq!(month.year, 2026);
        assert_eq!(month.month, 4);
        assert_eq!(month.pomodoros_completed, 3);
        assert_eq!(month.focused_minutes(), 75);
    }

    #[test]
    fn latest_monthly_heatmap_uses_latest_recorded_month_data() {
        let mut stats = FocusStats::default();
        stats.insert_daily_for_tests(
            "2026-03-31",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 15 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-01",
            DailyStats {
                pomodoros_completed: 2,
                focused_seconds: 60 * 60,
                goal: None,
            },
        );
        stats.insert_daily_for_tests(
            "2026-04-03",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 45 * 60,
                goal: None,
            },
        );

        let heatmap = stats.latest_monthly_heatmap();
        assert_eq!(heatmap.year, 2026);
        assert_eq!(heatmap.month, 4);
        assert_eq!(heatmap.days_in_month, 30);
        assert_eq!(heatmap.max_focused_minutes, 60);
        assert_eq!(heatmap.days[0].day, 1);
        assert_eq!(heatmap.days[0].pomodoros_completed, 2);
        assert_eq!(heatmap.days[0].focused_minutes(), 60);
        assert_eq!(heatmap.days[1].day, 2);
        assert_eq!(heatmap.days[1].focused_minutes(), 0);
        assert_eq!(heatmap.days[2].day, 3);
        assert_eq!(heatmap.days[2].focused_minutes(), 45);
    }

    #[test]
    fn profile_totals_groups_by_profile_and_unknown() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        stats.record_completed_pomodoro_with_task(
            "2026-04-09",
            goal,
            Some("Project A"),
            50 * 60,
            Some(ProfileId::DeepWork),
        );
        stats.record_completed_pomodoro_with_task(
            "2026-04-10",
            goal,
            Some("Project B"),
            25 * 60,
            Some(ProfileId::Classic),
        );
        stats.record_completed_pomodoro_with_task(
            "2026-04-11",
            goal,
            Some("Project C"),
            40 * 60,
            None,
        );

        let totals = stats.profile_totals();
        assert_eq!(totals.len(), 3);
        let deep_work = totals
            .iter()
            .find(|entry| entry.profile == ProfileBucket::DeepWork)
            .copied()
            .unwrap();
        let classic = totals
            .iter()
            .find(|entry| entry.profile == ProfileBucket::Classic)
            .copied()
            .unwrap();
        let unknown = totals
            .iter()
            .find(|entry| entry.profile == ProfileBucket::Unknown)
            .copied()
            .unwrap();

        assert_eq!(deep_work.pomodoros_completed, 1);
        assert_eq!(deep_work.focused_minutes(), 50);
        assert_eq!(classic.pomodoros_completed, 1);
        assert_eq!(classic.focused_minutes(), 25);
        assert_eq!(unknown.pomodoros_completed, 1);
        assert_eq!(unknown.focused_minutes(), 40);
    }

    #[test]
    fn profile_effectiveness_reports_share_and_average_minutes() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        stats.record_completed_pomodoro_with_task(
            "2026-04-09",
            goal,
            Some("Project A"),
            50 * 60,
            Some(ProfileId::DeepWork),
        );
        stats.record_completed_pomodoro_with_task(
            "2026-04-10",
            goal,
            Some("Project D"),
            40 * 60,
            Some(ProfileId::DeepWork),
        );
        stats.record_completed_pomodoro_with_task(
            "2026-04-11",
            goal,
            Some("Project B"),
            25 * 60,
            Some(ProfileId::Classic),
        );
        stats.record_completed_pomodoro_with_task(
            "2026-04-12",
            goal,
            Some("Project C"),
            40 * 60,
            None,
        );

        let effectiveness = stats.profile_effectiveness();
        assert_eq!(effectiveness.len(), 3);

        let deep_work = effectiveness
            .iter()
            .find(|entry| entry.profile == ProfileBucket::DeepWork)
            .copied()
            .unwrap();
        let classic = effectiveness
            .iter()
            .find(|entry| entry.profile == ProfileBucket::Classic)
            .copied()
            .unwrap();
        let unknown = effectiveness
            .iter()
            .find(|entry| entry.profile == ProfileBucket::Unknown)
            .copied()
            .unwrap();

        assert_eq!(deep_work.sessions_completed, 2);
        assert_eq!(deep_work.active_days, 2);
        assert_eq!(deep_work.average_focused_minutes_per_session(), 45);
        assert_eq!(deep_work.focus_share_pct, 58);

        assert_eq!(classic.sessions_completed, 1);
        assert_eq!(classic.average_focused_minutes_per_session(), 25);
        assert_eq!(classic.focus_share_pct, 16);

        assert_eq!(unknown.sessions_completed, 1);
        assert_eq!(unknown.average_focused_minutes_per_session(), 40);
        assert_eq!(unknown.focus_share_pct, 26);
    }

    #[test]
    fn export_to_dir_writes_daily_and_weekly_json_and_csv() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        let today = chrono::Local::now().date_naive();
        let labeled_day = today.format("%Y-%m-%d").to_string();
        let other_day_date = [today.pred_opt(), today.succ_opt()]
            .into_iter()
            .flatten()
            .find(|candidate| candidate.iso_week() == today.iso_week())
            .unwrap_or(today);
        let other_day = other_day_date.format("%Y-%m-%d").to_string();
        let recent_window_start = today
            .checked_sub_signed(chrono::Duration::days(6))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        let recent_window_end = today.format("%Y-%m-%d").to_string();
        let previous_window_end = today
            .checked_sub_signed(chrono::Duration::days(7))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        let previous_window_start = today
            .checked_sub_signed(chrono::Duration::days(13))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();

        stats.record_focus_elapsed(&labeled_day, 30 * 60, goal);
        stats.record_completed_pomodoro_with_task(
            &labeled_day,
            goal,
            Some("Project A"),
            30 * 60,
            Some(ProfileId::Classic),
        );
        stats.record_break_glass_override_event(
            &labeled_day,
            1_711_000_000,
            Some("Project A"),
            300,
        );
        stats.record_focus_elapsed(&other_day, 45 * 60, goal);
        stats.record_completed_pomodoro(&other_day, goal);

        let export_dir = unique_temp_dir("stats-export");
        let exported = stats.export_to_dir(&export_dir).unwrap();

        assert_eq!(
            exported.json_path.file_name().unwrap(),
            JSON_EXPORT_FILE_NAME
        );
        assert_eq!(exported.csv_path.file_name().unwrap(), CSV_EXPORT_FILE_NAME);

        let json = fs::read_to_string(&exported.json_path).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(json_value["schema_version"], EXPORT_SCHEMA_VERSION);
        let daily = json_value["daily"].as_array().unwrap();
        let weekly = json_value["weekly"].as_array().unwrap();
        let sessions = json_value["sessions"].as_array().unwrap();
        let overrides = json_value["overrides"].as_array().unwrap();
        let task_totals = json_value["task_totals"].as_array().unwrap();
        let task_trends = json_value["task_trends"].as_array().unwrap();
        let weekly_consistency = json_value["weekly_consistency"].as_array().unwrap();
        let profile_effectiveness = json_value["profile_effectiveness"].as_array().unwrap();
        assert_eq!(daily.len(), 2);
        assert!(!weekly.is_empty());
        assert_eq!(sessions.len(), 1);
        assert_eq!(overrides.len(), 1);
        assert_eq!(task_totals.len(), 1);
        assert_eq!(task_trends.len(), 1);
        assert!(!weekly_consistency.is_empty());
        assert_eq!(profile_effectiveness.len(), 1);
        assert!(
            daily
                .iter()
                .any(|entry| entry["date"] == labeled_day && entry["goal_met"] == true)
        );
        assert!(weekly.iter().any(|entry| entry["focused_minutes"] == 75));
        assert_eq!(sessions[0]["task_label"], "Project A");
        assert_eq!(sessions[0]["focus_intention"], "Project A");
        assert_eq!(sessions[0]["task_note"], "Project A");
        assert_eq!(sessions[0]["focused_minutes"], 30);
        assert_eq!(sessions[0]["profile"], "classic");
        assert_eq!(overrides[0]["duration_seconds"], 300);
        assert_eq!(overrides[0]["task_label"], "Project A");
        assert_eq!(task_totals[0]["task_label"], "Project A");
        assert_eq!(task_totals[0]["focused_minutes"], 30);
        assert_eq!(task_trends[0]["task_label"], "Project A");
        assert_eq!(task_trends[0]["recent_window_start"], recent_window_start);
        assert_eq!(task_trends[0]["recent_window_end"], recent_window_end);
        assert_eq!(
            task_trends[0]["previous_window_start"],
            previous_window_start
        );
        assert_eq!(task_trends[0]["previous_window_end"], previous_window_end);
        assert_eq!(task_trends[0]["delta_focused_minutes"], 30);
        assert!(
            weekly_consistency
                .iter()
                .any(|entry| entry["consistency_score_pct"].as_u64().unwrap_or(0) > 0)
        );
        assert_eq!(profile_effectiveness[0]["profile"], "Classic");
        assert_eq!(
            profile_effectiveness[0]["average_focused_minutes_per_session"],
            30
        );
        assert_eq!(profile_effectiveness[0]["focus_share_pct"], 100);

        let csv = fs::read_to_string(&exported.csv_path).unwrap();
        assert!(csv.contains("schema_version,record_type,date,week_label,year,week,pomodoros_completed,focused_seconds,focused_minutes,goal_minutes,goal_pomodoros,goal_met,task_label,break_glass_timestamp_epoch_secs,break_glass_duration_seconds,focus_intention,task_note,recent_window_start,recent_window_end,previous_window_start,previous_window_end,previous_pomodoros_completed,previous_focused_seconds,previous_focused_minutes,delta_focused_seconds,delta_focused_minutes,profile_name,sessions_completed,active_days,consistency_score_pct,average_focused_minutes_per_session,focus_share_pct"));
        assert!(csv.contains(&format!(
            "3,daily,{labeled_day},,,,1,1800,30,25,1,true,,,,,"
        )));
        assert!(csv.contains("3,weekly,,"));
        assert!(csv.contains(&format!(
            "3,focus_session,{labeled_day},,,,1,1800,30,,,,Project A,,,Project A,Project A"
        )));
        assert!(csv.contains(&format!(
            "3,break_glass_override,{labeled_day},,,,0,0,0,,,,Project A,1711000000,300,,"
        )));
        assert!(csv.contains("3,task_summary,,,,,1,1800,30,,,,Project A"));
        assert!(csv.contains("3,task_trend,,,,,1,1800,30,,,,Project A"));
        assert!(csv.contains("3,weekly_consistency,"));
        assert!(csv.contains("3,profile_effectiveness,,,,,1,1800,30"));
        assert!(csv.contains("Classic,1,1,,30,100"));

        fs::remove_dir_all(export_dir).unwrap();
    }

    #[test]
    fn export_to_dir_returns_error_when_target_is_not_directory() {
        let stats = FocusStats::default();
        let export_root = unique_temp_dir("stats-export-error");
        let not_a_directory = export_root.join("already-a-file");
        fs::write(&not_a_directory, "occupied").unwrap();

        let result = stats.export_to_dir(&not_a_directory);

        assert!(result.is_err());

        fs::remove_dir_all(export_root).unwrap();
    }

    #[test]
    fn create_unique_temp_path_changes_between_calls() {
        let target = Path::new("focustime-stats.json");
        let first = create_unique_temp_path(target);
        let second = create_unique_temp_path(target);

        assert_ne!(first, second);
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("focustime-{label}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
