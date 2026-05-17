use std::{
    io,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(not(test))]
use std::{fs, path::Path};

#[cfg(test)]
use std::cell::RefCell;

use serde::{Deserialize, Serialize};

use crate::{
    config::ProfileId,
    task_labels::normalize_task_label,
    timer::{DEFAULT_LONG_BREAK_INTERVAL, TimerPhase, TimerState, TimerStatus},
};

#[cfg(not(test))]
use crate::config::app_data_path;

#[cfg(not(test))]
const RECOVERY_FILE_NAME: &str = "session-recovery.toml";
#[cfg(not(test))]
const WORKFLOW_STATE_FILE_NAME: &str = "workflow-state.toml";

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
    #[serde(default)]
    pub captured_at_epoch_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkflowStateSnapshot {
    #[serde(default)]
    pub schedule_delayed_occurrence_key: Option<String>,
    #[serde(default)]
    pub schedule_delay_until_epoch_secs: Option<i64>,
    #[serde(default)]
    pub schedule_armed_occurrence_key: Option<String>,
    #[serde(default)]
    pub last_schedule_occurrence_key: Option<String>,
    #[serde(default)]
    pub break_glass_expires_at_epoch_secs: Option<i64>,
    #[serde(default)]
    pub break_glass_confirmation_pending: bool,
    #[serde(default)]
    pub strict_reset_confirmation_pending: bool,
}

