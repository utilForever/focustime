use std::fmt;

use crate::error::{UserFacingError, UserMessage};

pub(crate) type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AppError {
    MissingTaskLabel { shortcut_hint: String },
    TimerNotIdleFocusPhase,
    TimerNotRunning { action: &'static str },
    TimerNotPaused,
    StrictModeActive { action: &'static str },
    TimerAlreadyIdle,
    TaskLabelEmpty,
    TaskLabelLookupFailed,
    ArchivedTaskLabel { label: String },
    BlockingPreviewFailed { source: String },
    WorkflowFailed { source: String },
}

impl AppError {
    pub(crate) fn workflow(source: impl Into<String>) -> Self {
        Self::WorkflowFailed {
            source: source.into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, needle: &str) -> bool {
        self.user_message().message.contains(needle)
    }
}

impl UserFacingError for AppError {
    fn user_message(&self) -> UserMessage {
        match self {
            Self::MissingTaskLabel { shortcut_hint } => UserMessage::with_hint(
                "app.focus.missing_task_label",
                format!(
                    "Cannot start focus: select a task label first (run TUI and press {shortcut_hint})."
                ),
                "Select a task with `focustime --task LABEL`, then run `focustime --start` again.",
            ),
            Self::TimerNotIdleFocusPhase => UserMessage::with_hint(
                "app.timer.not_idle_focus_phase",
                "Cannot start focus: timer is not idle in focus phase.",
                "Stop or finish the current phase before starting a new focus session.",
            ),
            Self::TimerNotRunning { action } => UserMessage::with_hint(
                "app.timer.not_running",
                format!("Cannot {action}: timer is not running."),
                "Start a focus session first with `focustime --start`.",
            ),
            Self::TimerNotPaused => UserMessage::with_hint(
                "app.timer.not_paused",
                "Cannot resume: timer is not paused.",
                "Pause a running session with `focustime --pause` before resuming.",
            ),
            Self::StrictModeActive { action } => UserMessage::with_hint(
                "app.timer.strict_mode_active",
                format!("Cannot {action}: strict mode is active during focus."),
                "Disable strict mode or wait until the focus phase ends.",
            ),
            Self::TimerAlreadyIdle => UserMessage::with_hint(
                "app.timer.already_idle",
                "Cannot stop: timer is already idle.",
                "Start a focus session before stopping it.",
            ),
            Self::TaskLabelEmpty => UserMessage::with_hint(
                "app.task_label.empty",
                "Cannot select task label: label cannot be empty.",
                "Pass a non-empty label, for example `focustime --task Docs`.",
            ),
            Self::TaskLabelLookupFailed => UserMessage::new(
                "app.task_label.lookup_failed",
                "Cannot select task label: label lookup failed.",
            ),
            Self::ArchivedTaskLabel { label } => UserMessage::with_hint(
                "app.task_label.archived",
                format!(
                    "Cannot select archived task label `{label}`. Unarchive it in task setup first."
                ),
                "Open task setup and unarchive the task before selecting it from the CLI.",
            ),
            Self::BlockingPreviewFailed { source } => UserMessage::with_hint(
                "app.blocking_preview.failed",
                format!("Failed to generate blocking preview: {source}"),
                "Run `focustime --diagnostics` to inspect blocking backend setup.",
            ),
            Self::WorkflowFailed { source } => UserMessage::new("app.workflow.failed", source),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.user_message().message)
    }
}

impl std::error::Error for AppError {}

impl From<String> for AppError {
    fn from(source: String) -> Self {
        Self::workflow(source)
    }
}

impl From<AppError> for UserMessage {
    fn from(error: AppError) -> Self {
        error.user_message()
    }
}

impl PartialEq<&str> for AppError {
    fn eq(&self, other: &&str) -> bool {
        self.user_message().message == *other
    }
}

impl PartialEq<String> for AppError {
    fn eq(&self, other: &String) -> bool {
        self.user_message().message == *other
    }
}
