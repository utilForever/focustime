use crate::app::{
    App, DateTime, FocusStartOutcome, Local, ScheduleDisplayState, ShortcutAction, TimerPhase,
    TimerState, TimerStatus, WindowOccurrence, active_occurrence, next_occurrence_after,
    occurrence_key,
};

struct ScheduleShortcutLabels {
    planner: String,
    toggle_pause: String,
    next_phase: String,
}

impl App {
    pub(crate) fn recurring_schedule_display_texts(&self) -> (String, String) {
        self.recurring_schedule_texts_at(self.current_frame_now)
    }

    pub(super) fn recurring_schedule_texts_at(&self, now: DateTime<Local>) -> (String, String) {
        let state = self.schedule_display_state_at(now);
        let labels = ScheduleShortcutLabels {
            planner: self.shortcut_hint(ShortcutAction::OpenSessionPlanner),
            toggle_pause: self.shortcut_hint(ShortcutAction::TimerTogglePause),
            next_phase: self.shortcut_hint(ShortcutAction::TimerNextPhase),
        };
        let next_text = schedule_next_window_text_from_state(&state, now);
        let status_text = schedule_status_text_from_state(&state, &labels);

        (next_text, status_text)
    }

    fn schedule_display_state_at(&self, now: DateTime<Local>) -> ScheduleDisplayState {
        let active_window = self.active_schedule_occurrence_at(now);
        let next_window = next_occurrence_after(now, &self.recurring_windows);
        ScheduleDisplayState {
            has_schedule_windows: !self.recurring_windows.is_empty(),
            active_window,
            next_window,
            is_armed: self.schedule_armed_occurrence_key.is_some(),
            has_selected_task: self.has_selectable_task_label_for_focus(),
            timer_phase: self.timer.phase,
            timer_status: self.timer.status,
        }
    }

    pub(super) fn active_schedule_occurrence_at(
        &self,
        now: DateTime<Local>,
    ) -> Option<WindowOccurrence> {
        active_occurrence(now, &self.recurring_windows)
    }

    pub(super) fn sync_recurring_schedule(&mut self, now: DateTime<Local>) {
        let active_window = self.active_schedule_occurrence_at(now);
        let active_occurrence_key = active_window.as_ref().map(occurrence_key);
        self.last_active_schedule_occurrence_key = active_occurrence_key.clone();

        if self.recurring_windows.is_empty() {
            self.schedule_armed_occurrence_key = None;
            return;
        }

        let Some(_active_window) = active_window else {
            self.schedule_armed_occurrence_key = None;
            return;
        };

        let Some(active_occurrence_key) = active_occurrence_key else {
            self.schedule_armed_occurrence_key = None;
            return;
        };
        if self.last_schedule_occurrence_key.as_deref() != Some(active_occurrence_key.as_str()) {
            self.last_schedule_occurrence_key = Some(active_occurrence_key.clone());
            self.handle_schedule_window_start(&active_occurrence_key);
        } else if self.focus_session_active_for_current_state() {
            self.schedule_armed_occurrence_key = None;
        }
    }

    fn handle_schedule_window_start(&mut self, active_occurrence_key: &str) {
        if self.focus_session_active_for_current_state() {
            self.schedule_armed_occurrence_key = None;
            return;
        }

        // Scheduled windows always target focus. If we're currently in a break phase
        // (idle/paused/running), advance to focus first so schedule auto-start checks
        // and manual [Space] behavior both operate on the focus phase.
        if self.timer.phase != TimerPhase::Focus {
            self.update_timer_and_sync(TimerState::next_phase);
        }

        match self.try_start_focus_session() {
            Ok(FocusStartOutcome::Started) => {
                self.phase_notification =
                    Some("Scheduled window started. Focus auto-started.".to_string());
                self.schedule_armed_occurrence_key = None;
            }
            Ok(FocusStartOutcome::MissingTaskLabel) | Ok(FocusStartOutcome::NotIdleFocusPhase) => {
                self.schedule_armed_occurrence_key = Some(active_occurrence_key.to_string());
                self.phase_notification = Some(self.schedule_arm_notification());
            }
            Err(error) => {
                self.schedule_armed_occurrence_key = Some(active_occurrence_key.to_string());
                self.phase_notification = Some(error);
            }
        }
    }

