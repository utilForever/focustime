use crate::app::{
    App, PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX, PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX,
    PROFILE_EDIT_SCHEDULE_DAY_INDEX, PROFILE_EDIT_SCHEDULE_END_INDEX,
    PROFILE_EDIT_SCHEDULE_START_INDEX, PROFILE_EDIT_SCHEDULE_WINDOW_INDEX,
    RecurringFocusWindowConfig, SCHEDULE_DAY_LABELS, SCHEDULE_DAY_TOKENS, bool_label, format_hhmm,
    format_schedule_days_for_display, parse_hhmm_minutes, sort_schedule_days,
};

impl App {
    pub(super) fn profile_edit_schedule_field_value(&self, field_index: usize) -> String {
        match field_index {
            PROFILE_EDIT_SCHEDULE_WINDOW_INDEX => self.schedule_window_selector_value(),
            PROFILE_EDIT_SCHEDULE_DAY_INDEX => self.schedule_day_selector_value(),
            PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX => self
                .selected_schedule_day_enabled()
                .map(|enabled| bool_label(enabled).to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_SCHEDULE_START_INDEX => self
                .selected_schedule_window()
                .map(|window| window.start.clone())
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_SCHEDULE_END_INDEX => self
                .selected_schedule_window()
                .map(|window| window.end.clone())
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX => self.schedule_window_collection_value(),
            _ => String::new(),
        }
    }

    fn schedule_window_selector_value(&self) -> String {
        if self.recurring_schedule.windows.is_empty() {
            "none".to_string()
        } else {
            format!(
                "{}/{}",
                self.profile_edit_schedule_window.saturating_add(1),
                self.recurring_schedule.windows.len()
            )
        }
    }

    fn schedule_day_selector_value(&self) -> String {
        if let Some(window) = self.selected_schedule_window() {
            let day_label = self.selected_schedule_day_label();
            let days = format_schedule_days_for_display(&window.days);
            format!("{day_label} ({days})")
        } else {
            "n/a".to_string()
        }
    }

    fn schedule_window_collection_value(&self) -> String {
        if self.recurring_schedule.windows.is_empty() {
            "→ Add window".to_string()
        } else {
            "← Remove · → Add".to_string()
        }
    }

    fn selected_schedule_window(&self) -> Option<&RecurringFocusWindowConfig> {
        self.recurring_schedule
            .windows
            .get(self.profile_edit_schedule_window)
    }

    fn selected_schedule_window_mut(&mut self) -> Option<&mut RecurringFocusWindowConfig> {
        self.recurring_schedule
            .windows
            .get_mut(self.profile_edit_schedule_window)
    }

    fn selected_schedule_day_token(&self) -> &'static str {
        SCHEDULE_DAY_TOKENS[self
            .profile_edit_schedule_day
            .min(SCHEDULE_DAY_TOKENS.len() - 1)]
    }

    fn selected_schedule_day_label(&self) -> &'static str {
        SCHEDULE_DAY_LABELS[self
            .profile_edit_schedule_day
            .min(SCHEDULE_DAY_LABELS.len() - 1)]
    }

    fn selected_schedule_day_enabled(&self) -> Option<bool> {
        let day = self.selected_schedule_day_token();
        self.selected_schedule_window().map(|window| {
            window
                .days
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(day))
        })
    }

    pub(super) fn clamp_profile_edit_schedule_selection(&mut self) {
        if self.recurring_schedule.windows.is_empty() {
            self.profile_edit_schedule_window = 0;
        } else {
            self.profile_edit_schedule_window = self
                .profile_edit_schedule_window
                .min(self.recurring_schedule.windows.len().saturating_sub(1));
        }
        self.profile_edit_schedule_day = self
            .profile_edit_schedule_day
            .min(SCHEDULE_DAY_TOKENS.len().saturating_sub(1));
    }

    pub(super) fn cycle_schedule_window(&mut self, increase: bool) {
        if self.recurring_schedule.windows.is_empty() {
            return;
        }
        let total = self.recurring_schedule.windows.len();
        if increase {
            self.profile_edit_schedule_window = (self.profile_edit_schedule_window + 1) % total;
        } else if self.profile_edit_schedule_window == 0 {
            self.profile_edit_schedule_window = total - 1;
        } else {
            self.profile_edit_schedule_window = self.profile_edit_schedule_window.saturating_sub(1);
        }
    }

    pub(super) fn cycle_schedule_day(&mut self, increase: bool) {
        let total = SCHEDULE_DAY_TOKENS.len();
        if increase {
            self.profile_edit_schedule_day = (self.profile_edit_schedule_day + 1) % total;
        } else if self.profile_edit_schedule_day == 0 {
            self.profile_edit_schedule_day = total - 1;
        } else {
            self.profile_edit_schedule_day = self.profile_edit_schedule_day.saturating_sub(1);
        }
    }

    pub(super) fn set_schedule_day_enabled(&mut self, enabled: bool) {
        let selected_day = self.selected_schedule_day_token().to_string();
        let Some(window) = self.selected_schedule_window_mut() else {
            return;
        };

        let currently_enabled = window
            .days
            .iter()
            .any(|day| day.eq_ignore_ascii_case(&selected_day));
        if enabled == currently_enabled {
            return;
        }

        if enabled {
            window.days.push(selected_day);
            sort_schedule_days(&mut window.days);
            return;
        }

        if window.days.len() <= 1 {
            return;
        }
        window
            .days
            .retain(|day| !day.eq_ignore_ascii_case(&selected_day));
        sort_schedule_days(&mut window.days);
    }

    pub(super) fn adjust_selected_schedule_time(&mut self, is_start: bool, increase: bool) {
        let step_minutes = self.schedule_runtime.time_step_minutes;
        let Some(window) = self.selected_schedule_window_mut() else {
            return;
        };

        let mut start = parse_hhmm_minutes(&window.start).unwrap_or(9 * 60);
        let mut end = parse_hhmm_minutes(&window.end).unwrap_or(10 * 60);
        if end <= start {
            end = start.saturating_add(1).min(23 * 60 + 59);
        }

        if is_start {
            if increase {
                start = start
                    .saturating_add(step_minutes)
                    .min(end.saturating_sub(1));
            } else {
                start = start.saturating_sub(step_minutes);
            }
        } else if increase {
            end = end
                .saturating_add(step_minutes)
                .min(23 * 60 + 59)
                .max(start.saturating_add(1));
        } else {
            end = end
                .saturating_sub(step_minutes)
                .max(start.saturating_add(1));
        }

        window.start = format_hhmm(start);
        window.end = format_hhmm(end);
    }

    pub(super) fn adjust_schedule_windows_collection(&mut self, increase: bool) {
        if increase {
            self.recurring_schedule
                .windows
                .push(RecurringFocusWindowConfig::default());
            self.profile_edit_schedule_window = self.recurring_schedule.windows.len() - 1;
            return;
        }

        if self.recurring_schedule.windows.is_empty() {
            return;
        }
        self.recurring_schedule
            .windows
            .remove(self.profile_edit_schedule_window);
        self.clamp_profile_edit_schedule_selection();
    }
}
