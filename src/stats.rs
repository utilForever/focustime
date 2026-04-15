use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;

use chrono::Datelike;
use serde::{Deserialize, Serialize};

#[cfg_attr(test, allow(dead_code))]
const STATS_FILE_NAME: &str = "stats.toml";

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

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(&self.to_persisted())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Best-effort atomic write: temp file + rename.
        // On Windows, rename cannot replace an existing file; fall back to
        // remove+rename when destination already exists.
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, &content)?;
        #[cfg(target_os = "windows")]
        {
            match fs::rename(&tmp, &path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    fs::remove_file(&path)?;
                    fs::rename(&tmp, &path)
                }
                Err(e) => Err(e),
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            fs::rename(&tmp, &path)
        }
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

        weekly
            .into_iter()
            .rev()
            .take(limit)
            .map(|(_, stats)| stats)
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