    fn schedule_arm_notification(&self) -> String {
        if !self.has_selectable_task_label_for_focus() {
            format!(
                "Scheduled window started. Select a task label with {}, then press {} to start focus.",
                self.shortcut_hint(ShortcutAction::OpenSessionPlanner),
                self.shortcut_hint(ShortcutAction::TimerTogglePause),
            )
        } else {
            format!(
                "Scheduled window started. Press {} to start focus.",
                self.shortcut_hint(ShortcutAction::TimerTogglePause),
            )
        }
    }
}

fn schedule_next_window_text_from_state(
    state: &ScheduleDisplayState,
    now: DateTime<Local>,
) -> String {
    if !state.has_schedule_windows {
        return "🗓  Next schedule: none configured".to_string();
    }

    if let Some(active) = state.active_window.as_ref() {
        let mut text = format!(
            "🗓  Next schedule: in progress until {}",
            active.end.format("%H:%M")
        );
        if let Some(next) = state.next_window.as_ref() {
            text.push_str(&format!(
                " · then {}",
                format_schedule_occurrence_for_display(next, now)
            ));
        }
        return text;
    }

    if let Some(next) = state.next_window.as_ref() {
        return format!(
            "🗓  Next schedule: {}",
            format_schedule_occurrence_for_display(next, now)
        );
    }

    "🗓  Next schedule: no upcoming window".to_string()
}

fn schedule_status_text_from_state(
    state: &ScheduleDisplayState,
    labels: &ScheduleShortcutLabels,
) -> String {
    if !state.has_schedule_windows {
        return "⚙  Schedule status: off".to_string();
    }

    if state.active_window.is_some() {
        return schedule_active_window_status_text(state, labels);
    }

    if state.is_armed {
        return schedule_armed_status_text(state.has_selected_task, labels);
    }

    "⚙  Schedule status: ready for next window".to_string()
}

fn schedule_active_window_status_text(
    state: &ScheduleDisplayState,
    labels: &ScheduleShortcutLabels,
) -> String {
    if state.timer_phase != TimerPhase::Focus {
        return format!(
            "⚙  Schedule status: window active; press {} to switch to focus",
            labels.next_phase
        );
    }

    match state.timer_status {
        TimerStatus::Running => "⚙  Schedule status: in window; focus running".to_string(),
        TimerStatus::Paused => format!(
            "⚙  Schedule status: window active; press {} to resume focus",
            labels.toggle_pause
        ),
        TimerStatus::Idle => {
            schedule_idle_focus_status_text(state.has_selected_task, state.is_armed, labels)
        }
    }
}

fn schedule_idle_focus_status_text(
    has_selected_task: bool,
    is_armed: bool,
    labels: &ScheduleShortcutLabels,
) -> String {
    if !has_selected_task {
        format!(
            "⚙  Schedule status: window active; select {}, then press {}",
            labels.planner, labels.toggle_pause
        )
    } else if is_armed {
        format!(
            "⚙  Schedule status: armed; press {} to start focus",
            labels.toggle_pause
        )
    } else {
        format!(
            "⚙  Schedule status: window active; press {} to start focus",
            labels.toggle_pause
        )
    }
}

fn schedule_armed_status_text(has_selected_task: bool, labels: &ScheduleShortcutLabels) -> String {
    if has_selected_task {
        format!(
            "⚙  Schedule status: armed; press {} to start focus",
            labels.toggle_pause
        )
    } else {
        format!(
            "⚙  Schedule status: armed; select {}, then press {}",
            labels.planner, labels.toggle_pause
        )
    }
}

fn format_schedule_occurrence_for_display(
    occurrence: &WindowOccurrence,
    now: DateTime<Local>,
) -> String {
    let start_date = occurrence.start.date_naive();
    let today = now.date_naive();
    let day = if start_date == today {
        "today".to_string()
    } else if today.succ_opt() == Some(start_date) {
        "tomorrow".to_string()
    } else {
        occurrence.start.format("%a").to_string()
    };
    format!(
        "{day} {}-{}",
        occurrence.start.format("%H:%M"),
        occurrence.end.format("%H:%M")
    )
}
