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
        let (task_labels, selected_task_label, task_label_favorites, task_label_archived) =
            normalize_task_planner_state(
                persisted.task_labels,
                persisted.selected_task_label,
                persisted.task_label_favorites,
                persisted.task_label_archived,
            );
        let task_goal_targets = normalize_task_goal_targets(persisted.task_goal_targets);
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
        let mut session_interruptions = Vec::new();
        for event in persisted.session_interruptions {
            session_interruptions.push(SessionInterruptionEvent {
                timestamp_epoch_secs: event.timestamp_epoch_secs,
                date: event.date.trim().to_string(),
                reason: event.reason,
                task_label: event
                    .task_label
                    .and_then(|label| normalize_task_label(&label)),
                focus_intention: event
                    .focus_intention
                    .and_then(|value| normalize_session_metadata_text(&value)),
                task_note: event
                    .task_note
                    .and_then(|value| normalize_session_metadata_text(&value)),
                remaining_secs: event.remaining_secs,
                profile: event.profile,
            });
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
            weekly_goal_snapshots: persisted.weekly_goal_snapshots,
            monthly_goal_snapshots: persisted.monthly_goal_snapshots,
            task_labels,
            selected_task_label,
            task_label_favorites,
            task_label_archived,
            focus_sessions,
            session_interruptions,
            break_glass_overrides,
            task_goal_targets,
        }
    }

    fn to_persisted(&self) -> PersistedStats {
        PersistedStats {
            daily: self.daily.clone(),
            weekly_goal_snapshots: self.weekly_goal_snapshots.clone(),
            monthly_goal_snapshots: self.monthly_goal_snapshots.clone(),
            task_labels: self.task_labels.clone(),
            selected_task_label: self.selected_task_label.clone(),
            task_label_favorites: planner_state_labels_for_keys(
                &self.task_label_favorites,
                &self.task_labels,
            ),
            task_label_archived: planner_state_labels_for_keys(
                &self.task_label_archived,
                &self.task_labels,
            ),
            focus_sessions: self.focus_sessions.clone(),
            session_interruptions: self.session_interruptions.clone(),
            break_glass_overrides: self.break_glass_overrides.clone(),
            task_goal_targets: self.task_goal_targets.clone(),
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

    pub fn weekly_goal_snapshot_for_day(
        &self,
        day: chrono::NaiveDate,
    ) -> Option<DailyGoalSnapshot> {
        let key = week_key_for_day(day);
        self.weekly_goal_snapshots.get(&key).copied()
    }

    pub fn monthly_goal_snapshot_for_day(
        &self,
        day: chrono::NaiveDate,
    ) -> Option<DailyGoalSnapshot> {
        let key = month_key_for_day(day);
        self.monthly_goal_snapshots.get(&key).copied()
    }

    pub fn weekly_focus_score_for_day(&self, day: chrono::NaiveDate) -> WeeklyFocusScore {
        let iso_week = day.iso_week();
        let year = iso_week.year();
        let week = iso_week.week();
        let week_label = format_week_label(year, week);
        let active_days = self
            .weekly_active_days()
            .get(&(year, week))
            .copied()
            .unwrap_or(0);
        let consistency_score_pct = consistency_score_from_active_days(active_days);
        let totals = self.weekly_for_day(day);
        let completion_score_pct = self
            .weekly_goal_snapshot_for_day(day)
            .and_then(|goal| weekly_completion_score_pct(goal, totals));
        let focus_score_pct = completion_score_pct
            .map(|completion| average_two_percentages(consistency_score_pct, completion));

        WeeklyFocusScore {
            year,
            week,
            week_label,
            active_days,
            consistency_score_pct,
            completion_score_pct,
            focus_score_pct,
        }
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

    pub fn recent_weekly_focus_scores(&self, limit: usize) -> Vec<WeeklyFocusScore> {
        let mut weekly = self.weekly_focus_score_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
    }

    pub fn latest_weekly_focus_score(&self) -> Option<WeeklyFocusScore> {
        self.recent_weekly_focus_scores(1).into_iter().next()
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
            interruptions: self.export_session_interruption_rows(),
            overrides: self.export_break_glass_override_rows(),
            task_totals: self.export_task_totals_rows(),
            task_trends: self.export_task_trend_rows(),
            weekly_consistency: self.export_weekly_consistency_rows(),
            focus_scores: self.export_focus_score_rows(),
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

    fn export_session_interruption_rows(&self) -> Vec<SessionInterruptionExportRow> {
        self.session_interruptions
            .iter()
            .map(|event| SessionInterruptionExportRow {
                timestamp_epoch_secs: event.timestamp_epoch_secs,
                date: event.date.clone(),
                reason: event.reason,
                task_label: event.task_label.clone(),
                focus_intention: event.focus_intention.clone(),
                task_note: event.task_note.clone(),
                remaining_secs: event.remaining_secs,
                profile: event.profile,
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

    fn export_focus_score_rows(&self) -> Vec<FocusScoreExportRow> {
        self.weekly_focus_score_stats()
            .into_iter()
            .map(|entry| FocusScoreExportRow {
                year: entry.year,
                week: entry.week,
                week_label: entry.week_label,
                active_days: entry.active_days,
                consistency_score_pct: entry.consistency_score_pct,
                completion_score_pct: entry.completion_score_pct,
                focus_score_pct: entry.focus_score_pct,
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
        self.weekly_active_days()
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

    fn weekly_focus_score_stats(&self) -> Vec<WeeklyFocusScore> {
        let consistency_by_key: BTreeMap<(i32, u32), WeeklyConsistency> = self
            .weekly_consistency_stats()
            .into_iter()
            .map(|consistency| ((consistency.year, consistency.week), consistency))
            .collect();
        let weekly_totals_by_key: BTreeMap<(i32, u32), WeeklyStats> = self
            .weekly_stats()
            .into_iter()
            .map(|stats| ((stats.year, stats.week), stats))
            .collect();
        let mut all_week_keys: BTreeSet<(i32, u32)> = consistency_by_key.keys().copied().collect();
        for week_label in self.weekly_goal_snapshots.keys() {
            if let Some(week_key) = parse_week_label(week_label) {
                all_week_keys.insert(week_key);
            }
        }

        all_week_keys
            .into_iter()
            .map(|(year, week)| {
                let consistency = consistency_by_key.get(&(year, week));
                let week_label = format_week_label(year, week);
                let active_days = consistency.map_or(0, |entry| entry.active_days);
                let consistency_score_pct =
                    consistency.map_or(0, |entry| entry.consistency_score_pct);
                let totals =
                    weekly_totals_by_key
                        .get(&(year, week))
                        .copied()
                        .unwrap_or(WeeklyStats {
                            year,
                            week,
                            ..WeeklyStats::default()
                        });
                let completion_score_pct = self
                    .weekly_goal_snapshots
                    .get(&week_label)
                    .copied()
                    .and_then(|goal| weekly_completion_score_pct(goal, totals));
                let focus_score_pct = completion_score_pct
                    .map(|completion| average_two_percentages(consistency_score_pct, completion));
                WeeklyFocusScore {
                    year,
                    week,
                    week_label,
                    active_days,
                    consistency_score_pct,
                    completion_score_pct,
                    focus_score_pct,
                }
            })
            .collect()
    }

    fn weekly_active_days(&self) -> BTreeMap<(i32, u32), u8> {
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
            interruption_timestamp_epoch_secs: None,
            interruption_reason: None,
            interruption_remaining_secs: None,
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
            completion_score_pct: None,
            focus_score_pct: None,
            average_focused_minutes_per_session: None,
            focus_share_pct: None,
        }
    }

    fn csv_rows(&self) -> Vec<CsvExportRow> {
        let mut rows = Vec::with_capacity(
            self.daily.len()
                + self.weekly.len()
                + self.sessions.len()
                + self.interruptions.len()
                + self.overrides.len()
                + self.task_totals.len()
                + self.task_trends.len()
                + self.weekly_consistency.len()
                + self.focus_scores.len()
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

        for interruption in &self.interruptions {
            rows.push(CsvExportRow {
                date: Some(interruption.date.clone()),
                task_label: interruption.task_label.clone(),
                interruption_timestamp_epoch_secs: Some(interruption.timestamp_epoch_secs),
                interruption_reason: Some(interruption.reason),
                interruption_remaining_secs: Some(interruption.remaining_secs),
                focus_intention: interruption.focus_intention.clone(),
                task_note: interruption.task_note.clone(),
                ..Self::csv_row_defaults("session_interruption")
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

        for focus_score in &self.focus_scores {
            rows.push(CsvExportRow {
                week_label: Some(focus_score.week_label.clone()),
                year: Some(focus_score.year),
                week: Some(focus_score.week),
                active_days: Some(u32::from(focus_score.active_days)),
                consistency_score_pct: Some(focus_score.consistency_score_pct),
                completion_score_pct: focus_score.completion_score_pct,
                focus_score_pct: focus_score.focus_score_pct,
                ..Self::csv_row_defaults("focus_score")
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
    favorites: Vec<String>,
    archived: Vec<String>,
) -> (
    Vec<String>,
    Option<String>,
    BTreeSet<String>,
    BTreeSet<String>,
) {
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

    let mut normalized_selected = selected
        .and_then(|value| normalize_task_label(&value))
        .map(|value| canonical_task_label(&normalized_labels, &value).unwrap_or(value));
    if let Some(selected_label) = normalized_selected.as_ref() {
        let key = selected_label.to_ascii_lowercase();
        if seen.insert(key) {
            normalized_labels.push(selected_label.clone());
        }
    }

    let task_label_favorites =
        normalize_task_label_state_keys(&mut normalized_labels, &mut seen, favorites);
    let task_label_archived =
        normalize_task_label_state_keys(&mut normalized_labels, &mut seen, archived);
    if normalized_selected.as_ref().is_some_and(|selected_label| {
        task_label_archived.contains(&selected_label.to_ascii_lowercase())
    }) {
        normalized_selected = None;
    }

    (
        normalized_labels,
        normalized_selected,
        task_label_favorites,
        task_label_archived,
    )
}

fn normalize_task_label_state_keys(
    normalized_labels: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    values: Vec<String>,
) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for value in values {
        let Some(normalized) = normalize_task_label(&value) else {
            continue;
        };
        let canonical = canonical_task_label(normalized_labels, &normalized).unwrap_or(normalized);
        let key = canonical.to_ascii_lowercase();
        if seen.insert(key.clone()) {
            normalized_labels.push(canonical);
        }
        keys.insert(key);
    }
    keys
}

fn planner_state_labels_for_keys(keys: &BTreeSet<String>, labels: &[String]) -> Vec<String> {
    if keys.is_empty() {
        return Vec::new();
    }

    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    for label in labels {
        let key = label.to_ascii_lowercase();
        if keys.contains(&key) && seen.insert(key.clone()) {
            values.push(label.clone());
        }
    }
    for key in keys {
        if seen.insert(key.clone()) {
            values.push(key.clone());
        }
    }
    values
}

fn normalize_task_goal_targets(
    task_goal_targets: BTreeMap<String, DailyGoalSnapshot>,
) -> BTreeMap<String, DailyGoalSnapshot> {
    let mut normalized = BTreeMap::new();
    for (label, target) in task_goal_targets {
        if !target.has_any_target() {
            continue;
        }
        let Some(label) = normalize_task_label(&label) else {
            continue;
        };
        normalized.insert(label.to_ascii_lowercase(), target);
    }
    normalized
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

fn weekly_completion_score_pct(goal: DailyGoalSnapshot, totals: WeeklyStats) -> Option<u8> {
    let minute_score = if goal.minutes > 0 {
        let completed_minutes = totals.focused_minutes().min(goal.minutes);
        Some(percentage_round_nearest(completed_minutes, goal.minutes))
    } else {
        None
    };
    let pomodoro_score = if goal.pomodoros > 0 {
        let completed_pomodoros = totals.pomodoros_completed.min(goal.pomodoros);
        Some(percentage_round_nearest(
            u64::from(completed_pomodoros),
            u64::from(goal.pomodoros),
        ))
    } else {
        None
    };
    match (minute_score, pomodoro_score) {
        (None, None) => None,
        (Some(score), None) | (None, Some(score)) => Some(score),
        (Some(left), Some(right)) => Some(average_two_percentages(left, right)),
    }
}

fn average_two_percentages(left: u8, right: u8) -> u8 {
    let sum = u16::from(left) + u16::from(right);
    sum.div_ceil(2) as u8
}

fn format_week_label(year: i32, week: u32) -> String {
    format!("{year:04}-W{week:02}")
}

fn parse_week_label(week_label: &str) -> Option<(i32, u32)> {
    let (year, week) = week_label.split_once("-W")?;
    let parsed_year = year.parse::<i32>().ok()?;
    let parsed_week = week.parse::<u32>().ok()?;
    Some((parsed_year, parsed_week))
}

fn week_key_for_day(day: chrono::NaiveDate) -> String {
    let week = day.iso_week();
    format_week_label(week.year(), week.week())
}

fn month_key_for_day(day: chrono::NaiveDate) -> String {
    format!("{:04}-{:02}", day.year(), day.month())
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
mod tests;
