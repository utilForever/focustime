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

#[cfg_attr(test, allow(dead_code))]
const STATS_FILE_NAME: &str = "stats.toml";
const JSON_EXPORT_FILE_NAME: &str = "focustime-stats.json";
const CSV_EXPORT_FILE_NAME: &str = "focustime-stats.csv";
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

    pub fn is_met_by(self, stats: DailyStats) -> bool {
        self.has_any_target()
            && (self.minutes == 0 || stats.focused_minutes() >= self.minutes)
            && (self.pomodoros == 0 || stats.pomodoros_completed >= self.pomodoros)
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct StatsExport {
    daily: Vec<DailyExportRow>,
    weekly: Vec<WeeklyExportRow>,
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

#[derive(Debug, Clone, Serialize)]
struct CsvExportRow {
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct PersistedStats {
    #[serde(default)]
    daily: BTreeMap<String, DailyStats>,
}

#[derive(Debug, Clone, Default)]
pub struct FocusStats {
    session: SessionStats,
    daily: BTreeMap<String, DailyStats>,
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
        Self {
            session: SessionStats::default(),
            daily: persisted.daily,
        }
    }

    fn to_persisted(&self) -> PersistedStats {
        PersistedStats {
            daily: self.daily.clone(),
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
        self.session.pomodoros_completed = self.session.pomodoros_completed.saturating_add(1);
        let daily = self.daily.entry(day_key.to_string()).or_default();
        daily.pomodoros_completed = daily.pomodoros_completed.saturating_add(1);
        daily.goal = Some(goal);
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

    pub fn recent_daily(&self, limit: usize) -> Vec<(String, DailyStats)> {
        self.daily
            .iter()
            .rev()
            .take(limit)
            .map(|(day, stats)| (day.clone(), *stats))
            .collect()
    }

    pub fn recent_weekly(&self, limit: usize) -> Vec<WeeklyStats> {
        let mut weekly = self.weekly_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
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
            daily: self.export_daily_rows(),
            weekly: self.export_weekly_rows(),
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

    fn csv_rows(&self) -> Vec<CsvExportRow> {
        let mut rows = Vec::with_capacity(self.daily.len() + self.weekly.len());

        for daily in &self.daily {
            rows.push(CsvExportRow {
                record_type: "daily",
                date: Some(daily.date.clone()),
                week_label: None,
                year: None,
                week: None,
                pomodoros_completed: daily.pomodoros_completed,
                focused_seconds: daily.focused_seconds,
                focused_minutes: daily.focused_minutes,
                goal_minutes: daily.goal.map(|goal| goal.minutes),
                goal_pomodoros: daily.goal.map(|goal| goal.pomodoros),
                goal_met: daily.goal.map(|_| daily.goal_met),
            });
        }

        for weekly in &self.weekly {
            rows.push(CsvExportRow {
                record_type: "weekly",
                date: None,
                week_label: Some(weekly.week_label.clone()),
                year: Some(weekly.year),
                week: Some(weekly.week),
                pomodoros_completed: weekly.pomodoros_completed,
                focused_seconds: weekly.focused_seconds,
                focused_minutes: weekly.focused_minutes,
                goal_minutes: None,
                goal_pomodoros: None,
                goal_met: None,
            });
        }

        rows
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn export_to_dir_writes_daily_and_weekly_json_and_csv() {
        let mut stats = FocusStats::default();
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        stats.record_focus_elapsed("2026-04-06", 30 * 60, goal);
        stats.record_completed_pomodoro("2026-04-06", goal);
        stats.record_focus_elapsed("2026-04-08", 45 * 60, goal);
        stats.record_completed_pomodoro("2026-04-08", goal);

        let export_dir = unique_temp_dir("stats-export");
        let exported = stats.export_to_dir(&export_dir).unwrap();

        assert_eq!(
            exported.json_path.file_name().unwrap(),
            JSON_EXPORT_FILE_NAME
        );
        assert_eq!(exported.csv_path.file_name().unwrap(), CSV_EXPORT_FILE_NAME);

        let json = fs::read_to_string(&exported.json_path).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let daily = json_value["daily"].as_array().unwrap();
        let weekly = json_value["weekly"].as_array().unwrap();
        assert_eq!(daily.len(), 2);
        assert_eq!(weekly.len(), 1);
        assert_eq!(daily[0]["date"], "2026-04-06");
        assert_eq!(daily[0]["goal_met"], true);
        assert_eq!(weekly[0]["week_label"], "2026-W15");
        assert_eq!(weekly[0]["focused_minutes"], 75);

        let csv = fs::read_to_string(&exported.csv_path).unwrap();
        assert!(csv.contains("record_type,date,week_label,year,week,pomodoros_completed,focused_seconds,focused_minutes,goal_minutes,goal_pomodoros,goal_met"));
        assert!(csv.contains("daily,2026-04-06,,,,1,1800,30,25,1,true"));
        assert!(csv.contains("weekly,,2026-W15,2026,15,2,4500,75,,,"));

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
