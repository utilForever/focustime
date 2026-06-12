use std::{
    io,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(not(test))]
use std::{fs, path::Path};

#[cfg(test)]
use std::cell::RefCell;

use serde::de::DeserializeOwned;
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

const RECOVERY_SNAPSHOT_SCHEMA_VERSION: u16 = 2;
const RECOVERY_CHECKSUM_ALGORITHM: &str = "fnv1a64-toml-v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RecoveryTimerPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

impl RecoveryTimerPhase {
    pub(crate) fn from_timer_phase(phase: TimerPhase) -> Self {
        match phase {
            TimerPhase::Focus => Self::Focus,
            TimerPhase::ShortBreak => Self::ShortBreak,
            TimerPhase::LongBreak => Self::LongBreak,
        }
    }

    pub(crate) fn to_timer_phase(self) -> TimerPhase {
        match self {
            Self::Focus => TimerPhase::Focus,
            Self::ShortBreak => TimerPhase::ShortBreak,
            Self::LongBreak => TimerPhase::LongBreak,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RecoveryTimerStatus {
    Idle,
    Running,
    Paused,
}

impl RecoveryTimerStatus {
    pub(crate) fn from_timer_status(status: TimerStatus) -> Self {
        match status {
            TimerStatus::Idle => Self::Idle,
            TimerStatus::Running => Self::Running,
            TimerStatus::Paused => Self::Paused,
        }
    }

    pub(crate) fn to_timer_status(self) -> TimerStatus {
        match self {
            Self::Idle => TimerStatus::Idle,
            Self::Running => TimerStatus::Running,
            Self::Paused => TimerStatus::Paused,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct InProgressSessionSnapshot {
    pub(crate) phase: RecoveryTimerPhase,
    pub(crate) status: RecoveryTimerStatus,
    pub(crate) remaining_secs: u64,
    #[serde(default)]
    pub(crate) pomodoros_completed: u32,
    pub(crate) selected_task_label: Option<String>,
    #[serde(default)]
    pub(crate) focus_intention: Option<String>,
    #[serde(default)]
    pub(crate) task_note: Option<String>,
    pub(crate) selected_profile: ProfileId,
    #[serde(default)]
    pub(crate) captured_at_epoch_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct WorkflowStateSnapshot {
    #[serde(default)]
    pub(crate) schedule_delayed_occurrence_key: Option<String>,
    #[serde(default)]
    pub(crate) schedule_delay_until_epoch_secs: Option<i64>,
    #[serde(default)]
    pub(crate) schedule_armed_occurrence_key: Option<String>,
    #[serde(default)]
    pub(crate) last_schedule_occurrence_key: Option<String>,
    #[serde(default)]
    pub(crate) break_glass_expires_at_epoch_secs: Option<i64>,
    #[serde(default)]
    pub(crate) break_glass_confirmation_pending: bool,
    #[serde(default)]
    pub(crate) strict_reset_confirmation_pending: bool,
    #[serde(default)]
    pub(crate) temporary_allowlist_entries: Vec<WorkflowTemporaryAllowlistEntrySnapshot>,
    #[serde(default)]
    pub(crate) temporary_overrides: Vec<WorkflowTemporaryOverrideSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct WorkflowTemporaryAllowlistEntrySnapshot {
    #[serde(default)]
    pub(crate) profile: String,
    #[serde(default)]
    pub(crate) site: String,
    #[serde(default)]
    pub(crate) expires_at_epoch_secs: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WorkflowTemporaryOverrideKind {
    AllowlistSite,
    BreakGlass,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct WorkflowTemporaryOverrideSnapshot {
    pub(crate) kind: WorkflowTemporaryOverrideKind,
    #[serde(default)]
    pub(crate) profile: Option<String>,
    #[serde(default)]
    pub(crate) site: Option<String>,
    #[serde(default)]
    pub(crate) expires_at_epoch_secs: Option<i64>,
    #[serde(default)]
    pub(crate) confirmation_pending: bool,
}

impl WorkflowTemporaryOverrideSnapshot {
    pub(crate) fn temporary_allowlist(
        profile: impl Into<String>,
        site: impl Into<String>,
        expires_at_epoch_secs: i64,
    ) -> Self {
        Self {
            kind: WorkflowTemporaryOverrideKind::AllowlistSite,
            profile: Some(profile.into()),
            site: Some(site.into()),
            expires_at_epoch_secs: Some(expires_at_epoch_secs),
            confirmation_pending: false,
        }
    }

    pub(crate) fn break_glass_active(expires_at_epoch_secs: i64) -> Self {
        Self {
            kind: WorkflowTemporaryOverrideKind::BreakGlass,
            profile: None,
            site: None,
            expires_at_epoch_secs: Some(expires_at_epoch_secs),
            confirmation_pending: false,
        }
    }

    pub(crate) fn break_glass_pending_confirmation() -> Self {
        Self {
            kind: WorkflowTemporaryOverrideKind::BreakGlass,
            profile: None,
            site: None,
            expires_at_epoch_secs: None,
            confirmation_pending: true,
        }
    }
}

impl WorkflowStateSnapshot {
    pub(crate) fn temporary_overrides_with_legacy_fallback(
        &self,
    ) -> Vec<WorkflowTemporaryOverrideSnapshot> {
        if !self.temporary_overrides.is_empty() {
            return self.temporary_overrides.clone();
        }

        let mut overrides = Vec::new();
        if let Some(expires_at_epoch_secs) = self.break_glass_expires_at_epoch_secs {
            overrides.push(WorkflowTemporaryOverrideSnapshot::break_glass_active(
                expires_at_epoch_secs,
            ));
        }
        if self.break_glass_confirmation_pending {
            overrides.push(WorkflowTemporaryOverrideSnapshot::break_glass_pending_confirmation());
        }
        overrides.extend(self.temporary_allowlist_entries.iter().map(|entry| {
            WorkflowTemporaryOverrideSnapshot::temporary_allowlist(
                entry.profile.clone(),
                entry.site.clone(),
                entry.expires_at_epoch_secs,
            )
        }));
        overrides
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedRecoverySnapshot<T> {
    schema_version: u16,
    checksum_algorithm: String,
    checksum: String,
    snapshot: T,
}

impl InProgressSessionSnapshot {
    #[allow(dead_code)]
    pub(crate) fn from_timer_state_with_metadata(
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

    pub(crate) fn phase(&self) -> TimerPhase {
        self.phase.to_timer_phase()
    }

    pub(crate) fn status(&self) -> TimerStatus {
        self.status.to_timer_status()
    }

    pub(crate) fn normalized_task_label(&self) -> Option<String> {
        self.selected_task_label
            .as_deref()
            .and_then(normalize_task_label)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn normalized_focus_intention(&self) -> Option<String> {
        self.focus_intention
            .as_deref()
            .and_then(normalize_metadata_text)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn normalized_task_note(&self) -> Option<String> {
        self.task_note.as_deref().and_then(normalize_metadata_text)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn validate_for_timer(&self, timer: &TimerState) -> Result<(), String> {
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
        if self.remaining_secs > phase_duration_secs {
            return Err(format!(
                "saved remaining time {}s is out of range for {} phase",
                self.remaining_secs,
                self.phase().label()
            ));
        }
        if self.remaining_secs == 0 && self.status != RecoveryTimerStatus::Running {
            return Err(format!(
                "saved remaining time 0s is only recoverable for running {} phase",
                self.phase().label()
            ));
        }
        if self.remaining_secs == 0 && self.captured_at_epoch_secs.is_none() {
            return Err(format!(
                "saved remaining time 0s is missing a capture timestamp for {} phase",
                self.phase().label()
            ));
        }

        Ok(())
    }

    pub(crate) fn reconcile_elapsed_for_timer(&self, timer: &TimerState) -> Self {
        let Some(now_epoch_secs) = current_epoch_secs() else {
            return self.clone();
        };
        self.reconcile_elapsed_for_timer_at_epoch_secs(timer, now_epoch_secs)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn reconcile_elapsed_for_timer_at_epoch_secs(
        &self,
        timer: &TimerState,
        now_epoch_secs: i64,
    ) -> Self {
        if self.status != RecoveryTimerStatus::Running {
            return self.clone();
        }
        if self.remaining_secs == 0 {
            return self.advance_completed_running_phase(timer);
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

        reconciled.advance_completed_running_phase(timer)
    }

    fn advance_completed_running_phase(&self, timer: &TimerState) -> Self {
        let mut reconciled = self.clone();
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
pub(crate) fn load() -> Result<Option<InProgressSessionSnapshot>, String> {
    let path = recovery_path().map_err(|e| format!("session recovery path failed: {e}"))?;
    load_recovery_file(
        &path,
        "session recovery",
        parse_in_progress_session_snapshot,
    )
}

#[cfg(not(test))]
pub(crate) fn save(snapshot: &InProgressSessionSnapshot) -> io::Result<()> {
    let path = recovery_path()?;
    let content = serialize_recovery_snapshot(snapshot)?;
    write_atomic_text(&path, &content)
}

#[cfg(not(test))]
pub(crate) fn clear() -> io::Result<()> {
    let path = recovery_path()?;
    remove_recovery_file_and_tmp(&path)
}

#[cfg(not(test))]
pub(crate) fn load_workflow_state() -> Result<Option<WorkflowStateSnapshot>, String> {
    let path = workflow_state_path().map_err(|e| format!("workflow state path failed: {e}"))?;
    load_recovery_file(&path, "workflow state", parse_workflow_state_snapshot)
}

#[cfg(not(test))]
pub(crate) fn save_workflow_state(snapshot: &WorkflowStateSnapshot) -> io::Result<()> {
    let path = workflow_state_path()?;
    let content = serialize_recovery_snapshot(snapshot)?;
    write_atomic_text(&path, &content)
}

#[cfg(not(test))]
pub(crate) fn clear_workflow_state() -> io::Result<()> {
    let path = workflow_state_path()?;
    remove_recovery_file_and_tmp(&path)
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

    let tmp_path = recovery_tmp_path(path);
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

#[cfg(not(test))]
fn load_recovery_file<T>(
    path: &Path,
    artifact_label: &str,
    parse: fn(&str) -> Result<T, String>,
) -> Result<Option<T>, String>
where
    T: Clone,
{
    let tmp_path = recovery_tmp_path(path);
    match read_and_parse_recovery_file(path, artifact_label, parse) {
        Ok(Some(snapshot)) => {
            let _ = fs::remove_file(&tmp_path);
            Ok(Some(snapshot))
        }
        Ok(None) => read_and_parse_recovery_file(&tmp_path, artifact_label, parse),
        Err(primary_error) => {
            match read_and_parse_recovery_file(&tmp_path, artifact_label, parse) {
                Ok(Some(snapshot)) => Ok(Some(snapshot)),
                Ok(None) => Err(primary_error),
                Err(tmp_error) => Err(format!("{primary_error}; fallback tmp failed: {tmp_error}")),
            }
        }
    }
}

#[cfg(not(test))]
fn read_and_parse_recovery_file<T>(
    path: &Path,
    artifact_label: &str,
    parse: fn(&str) -> Result<T, String>,
) -> Result<Option<T>, String> {
    match fs::read_to_string(path) {
        Ok(content) => parse(&content).map(Some),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{artifact_label} read failed: {e}")),
    }
}

#[cfg(not(test))]
fn remove_recovery_file_and_tmp(path: &Path) -> io::Result<()> {
    remove_file_if_exists(path)?;
    remove_file_if_exists(&recovery_tmp_path(path))
}

#[cfg(not(test))]
fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(not(test))]
fn recovery_tmp_path(path: &Path) -> std::path::PathBuf {
    path.with_extension("toml.tmp")
}

fn parse_in_progress_session_snapshot(content: &str) -> Result<InProgressSessionSnapshot, String> {
    parse_recovery_snapshot(content, "session recovery")
}

fn parse_workflow_state_snapshot(content: &str) -> Result<WorkflowStateSnapshot, String> {
    parse_recovery_snapshot(content, "workflow state")
}

fn parse_recovery_snapshot<T>(content: &str, artifact_label: &str) -> Result<T, String>
where
    T: DeserializeOwned + Serialize,
{
    if content.trim().is_empty() {
        return Err(format!("{artifact_label} is empty"));
    }

    match toml::from_str::<PersistedRecoverySnapshot<T>>(content) {
        Ok(persisted) => validate_persisted_recovery_snapshot(persisted, artifact_label),
        Err(envelope_error) if looks_like_recovery_envelope(content) => Err(format!(
            "{artifact_label} integrity envelope parse failed: {envelope_error}"
        )),
        Err(_) => toml::from_str(content)
            .map_err(|error| format!("{artifact_label} parse failed: {error}")),
    }
}

fn validate_persisted_recovery_snapshot<T>(
    persisted: PersistedRecoverySnapshot<T>,
    artifact_label: &str,
) -> Result<T, String>
where
    T: Serialize,
{
    if persisted.schema_version != RECOVERY_SNAPSHOT_SCHEMA_VERSION {
        return Err(format!(
            "{artifact_label} schema version {} is unsupported",
            persisted.schema_version
        ));
    }
    if persisted.checksum_algorithm != RECOVERY_CHECKSUM_ALGORITHM {
        return Err(format!(
            "{artifact_label} checksum algorithm '{}' is unsupported",
            persisted.checksum_algorithm
        ));
    }

    let snapshot_payload = canonical_snapshot_payload(&persisted.snapshot)
        .map_err(|error| format!("{artifact_label} payload serialization failed: {error}"))?;
    let expected_checksum = integrity_checksum(&snapshot_payload);
    if persisted.checksum != expected_checksum {
        return Err(format!("{artifact_label} integrity check failed"));
    }

    Ok(persisted.snapshot)
}

fn serialize_recovery_snapshot<T>(snapshot: &T) -> io::Result<String>
where
    T: Clone + Serialize,
{
    let snapshot_payload = canonical_snapshot_payload(snapshot)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let persisted = PersistedRecoverySnapshot {
        schema_version: RECOVERY_SNAPSHOT_SCHEMA_VERSION,
        checksum_algorithm: RECOVERY_CHECKSUM_ALGORITHM.to_string(),
        checksum: integrity_checksum(&snapshot_payload),
        snapshot: snapshot.clone(),
    };
    toml::to_string_pretty(&persisted).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn canonical_snapshot_payload<T>(snapshot: &T) -> Result<String, toml::ser::Error>
where
    T: Serialize,
{
    toml::to_string_pretty(snapshot)
}

fn looks_like_recovery_envelope(content: &str) -> bool {
    content.contains("schema_version")
        || content.contains("checksum")
        || content.contains("[snapshot]")
}

fn integrity_checksum(content: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
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
pub(crate) fn load() -> Result<Option<InProgressSessionSnapshot>, String> {
    TEST_LOAD_OVERRIDE.with(|slot| slot.borrow_mut().take().unwrap_or(Ok(None)))
}

#[cfg(test)]
pub(crate) fn save(snapshot: &InProgressSessionSnapshot) -> io::Result<()> {
    TEST_SAVED_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = Some(snapshot.clone());
    });
    Ok(())
}

#[cfg(test)]
pub(crate) fn clear() -> io::Result<()> {
    TEST_SAVED_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = None;
    });
    Ok(())
}

#[cfg(test)]
pub(crate) fn load_workflow_state() -> Result<Option<WorkflowStateSnapshot>, String> {
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
pub(crate) fn save_workflow_state(snapshot: &WorkflowStateSnapshot) -> io::Result<()> {
    TEST_SAVED_WORKFLOW_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = Some(snapshot.clone());
    });
    Ok(())
}

#[cfg(test)]
pub(crate) fn clear_workflow_state() -> io::Result<()> {
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
    fn running_snapshot_reconciliation_advances_zero_remaining_transition_snapshot() {
        let timer = TimerState::with_profile(60, 30, 90, 2);
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::ShortBreak,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 0,
            pomodoros_completed: 1,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: None,
            task_note: None,
            selected_profile: ProfileId::Classic,
            captured_at_epoch_secs: Some(100),
        };

        assert!(snapshot.validate_for_timer(&timer).is_ok());
        let reconciled = snapshot.reconcile_elapsed_for_timer_at_epoch_secs(&timer, 100);

        assert_eq!(reconciled.phase, RecoveryTimerPhase::Focus);
        assert_eq!(reconciled.status, RecoveryTimerStatus::Idle);
        assert_eq!(reconciled.remaining_secs, 60);
        assert_eq!(reconciled.pomodoros_completed, 1);
    }

    #[test]
    fn snapshot_validation_rejects_zero_remaining_paused_snapshot() {
        let timer = TimerState::with_profile(60, 30, 90, 2);
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Paused,
            remaining_secs: 0,
            pomodoros_completed: 0,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Docs".to_string()),
            task_note: Some("Docs".to_string()),
            selected_profile: ProfileId::Classic,
            captured_at_epoch_secs: Some(100),
        };

        assert!(snapshot.validate_for_timer(&timer).is_err());
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
    fn in_progress_snapshot_round_trips_with_integrity_envelope() {
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 120,
            pomodoros_completed: 2,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Write docs".to_string()),
            task_note: Some("API section".to_string()),
            selected_profile: ProfileId::Classic,
            captured_at_epoch_secs: Some(1_700_000_000),
        };

        let content = serialize_recovery_snapshot(&snapshot)
            .expect("snapshot should serialize with envelope");
        assert!(content.contains("schema_version = 2"));
        assert!(content.contains("[snapshot]"));

        let parsed =
            parse_in_progress_session_snapshot(&content).expect("enveloped snapshot should parse");
        assert_eq!(parsed, snapshot);
    }

    #[test]
    fn in_progress_snapshot_rejects_checksum_mismatch() {
        let snapshot = InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 120,
            pomodoros_completed: 2,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Write docs".to_string()),
            task_note: Some("API section".to_string()),
            selected_profile: ProfileId::Classic,
            captured_at_epoch_secs: Some(1_700_000_000),
        };
        let content = serialize_recovery_snapshot(&snapshot)
            .expect("snapshot should serialize with envelope");
        let corrupted = content.replace("remaining_secs = 120", "remaining_secs = 119");

        let error = parse_in_progress_session_snapshot(&corrupted)
            .expect_err("corrupted snapshot should fail integrity check");

        assert!(error.contains("integrity check failed"));
    }

    #[test]
    fn workflow_state_partial_envelope_does_not_fall_back_to_empty_legacy_state() {
        let content = r#"
schema_version = 2
checksum_algorithm = "fnv1a64-toml-v1"
checksum = "deadbeef"

[snapshot]
"#;

        let error = parse_workflow_state_snapshot(content)
            .expect_err("partial workflow envelope should fail integrity parsing");

        assert!(error.contains("integrity"));
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
            temporary_allowlist_entries: Vec::new(),
            temporary_overrides: Vec::new(),
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

    #[test]
    fn workflow_state_legacy_fields_synthesize_temporary_overrides() {
        let snapshot = WorkflowStateSnapshot {
            break_glass_expires_at_epoch_secs: Some(1_700_000_100),
            break_glass_confirmation_pending: true,
            temporary_allowlist_entries: vec![WorkflowTemporaryAllowlistEntrySnapshot {
                profile: "Work".to_string(),
                site: "reddit.com".to_string(),
                expires_at_epoch_secs: 1_700_000_200,
            }],
            ..WorkflowStateSnapshot::default()
        };

        let overrides = snapshot.temporary_overrides_with_legacy_fallback();

        assert!(
            overrides.contains(&WorkflowTemporaryOverrideSnapshot::break_glass_active(
                1_700_000_100
            ))
        );
        assert!(
            overrides
                .contains(&WorkflowTemporaryOverrideSnapshot::break_glass_pending_confirmation())
        );
        assert!(
            overrides.contains(&WorkflowTemporaryOverrideSnapshot::temporary_allowlist(
                "Work",
                "reddit.com",
                1_700_000_200
            ))
        );
    }

    #[test]
    fn workflow_state_explicit_temporary_overrides_take_precedence_over_legacy_fields() {
        let explicit = vec![WorkflowTemporaryOverrideSnapshot::temporary_allowlist(
            "Work",
            "news.ycombinator.com",
            1_700_000_300,
        )];
        let snapshot = WorkflowStateSnapshot {
            break_glass_expires_at_epoch_secs: Some(1_700_000_100),
            break_glass_confirmation_pending: true,
            temporary_allowlist_entries: vec![WorkflowTemporaryAllowlistEntrySnapshot {
                profile: "Work".to_string(),
                site: "reddit.com".to_string(),
                expires_at_epoch_secs: 1_700_000_200,
            }],
            temporary_overrides: explicit.clone(),
            ..WorkflowStateSnapshot::default()
        };

        let overrides = snapshot.temporary_overrides_with_legacy_fallback();

        assert_eq!(overrides, explicit);
    }
}