impl InProgressSessionSnapshot {
    #[allow(dead_code)]
    pub fn from_timer_state_with_metadata(
        timer: &TimerState,
        selected_task_label: Option<String>,
        focus_intention: Option<String>,
        task_note: Option<String>,
        selected_profile: ProfileId,
    ) -> Option<Self> {
        if timer.status == TimerStatus::Idle {
            return None;
        }

        let selected_task_label = selected_task_label
            .as_deref()
            .and_then(normalize_task_label)?;
        let focus_intention = focus_intention.as_deref().and_then(normalize_metadata_text);
        let task_note = task_note.as_deref().and_then(normalize_metadata_text);

        Some(Self {
            phase: RecoveryTimerPhase::from_timer_phase(timer.phase),
            status: RecoveryTimerStatus::from_timer_status(timer.status),
            remaining_secs: timer.remaining_secs,
            pomodoros_completed: timer.pomodoros_completed,
            selected_task_label: Some(selected_task_label),
            focus_intention,
            task_note,
            selected_profile,
            captured_at_epoch_secs: current_epoch_secs(),
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
        self.focus_intention
            .as_deref()
            .and_then(normalize_metadata_text)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn normalized_task_note(&self) -> Option<String> {
        self.task_note.as_deref().and_then(normalize_metadata_text)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn validate_for_timer(&self, timer: &TimerState) -> Result<(), String> {
        if !matches!(
            self.status,
            RecoveryTimerStatus::Running | RecoveryTimerStatus::Paused
        ) {
            return Err("saved status is not in-progress".to_string());
        }

        if self.normalized_task_label().is_none() {
            return Err("saved task label is missing or invalid".to_string());
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

    pub fn reconcile_elapsed_for_timer(&self, timer: &TimerState) -> Self {
        let Some(now_epoch_secs) = current_epoch_secs() else {
            return self.clone();
        };
        self.reconcile_elapsed_for_timer_at_epoch_secs(timer, now_epoch_secs)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn reconcile_elapsed_for_timer_at_epoch_secs(
        &self,
        timer: &TimerState,
        now_epoch_secs: i64,
    ) -> Self {
        if self.status != RecoveryTimerStatus::Running {
            return self.clone();
        }
        let Some(captured_at_epoch_secs) = self.captured_at_epoch_secs else {
            return self.clone();
        };
        if now_epoch_secs <= captured_at_epoch_secs {
            return self.clone();
        }

        let elapsed_secs = (now_epoch_secs - captured_at_epoch_secs) as u64;
        if elapsed_secs == 0 {
            return self.clone();
        }

        let mut reconciled = self.clone();
        if elapsed_secs < reconciled.remaining_secs {
            reconciled.remaining_secs -= elapsed_secs;
            return reconciled;
        }

        match reconciled.phase() {
            TimerPhase::Focus => {
                reconciled.pomodoros_completed = reconciled.pomodoros_completed.saturating_add(1);
                let next_phase = next_phase_after_focus(timer, reconciled.pomodoros_completed);
                reconciled.phase = RecoveryTimerPhase::from_timer_phase(next_phase);
                reconciled.remaining_secs = phase_duration_secs(timer, next_phase);
            }
            TimerPhase::ShortBreak | TimerPhase::LongBreak => {
                reconciled.phase = RecoveryTimerPhase::from_timer_phase(TimerPhase::Focus);
                reconciled.remaining_secs = phase_duration_secs(timer, TimerPhase::Focus);
            }
        }

        reconciled.status = RecoveryTimerStatus::Idle;
        reconciled.focus_intention = None;
        reconciled.task_note = None;
        reconciled
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

#[cfg(not(test))]
pub fn load_workflow_state() -> Result<Option<WorkflowStateSnapshot>, String> {
    let path = workflow_state_path().map_err(|e| format!("workflow state path failed: {e}"))?;
    match fs::read_to_string(path) {
        Ok(content) => toml::from_str(&content)
            .map(Some)
            .map_err(|e| format!("workflow state parse failed: {e}")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("workflow state read failed: {e}")),
    }
}

#[cfg(not(test))]
pub fn save_workflow_state(snapshot: &WorkflowStateSnapshot) -> io::Result<()> {
    let path = workflow_state_path()?;
    let content = toml::to_string_pretty(snapshot)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_atomic_text(&path, &content)
}

#[cfg(not(test))]
pub fn clear_workflow_state() -> io::Result<()> {
    let path = workflow_state_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
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
fn workflow_state_path() -> io::Result<std::path::PathBuf> {
    app_data_path(WORKFLOW_STATE_FILE_NAME).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot determine workflow state directory",
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

fn next_phase_after_focus(timer: &TimerState, completed_focus_count: u32) -> TimerPhase {
    let long_break_interval = if timer.long_break_interval == 0 {
        DEFAULT_LONG_BREAK_INTERVAL
    } else {
        timer.long_break_interval
    };
    if completed_focus_count % long_break_interval == 0 {
        TimerPhase::LongBreak
    } else {
        TimerPhase::ShortBreak
    }
}

fn normalize_metadata_text(value: &str) -> Option<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn current_epoch_secs() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
}

#[cfg(test)]
thread_local! {
    static TEST_LOAD_OVERRIDE: RefCell<Option<Result<Option<InProgressSessionSnapshot>, String>>> = const { RefCell::new(None) };
    static TEST_SAVED_SNAPSHOT: RefCell<Option<InProgressSessionSnapshot>> = const { RefCell::new(None) };
    static TEST_WORKFLOW_LOAD_OVERRIDE: RefCell<Option<Result<Option<WorkflowStateSnapshot>, String>>> = const { RefCell::new(None) };
    static TEST_SAVED_WORKFLOW_SNAPSHOT: RefCell<Option<WorkflowStateSnapshot>> = const { RefCell::new(None) };
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
pub fn load_workflow_state() -> Result<Option<WorkflowStateSnapshot>, String> {
    let override_value = TEST_WORKFLOW_LOAD_OVERRIDE.with(|slot| slot.borrow_mut().take());
    if let Some(result) = override_value {
        return result;
    }

    TEST_SAVED_WORKFLOW_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = None;
    });
    Ok(None)
}

#[cfg(test)]
pub fn save_workflow_state(snapshot: &WorkflowStateSnapshot) -> io::Result<()> {
    TEST_SAVED_WORKFLOW_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = Some(snapshot.clone());
    });
    Ok(())
}

#[cfg(test)]
pub fn clear_workflow_state() -> io::Result<()> {
    TEST_SAVED_WORKFLOW_SNAPSHOT.with(|slot| {
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

#[cfg(test)]
pub(crate) fn set_test_load_workflow_state(snapshot: Option<WorkflowStateSnapshot>) {
    TEST_WORKFLOW_LOAD_OVERRIDE.with(|slot| {
        *slot.borrow_mut() = Some(Ok(snapshot));
    });
}

#[cfg(test)]
pub(crate) fn test_saved_workflow_snapshot() -> Option<WorkflowStateSnapshot> {
    TEST_SAVED_WORKFLOW_SNAPSHOT.with(|slot| slot.borrow().clone())
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
            captured_at_epoch_secs: None,
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
            captured_at_epoch_secs: None,
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
            captured_at_epoch_secs: None,
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
            captured_at_epoch_secs: None,
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
            captured_at_epoch_secs: None,
        };

        assert!(snapshot.validate_for_timer(&timer).is_ok());
    }

    #[test]
    fn metadata_remains_empty_for_legacy_snapshots_without_backfill() {
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
            captured_at_epoch_secs: None,
        };

        assert_eq!(snapshot.normalized_focus_intention(), None);
        assert_eq!(snapshot.normalized_task_note(), None);
        assert!(snapshot.validate_for_timer(&timer).is_ok());
    }

    #[test]
    fn running_snapshot_reconciliation_subtracts_elapsed_time() {
        let timer = TimerState::with_profile(60, 30, 90, 4);
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 50,
            pomodoros_completed: 1,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Write docs".to_string()),
            task_note: Some("Section 1".to_string()),
            selected_profile: ProfileId::Classic,
            captured_at_epoch_secs: Some(100),
        };

        let reconciled = snapshot.reconcile_elapsed_for_timer_at_epoch_secs(&timer, 120);

        assert_eq!(reconciled.phase, RecoveryTimerPhase::Focus);
        assert_eq!(reconciled.status, RecoveryTimerStatus::Running);
        assert_eq!(reconciled.remaining_secs, 30);
        assert_eq!(reconciled.pomodoros_completed, 1);
        assert_eq!(reconciled.focus_intention.as_deref(), Some("Write docs"));
        assert_eq!(reconciled.task_note.as_deref(), Some("Section 1"));
    }

    #[test]
    fn running_focus_snapshot_reconciliation_advances_phase_when_elapsed_exceeds_remaining() {
        let timer = TimerState::with_profile(60, 30, 90, 2);
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 10,
            pomodoros_completed: 1,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Write docs".to_string()),
            task_note: Some("Section 1".to_string()),
            selected_profile: ProfileId::Classic,
            captured_at_epoch_secs: Some(100),
        };

        let reconciled = snapshot.reconcile_elapsed_for_timer_at_epoch_secs(&timer, 115);

        assert_eq!(reconciled.phase, RecoveryTimerPhase::LongBreak);
        assert_eq!(reconciled.status, RecoveryTimerStatus::Idle);
        assert_eq!(reconciled.remaining_secs, 90);
        assert_eq!(reconciled.pomodoros_completed, 2);
        assert!(reconciled.focus_intention.is_none());
        assert!(reconciled.task_note.is_none());
        assert_eq!(reconciled.selected_task_label.as_deref(), Some("Docs"));
    }

    #[test]
    fn paused_snapshot_reconciliation_does_not_change_state() {
        let timer = TimerState::with_profile(60, 30, 90, 4);
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Paused,
            remaining_secs: 50,
            pomodoros_completed: 1,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Write docs".to_string()),
            task_note: Some("Section 1".to_string()),
            selected_profile: ProfileId::Classic,
            captured_at_epoch_secs: Some(100),
        };

        let reconciled = snapshot.reconcile_elapsed_for_timer_at_epoch_secs(&timer, 1_000);

        assert_eq!(reconciled, snapshot);
    }

    #[test]
    fn in_progress_snapshot_deserializes_legacy_payload_without_capture_timestamp() {
        let snapshot: InProgressSessionSnapshot = toml::from_str(
            r#"
phase = "focus"
status = "running"
remaining_secs = 1200
pomodoros_completed = 2
selected_task_label = "Docs"
focus_intention = "Write docs"
task_note = "API section"
selected_profile = "classic"
"#,
        )
        .expect("legacy payload should deserialize");

        assert!(snapshot.captured_at_epoch_secs.is_none());
    }

    #[test]
    fn load_workflow_state_without_override_clears_saved_snapshot() {
        save_workflow_state(&WorkflowStateSnapshot {
            schedule_delayed_occurrence_key: Some("recurring:0:2026-05-10".to_string()),
            schedule_delay_until_epoch_secs: Some(1_700_000_000),
            schedule_armed_occurrence_key: None,
            last_schedule_occurrence_key: None,
            break_glass_expires_at_epoch_secs: None,
            break_glass_confirmation_pending: true,
            strict_reset_confirmation_pending: false,
        })
        .expect("save should succeed");

        let loaded = load_workflow_state().expect("load should succeed");
        assert!(loaded.is_none());
        assert!(test_saved_workflow_snapshot().is_none());
    }

    #[test]
    fn workflow_state_snapshot_deserializes_legacy_payload_without_new_fields() {
        let snapshot: WorkflowStateSnapshot = toml::from_str(
            r#"
schedule_delayed_occurrence_key = "recurring:0:2026-05-10"
schedule_delay_until_epoch_secs = 1700000000
break_glass_expires_at_epoch_secs = 1700000100
break_glass_confirmation_pending = true
"#,
        )
        .expect("legacy payload should deserialize");

        assert_eq!(
            snapshot.schedule_delayed_occurrence_key.as_deref(),
            Some("recurring:0:2026-05-10")
        );
        assert_eq!(
            snapshot.schedule_delay_until_epoch_secs,
            Some(1_700_000_000)
        );
        assert!(snapshot.schedule_armed_occurrence_key.is_none());
        assert!(snapshot.last_schedule_occurrence_key.is_none());
        assert_eq!(
            snapshot.break_glass_expires_at_epoch_secs,
            Some(1_700_000_100)
        );
        assert!(snapshot.break_glass_confirmation_pending);
        assert!(!snapshot.strict_reset_confirmation_pending);
    }
}
