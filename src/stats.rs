use std::collections::BTreeMap;
use std::fs;
use std::io;

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
pub struct DailyStats {
    #[serde(default)]
    pub pomodoros_completed: u32,
    #[serde(default)]
    pub focused_seconds: u64,
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
    #[cfg(not(test))]
    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    #[cfg(test)]
    pub fn load() -> Self {
        Self::default()
    }

    #[cfg(not(test))]
    fn try_load() -> Option<Self> {
        let path = crate::config::app_data_path(STATS_FILE_NAME)?;
        let content = fs::read_to_string(path).ok()?;
        Self::try_from_toml(&content)
    }

    fn try_from_toml(content: &str) -> Option<Self> {
        let persisted: PersistedStats = toml::from_str(content).ok()?;
        Some(Self::from_persisted(persisted))
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

    pub fn record_focus_elapsed(&mut self, day_key: &str, elapsed_secs: u64) {
        if elapsed_secs == 0 {
            return;
        }

        self.session.focused_seconds = self.session.focused_seconds.saturating_add(elapsed_secs);
        let daily = self.daily.entry(day_key.to_string()).or_default();
        daily.focused_seconds = daily.focused_seconds.saturating_add(elapsed_secs);
    }

    pub fn record_completed_pomodoro(&mut self, day_key: &str) {
        self.session.pomodoros_completed = self.session.pomodoros_completed.saturating_add(1);
        let daily = self.daily.entry(day_key.to_string()).or_default();
        daily.pomodoros_completed = daily.pomodoros_completed.saturating_add(1);
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
}

pub fn current_day_key() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_updates_session_and_daily_totals() {
        let mut stats = FocusStats::default();

        stats.record_focus_elapsed("2026-04-09", 125);
        stats.record_completed_pomodoro("2026-04-09");

        let session = stats.session();
        assert_eq!(session.pomodoros_completed, 1);
        assert_eq!(session.focused_seconds, 125);
        assert_eq!(session.focused_minutes(), 2);

        let day = stats.daily_for("2026-04-09");
        assert_eq!(day.pomodoros_completed, 1);
        assert_eq!(day.focused_seconds, 125);
        assert_eq!(day.focused_minutes(), 2);
    }

    #[test]
    fn recent_daily_is_sorted_newest_first() {
        let mut stats = FocusStats::default();
        stats.record_focus_elapsed("2026-04-08", 60);
        stats.record_focus_elapsed("2026-04-09", 120);

        let recent = stats.recent_daily(2);
        assert_eq!(recent[0].0, "2026-04-09");
        assert_eq!(recent[1].0, "2026-04-08");
    }

    #[test]
    fn persisted_stats_round_trip_preserves_daily_history() {
        let mut original = FocusStats::default();
        original.record_focus_elapsed("2026-04-09", 300);
        original.record_completed_pomodoro("2026-04-09");

        let persisted = original.to_persisted();
        let toml_str = toml::to_string_pretty(&persisted).unwrap();
        let restored = FocusStats::try_from_toml(&toml_str).unwrap();

        // Session stats are intentionally runtime-only and reset on startup.
        assert_eq!(restored.session(), SessionStats::default());
        let day = restored.daily_for("2026-04-09");
        assert_eq!(day.pomodoros_completed, 1);
        assert_eq!(day.focused_seconds, 300);
    }

    #[test]
    fn invalid_toml_falls_back_to_none_for_loader() {
        assert!(FocusStats::try_from_toml("this is not valid toml").is_none());
    }

    #[test]
    fn current_day_key_uses_iso_date_format() {
        let key = current_day_key();
        assert_eq!(key.len(), 10);
        assert_eq!(&key[4..5], "-");
        assert_eq!(&key[7..8], "-");
    }
}
