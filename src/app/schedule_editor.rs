use crate::app::{
    App, OneTimeFocusWindowConfig, PROFILE_EDIT_ONE_TIME_ADD_REMOVE_INDEX,
    PROFILE_EDIT_ONE_TIME_DATE_INDEX, PROFILE_EDIT_ONE_TIME_END_INDEX,
    PROFILE_EDIT_ONE_TIME_START_INDEX, PROFILE_EDIT_ONE_TIME_WINDOW_INDEX,
    PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX, PROFILE_EDIT_SCHEDULE_CONFLICTS_INDEX,
    PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX, PROFILE_EDIT_SCHEDULE_DAY_INDEX,
    PROFILE_EDIT_SCHEDULE_END_INDEX, PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX,
    PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX, PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX,
    PROFILE_EDIT_SCHEDULE_START_INDEX, PROFILE_EDIT_SCHEDULE_WINDOW_INDEX,
    RecurringFocusWindowConfig, SCHEDULE_DAY_LABELS, SCHEDULE_DAY_TOKENS,
    SCHEDULE_TIME_STEP_MINUTES, bool_label, format_hhmm, format_schedule_conflict,
    format_schedule_days_for_display, inspect_schedule_conflicts_from_config, parse_hhmm_minutes,
    parse_schedule_exception_date, sort_one_time_windows, sort_schedule_days,
    sort_schedule_exception_dates,
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
            PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX => self.schedule_exception_selector_value(),
            PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX => self
                .selected_schedule_exception_date()
                .cloned()
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX => {
                self.schedule_exception_collection_value()
            }
            PROFILE_EDIT_ONE_TIME_WINDOW_INDEX => self.one_time_window_selector_value(),
            PROFILE_EDIT_ONE_TIME_DATE_INDEX => self
                .selected_one_time_window()
                .map(|window| window.date.clone())
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_ONE_TIME_START_INDEX => self
                .selected_one_time_window()
                .map(|window| window.start.clone())
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_ONE_TIME_END_INDEX => self
                .selected_one_time_window()
                .map(|window| window.end.clone())
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_ONE_TIME_ADD_REMOVE_INDEX => self.one_time_window_collection_value(),
            PROFILE_EDIT_SCHEDULE_CONFLICTS_INDEX => self.schedule_conflict_summary_value(),
            _ => String::new(),
        }
    }

    fn schedule_conflict_messages(&self) -> Vec<String> {
        inspect_schedule_conflicts_from_config(&self.recurring_schedule)
            .into_iter()
            .map(|conflict| format_schedule_conflict(&conflict))
            .collect()
    }

    fn schedule_conflict_summary_value(&self) -> String {
        let conflicts = self.schedule_conflict_messages();
        if conflicts.is_empty() {
            return "none detected".to_string();
        }

        format!("{} detected · {}", conflicts.len(), conflicts[0])
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

    fn schedule_exception_selector_value(&self) -> String {
        if self.recurring_schedule.exception_dates.is_empty() {
            "none".to_string()
        } else {
            format!(
                "{}/{}",
                self.profile_edit_schedule_exception.saturating_add(1),
                self.recurring_schedule.exception_dates.len()
            )
        }
    }

    fn schedule_exception_collection_value(&self) -> String {
        if self.recurring_schedule.exception_dates.is_empty() {
            "→ Add date".to_string()
        } else {
            "← Remove · → Add".to_string()
        }
    }

    fn one_time_window_selector_value(&self) -> String {
        if self.recurring_schedule.one_time_windows.is_empty() {
            "none".to_string()
        } else {
            format!(
                "{}/{}",
                self.profile_edit_one_time_window.saturating_add(1),
                self.recurring_schedule.one_time_windows.len()
            )
        }
    }

    fn one_time_window_collection_value(&self) -> String {
        if self.recurring_schedule.one_time_windows.is_empty() {
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

    fn selected_schedule_exception_date(&self) -> Option<&String> {
        self.recurring_schedule
            .exception_dates
            .get(self.profile_edit_schedule_exception)
    }

    fn selected_one_time_window(&self) -> Option<&OneTimeFocusWindowConfig> {
        self.recurring_schedule
            .one_time_windows
            .get(self.profile_edit_one_time_window)
    }

    fn selected_one_time_window_mut(&mut self) -> Option<&mut OneTimeFocusWindowConfig> {
        self.recurring_schedule
            .one_time_windows
            .get_mut(self.profile_edit_one_time_window)
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
        if self.recurring_schedule.exception_dates.is_empty() {
            self.profile_edit_schedule_exception = 0;
        } else {
            self.profile_edit_schedule_exception = self.profile_edit_schedule_exception.min(
                self.recurring_schedule
                    .exception_dates
                    .len()
                    .saturating_sub(1),
            );
        }
        if self.recurring_schedule.one_time_windows.is_empty() {
            self.profile_edit_one_time_window = 0;
        } else {
            self.profile_edit_one_time_window = self.profile_edit_one_time_window.min(
                self.recurring_schedule
                    .one_time_windows
                    .len()
                    .saturating_sub(1),
            );
        }
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

    pub(super) fn cycle_schedule_exception(&mut self, increase: bool) {
        if self.recurring_schedule.exception_dates.is_empty() {
            return;
        }
        let total = self.recurring_schedule.exception_dates.len();
        if increase {
            self.profile_edit_schedule_exception =
                (self.profile_edit_schedule_exception + 1) % total;
        } else if self.profile_edit_schedule_exception == 0 {
            self.profile_edit_schedule_exception = total - 1;
        } else {
            self.profile_edit_schedule_exception =
                self.profile_edit_schedule_exception.saturating_sub(1);
        }
    }

    pub(super) fn cycle_one_time_window(&mut self, increase: bool) {
        if self.recurring_schedule.one_time_windows.is_empty() {
            return;
        }
        let total = self.recurring_schedule.one_time_windows.len();
        if increase {
            self.profile_edit_one_time_window = (self.profile_edit_one_time_window + 1) % total;
        } else if self.profile_edit_one_time_window == 0 {
            self.profile_edit_one_time_window = total - 1;
        } else {
            self.profile_edit_one_time_window = self.profile_edit_one_time_window.saturating_sub(1);
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
                    .saturating_add(SCHEDULE_TIME_STEP_MINUTES)
                    .min(end.saturating_sub(1));
            } else {
                start = start.saturating_sub(SCHEDULE_TIME_STEP_MINUTES);
            }
        } else if increase {
            end = end
                .saturating_add(SCHEDULE_TIME_STEP_MINUTES)
                .min(23 * 60 + 59)
                .max(start.saturating_add(1));
        } else {
            end = end
                .saturating_sub(SCHEDULE_TIME_STEP_MINUTES)
                .max(start.saturating_add(1));
        }

        window.start = format_hhmm(start);
        window.end = format_hhmm(end);
    }

    pub(super) fn adjust_selected_schedule_exception_date(&mut self, increase: bool) {
        let Some(current_value) = self.selected_schedule_exception_date().cloned() else {
            return;
        };
        let Some(current_date) = parse_schedule_exception_date(&current_value) else {
            return;
        };
        let next_date = if increase {
            current_date.succ_opt().unwrap_or(current_date)
        } else {
            current_date.pred_opt().unwrap_or(current_date)
        };
        let next_value = next_date.format("%Y-%m-%d").to_string();
        if let Some(target) = self
            .recurring_schedule
            .exception_dates
            .get_mut(self.profile_edit_schedule_exception)
        {
            *target = next_value.clone();
        }
        sort_schedule_exception_dates(&mut self.recurring_schedule.exception_dates);
        if let Some(position) = self
            .recurring_schedule
            .exception_dates
            .iter()
            .position(|value| value == &next_value)
        {
            self.profile_edit_schedule_exception = position;
        }
    }

    pub(super) fn adjust_selected_one_time_date(&mut self, increase: bool) {
        let Some(current_window) = self.selected_one_time_window().cloned() else {
            return;
        };
        let Some(current_date) = parse_schedule_exception_date(&current_window.date) else {
            return;
        };
        let next_date = if increase {
            current_date.succ_opt().unwrap_or(current_date)
        } else {
            current_date.pred_opt().unwrap_or(current_date)
        };
        let updated = OneTimeFocusWindowConfig {
            date: next_date.format("%Y-%m-%d").to_string(),
            start: current_window.start,
            end: current_window.end,
        };
        if let Some(target) = self
            .recurring_schedule
            .one_time_windows
            .get_mut(self.profile_edit_one_time_window)
        {
            *target = updated.clone();
        }
        sort_one_time_windows(&mut self.recurring_schedule.one_time_windows);
        if let Some(position) =
            self.recurring_schedule
                .one_time_windows
                .iter()
                .position(|candidate| {
                    candidate.date == updated.date
                        && candidate.start == updated.start
                        && candidate.end == updated.end
                })
        {
            self.profile_edit_one_time_window = position;
        }
    }

    pub(super) fn adjust_selected_one_time_time(&mut self, is_start: bool, increase: bool) {
        let updated = {
            let Some(window) = self.selected_one_time_window_mut() else {
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
                        .saturating_add(SCHEDULE_TIME_STEP_MINUTES)
                        .min(end.saturating_sub(1));
                } else {
                    start = start.saturating_sub(SCHEDULE_TIME_STEP_MINUTES);
                }
            } else if increase {
                end = end
                    .saturating_add(SCHEDULE_TIME_STEP_MINUTES)
                    .min(23 * 60 + 59)
                    .max(start.saturating_add(1));
            } else {
                end = end
                    .saturating_sub(SCHEDULE_TIME_STEP_MINUTES)
                    .max(start.saturating_add(1));
            }

            let updated = OneTimeFocusWindowConfig {
                date: window.date.clone(),
                start: format_hhmm(start),
                end: format_hhmm(end),
            };
            *window = updated.clone();
            updated
        };
        sort_one_time_windows(&mut self.recurring_schedule.one_time_windows);
        if let Some(position) =
            self.recurring_schedule
                .one_time_windows
                .iter()
                .position(|candidate| {
                    candidate.date == updated.date
                        && candidate.start == updated.start
                        && candidate.end == updated.end
                })
        {
            self.profile_edit_one_time_window = position;
        }
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

    pub(super) fn adjust_schedule_exceptions_collection(&mut self, increase: bool) {
        if increase {
            let mut candidate = self.current_frame_now.date_naive();
            let mut candidate_value = candidate.format("%Y-%m-%d").to_string();
            while self
                .recurring_schedule
                .exception_dates
                .iter()
                .any(|value| value == &candidate_value)
            {
                let Some(next_candidate) = candidate.succ_opt() else {
                    return;
                };
                candidate = next_candidate;
                candidate_value = candidate.format("%Y-%m-%d").to_string();
            }
            self.recurring_schedule
                .exception_dates
                .push(candidate_value.clone());
            sort_schedule_exception_dates(&mut self.recurring_schedule.exception_dates);
            self.profile_edit_schedule_exception = self
                .recurring_schedule
                .exception_dates
                .iter()
                .position(|value| value == &candidate_value)
                .unwrap_or(0);
            return;
        }

        if self.recurring_schedule.exception_dates.is_empty() {
            return;
        }
        self.recurring_schedule
            .exception_dates
            .remove(self.profile_edit_schedule_exception);
        self.clamp_profile_edit_schedule_selection();
    }

    pub(super) fn adjust_one_time_windows_collection(&mut self, increase: bool) {
        if increase {
            let default_window = OneTimeFocusWindowConfig::default();
            let mut candidate_date = self.current_frame_now.date_naive();
            let mut added = OneTimeFocusWindowConfig {
                date: candidate_date.format("%Y-%m-%d").to_string(),
                start: default_window.start.clone(),
                end: default_window.end.clone(),
            };
            while self
                .recurring_schedule
                .one_time_windows
                .iter()
                .any(|candidate| {
                    candidate.date == added.date
                        && candidate.start == added.start
                        && candidate.end == added.end
                })
            {
                let Some(next_date) = candidate_date.succ_opt() else {
                    return;
                };
                candidate_date = next_date;
                added.date = candidate_date.format("%Y-%m-%d").to_string();
            }
            self.recurring_schedule.one_time_windows.push(added.clone());
            sort_one_time_windows(&mut self.recurring_schedule.one_time_windows);
            self.profile_edit_one_time_window = self
                .recurring_schedule
                .one_time_windows
                .iter()
                .position(|candidate| {
                    candidate.date == added.date
                        && candidate.start == added.start
                        && candidate.end == added.end
                })
                .unwrap_or_else(|| {
                    self.recurring_schedule
                        .one_time_windows
                        .len()
                        .saturating_sub(1)
                });
            return;
        }

        if self.recurring_schedule.one_time_windows.is_empty() {
            return;
        }
        self.recurring_schedule
            .one_time_windows
            .remove(self.profile_edit_one_time_window);
        self.clamp_profile_edit_schedule_selection();
    }
}
