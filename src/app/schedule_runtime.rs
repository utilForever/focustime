use crate::app::{
    App, DateTime, Local, ScheduleDisplayState, ShortcutAction, TimerPhase, TimerState,
    TimerStatus, WindowOccurrence, active_occurrence, active_one_time_occurrence,
    next_occurrence_after, next_one_time_occurrence_after, occurrence_key, pick_active_occurrence,
    pick_next_occurrence,
};

struct ScheduleShortcutLabels {
    planner: String,
    toggle_pause: String,
    next_phase: String,
    delay: String,
}

impl App {
    pub fn recurring_schedule_display_texts(&self) -> (String, String) {
        self.recurring_schedule_texts_at(self.current_frame_now)
    }

    pub(super) fn recurring_schedule_texts_at(&self, now: DateTime<Local>) -> (String, String) {
        let state = self.schedule_display_state_at(now);
        let labels = ScheduleShortcutLabels {
            planner: self.shortcut_hint(ShortcutAction::OpenSessionPlanner),
            toggle_pause: self.shortcut_hint(ShortcutAction::TimerTogglePause),
            next_phase: self.shortcut_hint(ShortcutAction::TimerNextPhase),
            delay: self.shortcut_hint(ShortcutAction::DelayScheduleStart),
        };
        (
            schedule_next_window_text_from_state(&state, now),
            schedule_status_text_from_state(&state, &labels),
        )
    }

    fn schedule_display_state_at(&self, now: DateTime<Local>) -> ScheduleDisplayState {
        let today = now.date_naive();
        let active_window = self.active_schedule_occurrence_at(now);
        let recurring_next = next_occurrence_after(
            now,
            &self.recurring_windows,
            &self.recurring_exception_dates,
        );
        let one_time_next = next_one_time_occurrence_after(now, &self.one_time_windows);
        let has_one_time_window_today = self
            .one_time_windows
            .iter()
            .any(|window| window.date == today);
        let delayed_until = active_window.as_ref().and_then(|occurrence| {
            self.schedule_delay_until_for_occurrence_key(&occurrence_key(occurrence), now)
        });
        ScheduleDisplayState {
            has_schedule_windows: !(self.recurring_windows.is_empty()
                && self.one_time_windows.is_empty()),
            active_window,
            next_window: pick_next_occurrence(recurring_next, one_time_next),
            is_exception_today: self.recurring_exception_dates.contains(&today)
                && !has_one_time_window_today,
            is_armed: self.schedule_armed_occurrence_key.is_some(),
            delayed_until,
            has_selected_task: self.has_selectable_task_label_for_focus(),
            timer_phase: self.timer.phase,
            timer_status: self.timer.status,
        }
    }

    pub(super) fn active_schedule_occurrence_at(
        &self,
        now: DateTime<Local>,
    ) -> Option<WindowOccurrence> {
        let recurring_active = active_occurrence(
            now,
            &self.recurring_windows,
            &self.recurring_exception_dates,
        );
        let one_time_active = active_one_time_occurrence(now, &self.one_time_windows);
        pick_active_occurrence(recurring_active, one_time_active)
    }

    pub(super) fn sync_recurring_schedule(&mut self, now: DateTime<Local>) {
        if self.recurring_windows.is_empty() && self.one_time_windows.is_empty() {
            self.schedule_armed_occurrence_key = None;
            self.clear_schedule_delay_state();
            return;
        }

        let Some(active_window) = self.active_schedule_occurrence_at(now) else {
            self.schedule_armed_occurrence_key = None;
            self.clear_schedule_delay_state();
            return;
        };

        let active_occurrence_key = occurrence_key(&active_window);
        if self.sync_schedule_delay_state_for_occurrence(&active_occurrence_key, now) {
            self.schedule_armed_occurrence_key = None;
            return;
        }
        if self.last_schedule_occurrence_key.as_deref() != Some(active_occurrence_key.as_str()) {
            self.last_schedule_occurrence_key = Some(active_occurrence_key.clone());
            self.handle_schedule_window_start(&active_occurrence_key);
        } else if self.focus_session_active_for_current_state() {
            self.schedule_armed_occurrence_key = None;
        }
    }

    fn schedule_delay_until_for_occurrence_key(
        &self,
        occurrence_key: &str,
        now: DateTime<Local>,
    ) -> Option<DateTime<Local>> {
        match (
            self.schedule_delayed_occurrence_key.as_deref(),
            self.schedule_delay_until,
        ) {
            (Some(delayed_key), Some(delayed_until))
                if delayed_key == occurrence_key && delayed_until > now =>
            {
                Some(delayed_until)
            }
            _ => None,
        }
    }

    pub(super) fn clear_schedule_delay_state(&mut self) {
        self.schedule_delayed_occurrence_key = None;
        self.schedule_delay_until = None;
    }

    fn sync_schedule_delay_state_for_occurrence(
        &mut self,
        active_occurrence_key: &str,
        now: DateTime<Local>,
    ) -> bool {
        if self
            .schedule_delay_until_for_occurrence_key(active_occurrence_key, now)
            .is_some()
        {
            return true;
        }
        self.clear_schedule_delay_state();
        false
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

        if self.can_auto_start_focus_for_schedule() {
            self.update_timer_and_sync(TimerState::toggle_pause);
            self.phase_notification =
                Some("Scheduled window started. Focus auto-started.".to_string());
            self.schedule_armed_occurrence_key = None;
            return;
        }

        self.schedule_armed_occurrence_key = Some(active_occurrence_key.to_string());
        self.phase_notification = Some(self.schedule_arm_notification());
    }

    fn can_auto_start_focus_for_schedule(&self) -> bool {
        self.timer.phase == TimerPhase::Focus
            && self.timer.status == TimerStatus::Idle
            && self.has_selectable_task_label_for_focus()
    }

    fn schedule_arm_notification(&self) -> String {
        if !self.has_selectable_task_label_for_focus() {
            format!(
                "Scheduled window started. Select a task label with {}, then press {} to start focus or {} to delay 10m.",
                self.shortcut_hint(ShortcutAction::OpenSessionPlanner),
                self.shortcut_hint(ShortcutAction::TimerTogglePause),
                self.shortcut_hint(ShortcutAction::DelayScheduleStart),
            )
        } else {
            format!(
                "Scheduled window started. Press {} to start focus or {} to delay 10m.",
                self.shortcut_hint(ShortcutAction::TimerTogglePause),
                self.shortcut_hint(ShortcutAction::DelayScheduleStart),
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

    if let Some(delayed_until) = state.delayed_until.as_ref() {
        return schedule_delayed_status_text(*delayed_until, state.has_selected_task, labels);
    }

    if state.active_window.is_some() {
        return schedule_active_window_status_text(state, labels);
    }

    if state.is_armed {
        return schedule_armed_status_text(state.has_selected_task, labels);
    }

    if state.is_exception_today {
        return "⚙  Schedule status: skipped today (exception date)".to_string();
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

fn schedule_delayed_status_text(
    delayed_until: DateTime<Local>,
    has_selected_task: bool,
    labels: &ScheduleShortcutLabels,
) -> String {
    if has_selected_task {
        format!(
            "⚙  Schedule status: delayed until {}; press {} to start now or {} to delay 10m",
            delayed_until.format("%H:%M"),
            labels.toggle_pause,
            labels.delay,
        )
    } else {
        format!(
            "⚙  Schedule status: delayed until {}; select {} then {}, or press {} to delay 10m",
            delayed_until.format("%H:%M"),
            labels.planner,
            labels.toggle_pause,
            labels.delay,
        )
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
