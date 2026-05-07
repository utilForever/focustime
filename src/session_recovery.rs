use std::io;

#[cfg(not(test))]
use std::{fs, path::Path};

#[cfg(test)]
use std::cell::RefCell;

use serde::{Deserialize, Serialize};

use crate::{
    config::ProfileId,
    task_labels::normalize_task_label,
    timer::{TimerPhase, TimerState, TimerStatus},
};

#[cfg(not(test))]
use crate::config::app_data_path;

#[cfg(not(test))]
const RECOVERY_FILE_NAME: &str = "session-recovery.toml";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryTimerPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

impl RecoveryTimerPhase {
    pub fn from_timer_phase(phase: TimerPhase) -> Self {
        match phase {
            TimerPhase::Focus => Self::Focus,
            TimerPhase::ShortBreak => Self::ShortBreak,
            TimerPhase::LongBreak => Self::LongBreak,
        }
    }

    pub fn to_timer_phase(self) -> TimerPhase {
        match self {
            Self::Focus => TimerPhase::Focus,
            Self::ShortBreak => TimerPhase::ShortBreak,
            Self::LongBreak => TimerPhase::LongBreak,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryTimerStatus {
    Idle,
    Running,
    Paused,
}

impl RecoveryTimerStatus {
    pub fn from_timer_status(status: TimerStatus) -> Self {
        match status {
            TimerStatus::Idle => Self::Idle,
            TimerStatus::Running => Self::Running,
            TimerStatus::Paused => Self::Paused,
        }
    }

    pub fn to_timer_status(self) -> TimerStatus {
        match self {
            Self::Idle => TimerStatus::Idle,
            Self::Running => TimerStatus::Running,
            Self::Paused => TimerStatus::Paused,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InProgressSessionSnapshot {
    pub phase: RecoveryTimerPhase,
    pub status: RecoveryTimerStatus,
    pub remaining_secs: u64,
    #[serde(default)]
    pub pomodoros_completed: u32,
    pub selected_task_label: Option<String>,
    #[serde(default)]
    pub focus_intention: Option<String>,
    #[serde(default)]
    pub task_note: Option<String>,
    pub selected_profile: ProfileId,
}

impl InProgressSessionSnapshot {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_timer_state_with_metadata(
        timer: &TimerState,
        selected_task_label: Option<String>,
        focus_intention: Option<String>,
        task_note: Option<String>,
        selected_profile: ProfileId,
    ) -> Option<Self> {
        Self::from_timer_state_with_metadata_with_fallback(
            timer,
            selected_task_label,
            focus_intention,
            task_note,
            selected_profile,
            true,
        )
    }

    pub fn from_timer_state_with_metadata_with_fallback(
        timer: &TimerState,
        selected_task_label: Option<String>,
        focus_intention: Option<String>,
        task_note: Option<String>,
        selected_profile: ProfileId,
        metadata_fallback_to_task_label: bool,
    ) -> Option<Self> {
        if timer.status == TimerStatus::Idle {
            return None;
        }

        let selected_task_label = selected_task_label
            .as_deref()
            .and_then(normalize_task_label)?;
        let focus_intention = focus_intention.as_deref().and_then(normalize_metadata_text);
        let task_note = task_note.as_deref().and_then(normalize_metadata_text);
        let focus_intention = if focus_intention.is_some() || !metadata_fallback_to_task_label {
            focus_intention
        } else {
            Some(selected_task_label.clone())
        };
        let task_note = if task_note.is_some() || !metadata_fallback_to_task_label {
            task_note
        } else {
            Some(selected_task_label.clone())
        };

        Some(Self {
            phase: RecoveryTimerPhase::from_timer_phase(timer.phase),
            status: RecoveryTimerStatus::from_timer_status(timer.status),
            remaining_secs: timer.remaining_secs,
            pomodoros_completed: timer.pomodoros_completed,
            selected_task_label: Some(selected_task_label),
            focus_intention,
            task_note,
            selected_profile,
        })
    }

    pub fn phase(&self) -> TimerPhase {
        self.phase.to_timer_phase()
    }

    pub fn status(&self) -> TimerStatus {
        self.status.to_timer_status()
    }

    pub fn normalized_task_label(&self) -> Option<String> {
        self.selected_task_label
            .as_deref()
            .and_then(normalize_task_label)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn normalized_focus_intention(&self) -> Option<String> {
        self.normalized_focus_intention_with_fallback(true)
    }

    pub fn normalized_focus_intention_with_fallback(
        &self,
        fallback_to_task_label: bool,
    ) -> Option<String> {
        let normalized = self
            .focus_intention
            .as_deref()
            .and_then(normalize_metadata_text);
        if normalized.is_some() || !fallback_to_task_label {
            normalized
        } else {
            self.normalized_task_label()
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn normalized_task_note(&self) -> Option<String> {
        self.normalized_task_note_with_fallback(true)
    }

    pub fn normalized_task_note_with_fallback(
        &self,
        fallback_to_task_label: bool,
    ) -> Option<String> {
        let normalized = self.task_note.as_deref().and_then(normalize_metadata_text);
        if normalized.is_some() || !fallback_to_task_label {
            normalized
        } else {
            self.normalized_task_label()
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn validate_for_timer(&self, timer: &TimerState) -> Result<(), String> {
        self.validate_for_timer_with_fallback(timer, true)
    }

    pub fn validate_for_timer_with_fallback(
        &self,
        timer: &TimerState,
        fallback_to_task_label: bool,
    ) -> Result<(), String> {
        if !matches!(
            self.status,
            RecoveryTimerStatus::Running | RecoveryTimerStatus::Paused
        ) {
            return Err("saved status is not in-progress".to_string());
        }

        if self.normalized_task_label().is_none() {
            return Err("saved task label is missing or invalid".to_string());
        }
        if self
            .normalized_focus_intention_with_fallback(fallback_to_task_label)
            .is_none()
        {
            return Err("saved focus intention is missing or invalid".to_string());
        }
        if self
            .normalized_task_note_with_fallback(fallback_to_task_label)
            .is_none()
        {
            return Err("saved task note is missing or invalid".to_string());
        }

        let phase_duration_secs = phase_duration_secs(timer, self.phase());
        if self.remaining_secs == 0 || self.remaining_secs > phase_duration_secs {
            return Err(format!(
                "saved remaining time {}s is out of range for {} phase",
                self.remaining_secs,
                self.phase().label()
            ));
        }

        Ok(())
    }
}

#[cfg(not(test))]
pub fn load() -> Result<Option<InProgressSessionSnapshot>, String> {
    let path = recovery_path().map_err(|e| format!("session recovery path failed: {e}"))?;
    match fs::read_to_string(path) {
        Ok(content) => toml::from_str(&content)
            .map(Some)
            .map_err(|e| format!("session recovery parse failed: {e}")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("session recovery read failed: {e}")),
    }
}

#[cfg(not(test))]
pub fn save(snapshot: &InProgressSessionSnapshot) -> io::Result<()> {
    let path = recovery_path()?;
    let content = toml::to_string_pretty(snapshot)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_atomic_text(&path, &content)
}

#[cfg(not(test))]
pub fn clear() -> io::Result<()> {
    let path = recovery_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
thread_local! {
    static TEST_LOAD_OVERRIDE: RefCell<Option<Result<Option<InProgressSessionSnapshot>, String>>> = const { RefCell::new(None) };
    static TEST_SAVED_SNAPSHOT: RefCell<Option<InProgressSessionSnapshot>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub fn load() -> Result<Option<InProgressSessionSnapshot>, String> {
    TEST_LOAD_OVERRIDE.with(|slot| slot.borrow_mut().take().unwrap_or(Ok(None)))
}

#[cfg(test)]
pub fn save(snapshot: &InProgressSessionSnapshot) -> io::Result<()> {
    TEST_SAVED_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = Some(snapshot.clone());
    });
    Ok(())
}

#[cfg(test)]
pub fn clear() -> io::Result<()> {
    TEST_SAVED_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = None;
    });
    Ok(())
}

#[cfg(test)]
pub(crate) fn set_test_load_snapshot(snapshot: Option<InProgressSessionSnapshot>) {
    TEST_LOAD_OVERRIDE.with(|slot| {
        *slot.borrow_mut() = Some(Ok(snapshot));
    });
}

#[cfg(test)]
pub(crate) fn set_test_load_error(error: impl Into<String>) {
    TEST_LOAD_OVERRIDE.with(|slot| {
        *slot.borrow_mut() = Some(Err(error.into()));
    });
}

#[cfg(test)]
pub(crate) fn test_saved_snapshot() -> Option<InProgressSessionSnapshot> {
    TEST_SAVED_SNAPSHOT.with(|slot| slot.borrow().clone())
}

#[cfg(not(test))]
fn recovery_path() -> io::Result<std::path::PathBuf> {
    app_data_path(RECOVERY_FILE_NAME).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot determine session recovery directory",
        )
    })
}

#[cfg(not(test))]
fn write_atomic_text(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, content)?;

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

fn phase_duration_secs(timer: &TimerState, phase: TimerPhase) -> u64 {
    match phase {
        TimerPhase::Focus => timer.focus_secs,
        TimerPhase::ShortBreak => timer.short_break_secs,
        TimerPhase::LongBreak => timer.long_break_secs,
    }
}

fn normalize_metadata_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use crate::session_recovery::*;

    #[test]
    fn snapshot_validation_rejects_idle_status() {
        let timer = TimerState::new();
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Idle,
            remaining_secs: 10,
            pomodoros_completed: 0,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Docs".to_string()),
            task_note: Some("Docs".to_string()),
            selected_profile: ProfileId::Classic,
        };

        assert!(snapshot.validate_for_timer(&timer).is_err());
    }

    #[test]
    fn snapshot_validation_rejects_zero_remaining_time() {
        let timer = TimerState::new();
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 0,
            pomodoros_completed: 0,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Docs".to_string()),
            task_note: Some("Docs".to_string()),
            selected_profile: ProfileId::Classic,
        };

        assert!(snapshot.validate_for_timer(&timer).is_err());
    }

    #[test]
    fn snapshot_validation_rejects_out_of_range_remaining_time() {
        let timer = TimerState::with_profile(60, 30, 90, 4);
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::ShortBreak,
            status: RecoveryTimerStatus::Paused,
            remaining_secs: 31,
            pomodoros_completed: 0,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Docs".to_string()),
            task_note: Some("Docs".to_string()),
            selected_profile: ProfileId::Classic,
        };

        assert!(snapshot.validate_for_timer(&timer).is_err());
    }

    #[test]
    fn snapshot_validation_rejects_missing_task_label() {
        let timer = TimerState::new();
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 50,
            pomodoros_completed: 0,
            selected_task_label: None,
            focus_intention: Some("Docs".to_string()),
            task_note: Some("Docs".to_string()),
            selected_profile: ProfileId::Classic,
        };

        assert!(snapshot.validate_for_timer(&timer).is_err());
    }

    #[test]
    fn snapshot_validation_accepts_running_state() {
        let timer = TimerState::with_profile(60, 30, 90, 4);
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::LongBreak,
            status: RecoveryTimerStatus::Paused,
            remaining_secs: 90,
            pomodoros_completed: 2,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Docs".to_string()),
            task_note: Some("Docs".to_string()),
            selected_profile: ProfileId::DeepWork,
        };

        assert!(snapshot.validate_for_timer(&timer).is_ok());
    }

    #[test]
    fn metadata_defaults_to_task_label_for_legacy_snapshots() {
        let timer = TimerState::with_profile(60, 30, 90, 4);
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 60,
            pomodoros_completed: 1,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: None,
            task_note: None,
            selected_profile: ProfileId::Classic,
        };

        assert_eq!(
            snapshot.normalized_focus_intention().as_deref(),
            Some("Docs")
        );
        assert_eq!(snapshot.normalized_task_note().as_deref(), Some("Docs"));
        assert!(snapshot.validate_for_timer(&timer).is_ok());
    }

    #[test]
    fn metadata_fallback_can_be_disabled_for_legacy_snapshots() {
        let timer = TimerState::with_profile(60, 30, 90, 4);
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 60,
            pomodoros_completed: 1,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: None,
            task_note: None,
            selected_profile: ProfileId::Classic,
        };

        assert_eq!(
            snapshot
                .normalized_focus_intention_with_fallback(false)
                .as_deref(),
            None
        );
        assert_eq!(
            snapshot
                .normalized_task_note_with_fallback(false)
                .as_deref(),
            None
        );
        assert!(
            snapshot
                .validate_for_timer_with_fallback(&timer, false)
                .is_err()
        );
    }
}
