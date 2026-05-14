use std::path::PathBuf;

#[cfg(not(test))]
use crate::stats::fs;
use crate::stats::{
    BreakGlassOverrideEvent, FocusSessionRecord, FocusStats, PersistedStats, STATS_FILE_NAME,
    SessionInterruptionEvent, SessionStats, StatsLoadOptions, StatsSaveOptions, io,
    normalize_session_metadata_text, normalize_task_goal_targets, normalize_task_label,
    normalize_task_planner_state, planner_state_labels_for_keys, write_atomic_bytes,
};

impl FocusStats {
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn load() -> Result<Self, String> {
        Self::load_with_options(StatsLoadOptions::default())
    }

    #[cfg(test)]
    pub fn load_with_options(_options: StatsLoadOptions) -> Result<Self, String> {
        Ok(Self::default())
    }

    #[cfg(not(test))]
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn load() -> Result<Self, String> {
        Self::load_with_options(StatsLoadOptions::default())
    }

    #[cfg(not(test))]
    pub fn load_with_options(options: StatsLoadOptions) -> Result<Self, String> {
        Self::try_load(options)
    }

    #[cfg(not(test))]
    fn try_load(options: StatsLoadOptions) -> Result<Self, String> {
        let path = stats_path().ok_or_else(|| "cannot determine stats directory".to_string())?;
        match fs::read_to_string(&path) {
            Ok(content) => Self::try_from_toml_with_options(&content, options)
                .map_err(|error| format!("stats parse failed at `{}`: {error}", path.display())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!(
                "stats read failed at `{}`: {error}",
                path.display()
            )),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn try_from_toml(content: &str) -> Result<Self, String> {
        Self::try_from_toml_with_options(content, StatsLoadOptions::default())
    }

    pub(super) fn try_from_toml_with_options(
        content: &str,
        options: StatsLoadOptions,
    ) -> Result<Self, String> {
        let persisted: PersistedStats =
            toml::from_str(content).map_err(|e| format!("stats parse failed: {e}"))?;
        Ok(Self::from_persisted(persisted, options))
    }

    fn from_persisted(persisted: PersistedStats, _options: StatsLoadOptions) -> Self {
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
                let focus_intention = normalize_session_metadata_text(&session.focus_intention);
                let task_note = normalize_session_metadata_text(&session.task_note);
                let focus_intention = focus_intention.unwrap_or_default();
                let task_note = task_note.unwrap_or_default();
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

    pub(super) fn to_persisted(&self) -> PersistedStats {
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

    pub fn save_with_options(&self, _options: StatsSaveOptions) -> io::Result<()> {
        let path = stats_path().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "cannot determine stats directory")
        })?;
        let content = toml::to_string_pretty(&self.to_persisted())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        write_atomic_bytes(&path, content.as_bytes())
    }
}

fn stats_path() -> Option<PathBuf> {
    crate::config::stats_data_path(STATS_FILE_NAME)
}
