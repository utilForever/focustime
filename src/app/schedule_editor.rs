use crate::app::{
    App, OneTimeFocusWindowConfig, PROFILE_EDIT_AUTOMATION_TRIGGER_ACTION_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_ADD_REMOVE_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_BLOCKLIST_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_CONDITION_INDEX, PROFILE_EDIT_AUTOMATION_TRIGGER_DELAY_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_INDEX, PROFILE_EDIT_AUTOMATION_TRIGGER_PROFILE_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_TEMPLATE_INDEX, PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_AT_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_DAY_INDEX, PROFILE_EDIT_ONE_TIME_ADD_REMOVE_INDEX,
    PROFILE_EDIT_ONE_TIME_DATE_INDEX, PROFILE_EDIT_ONE_TIME_END_INDEX,
    PROFILE_EDIT_ONE_TIME_START_INDEX, PROFILE_EDIT_ONE_TIME_WINDOW_INDEX,
    PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX, PROFILE_EDIT_SCHEDULE_CONFLICTS_INDEX,
    PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX, PROFILE_EDIT_SCHEDULE_DAY_INDEX,
    PROFILE_EDIT_SCHEDULE_END_INDEX, PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX,
    PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX, PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX,
    PROFILE_EDIT_SCHEDULE_START_INDEX, PROFILE_EDIT_SCHEDULE_WINDOW_INDEX, PROFILE_IDS,
    RecurringFocusWindowConfig, SCHEDULE_DAY_LABELS, SCHEDULE_DAY_TOKENS, bool_label, format_hhmm,
    format_schedule_conflict, format_schedule_days_for_display,
    inspect_schedule_conflicts_from_config, parse_hhmm_minutes, parse_schedule_exception_date,
    profile_for_index, profile_index, sort_one_time_windows, sort_schedule_days,
    sort_schedule_exception_dates,
};
use crate::config::{
    AutomationTriggerActionConfig, AutomationTriggerConditionConfig, AutomationTriggerRuleConfig,
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
            PROFILE_EDIT_AUTOMATION_TRIGGER_INDEX => self.automation_trigger_selector_value(),
            PROFILE_EDIT_AUTOMATION_TRIGGER_CONDITION_INDEX => {
                self.automation_trigger_condition_value()
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_DAY_INDEX => {
                self.automation_trigger_time_day_value()
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_AT_INDEX => {
                self.automation_trigger_time_at_value()
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_ACTION_INDEX => self.automation_trigger_action_value(),
            PROFILE_EDIT_AUTOMATION_TRIGGER_PROFILE_INDEX => {
                self.automation_trigger_profile_value()
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_BLOCKLIST_INDEX => {
                self.automation_trigger_blocklist_value()
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_TEMPLATE_INDEX => {
                self.automation_trigger_template_value()
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_DELAY_INDEX => self.automation_trigger_delay_value(),
            PROFILE_EDIT_AUTOMATION_TRIGGER_ADD_REMOVE_INDEX => {
                self.automation_trigger_collection_value()
            }
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

    fn automation_trigger_selector_value(&self) -> String {
        if self.automation_trigger_rules_for_edit().is_empty() {
            "none".to_string()
        } else {
            format!(
                "{}/{}",
                self.profile_edit_automation_trigger.saturating_add(1),
                self.automation_trigger_rules_for_edit().len()
            )
        }
    }

    fn automation_trigger_condition_value(&self) -> String {
        match self.selected_automation_trigger().map(|rule| &rule.trigger) {
            Some(AutomationTriggerConditionConfig::ScheduleWindowStart) => {
                "schedule_window_start".to_string()
            }
            Some(AutomationTriggerConditionConfig::ScheduleWindowEnd) => {
                "schedule_window_end".to_string()
            }
            Some(AutomationTriggerConditionConfig::FocusStarted) => "focus_started".to_string(),
            Some(AutomationTriggerConditionConfig::FocusCompleted) => "focus_completed".to_string(),
            Some(AutomationTriggerConditionConfig::BreakStarted) => "break_started".to_string(),
            Some(AutomationTriggerConditionConfig::BreakCompleted) => "break_completed".to_string(),
            Some(AutomationTriggerConditionConfig::Time { .. }) => "time".to_string(),
            None => "n/a".to_string(),
        }
    }

    fn automation_trigger_time_day_value(&self) -> String {
        let Some(AutomationTriggerConditionConfig::Time { days, .. }) =
            self.selected_automation_trigger().map(|rule| &rule.trigger)
        else {
            return "n/a".to_string();
        };
        match days.len() {
            0 => "n/a".to_string(),
            1 => days[0].clone(),
            _ => days.join(","),
        }
    }

    fn automation_trigger_time_at_value(&self) -> String {
        let Some(AutomationTriggerConditionConfig::Time { at, .. }) =
            self.selected_automation_trigger().map(|rule| &rule.trigger)
        else {
            return "n/a".to_string();
        };
        at.clone()
    }

    fn automation_trigger_action_value(&self) -> String {
        match self.selected_automation_trigger().map(|rule| &rule.action) {
            Some(AutomationTriggerActionConfig::StartFocus) => "start_focus".to_string(),
            Some(AutomationTriggerActionConfig::DelayScheduleStart { .. }) => {
                "delay_schedule_start".to_string()
            }
            Some(AutomationTriggerActionConfig::ApplyDefaults { .. }) => {
                "apply_defaults".to_string()
            }
            None => "n/a".to_string(),
        }
    }

    fn automation_trigger_profile_value(&self) -> String {
        let Some(AutomationTriggerActionConfig::ApplyDefaults { profile, .. }) =
            self.selected_automation_trigger().map(|rule| &rule.action)
        else {
            return "n/a".to_string();
        };
        profile.label().to_string()
    }

    fn automation_trigger_blocklist_value(&self) -> String {
        let Some(AutomationTriggerActionConfig::ApplyDefaults {
            blocklist_profile, ..
        }) = self.selected_automation_trigger().map(|rule| &rule.action)
        else {
            return "n/a".to_string();
        };
        blocklist_profile.clone()
    }

    fn automation_trigger_template_value(&self) -> String {
        let Some(AutomationTriggerActionConfig::ApplyDefaults {
            session_template, ..
        }) = self.selected_automation_trigger().map(|rule| &rule.action)
        else {
            return "n/a".to_string();
        };
        session_template
            .clone()
            .unwrap_or_else(|| "none".to_string())
    }

    fn automation_trigger_delay_value(&self) -> String {
        let Some(AutomationTriggerActionConfig::DelayScheduleStart { delay_secs }) =
            self.selected_automation_trigger().map(|rule| &rule.action)
        else {
            return "n/a".to_string();
        };
        format!("{delay_secs}s")
    }

    fn automation_trigger_collection_value(&self) -> String {
        if self.automation_trigger_rules_for_edit().is_empty() {
            "→ Add rule".to_string()
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

    fn automation_trigger_rules_for_edit(&self) -> &[AutomationTriggerRuleConfig] {
        if self.profile_edit_active {
            &self.profile_edit_automation_triggers
        } else {
            &self.automation_triggers
        }
    }

    fn automation_trigger_rules_for_edit_mut(&mut self) -> &mut Vec<AutomationTriggerRuleConfig> {
        if self.profile_edit_active {
            &mut self.profile_edit_automation_triggers
        } else {
            &mut self.automation_triggers
        }
    }

    fn selected_automation_trigger(&self) -> Option<&AutomationTriggerRuleConfig> {
        self.automation_trigger_rules_for_edit()
            .get(self.profile_edit_automation_trigger)
    }

    fn selected_automation_trigger_mut(&mut self) -> Option<&mut AutomationTriggerRuleConfig> {
        let index = self.profile_edit_automation_trigger;
        self.automation_trigger_rules_for_edit_mut().get_mut(index)
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
        if self.automation_trigger_rules_for_edit().is_empty() {
            self.profile_edit_automation_trigger = 0;
        } else {
            self.profile_edit_automation_trigger = self.profile_edit_automation_trigger.min(
                self.automation_trigger_rules_for_edit()
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

    pub(super) fn adjust_selected_schedule_exception_date(&mut self, increase: bool) {
        let selected_index = self.profile_edit_schedule_exception;
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
        if let Some(existing_position) = self
            .recurring_schedule
            .exception_dates
            .iter()
            .position(|value| value == &next_value)
        {
            if existing_position != selected_index {
                self.recurring_schedule
                    .exception_dates
                    .remove(selected_index);
                self.profile_edit_schedule_exception =
                    adjusted_index_after_removal(selected_index, existing_position);
            }
            return;
        }
        if let Some(target) = self
            .recurring_schedule
            .exception_dates
            .get_mut(selected_index)
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
        let selected_index = self.profile_edit_one_time_window;
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
        if let Some(existing_position) =
            self.recurring_schedule
                .one_time_windows
                .iter()
                .position(|candidate| {
                    candidate.date == updated.date
                        && candidate.start == updated.start
                        && candidate.end == updated.end
                })
        {
            if existing_position != selected_index {
                self.recurring_schedule
                    .one_time_windows
                    .remove(selected_index);
                self.profile_edit_one_time_window =
                    adjusted_index_after_removal(selected_index, existing_position);
            }
            return;
        }
        if let Some(target) = self
            .recurring_schedule
            .one_time_windows
            .get_mut(selected_index)
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
        let selected_index = self.profile_edit_one_time_window;
        let Some(current_window) = self.selected_one_time_window().cloned() else {
            return;
        };
        let step_minutes = self.schedule_runtime.time_step_minutes;

        let mut start = parse_hhmm_minutes(&current_window.start).unwrap_or(9 * 60);
        let mut end = parse_hhmm_minutes(&current_window.end).unwrap_or(10 * 60);
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

        let updated = OneTimeFocusWindowConfig {
            date: current_window.date,
            start: format_hhmm(start),
            end: format_hhmm(end),
        };
        if let Some(existing_position) =
            self.recurring_schedule
                .one_time_windows
                .iter()
                .position(|candidate| {
                    candidate.date == updated.date
                        && candidate.start == updated.start
                        && candidate.end == updated.end
                })
        {
            if existing_position != selected_index {
                self.recurring_schedule
                    .one_time_windows
                    .remove(selected_index);
                self.profile_edit_one_time_window =
                    adjusted_index_after_removal(selected_index, existing_position);
            }
            return;
        }
        if let Some(target) = self
            .recurring_schedule
            .one_time_windows
            .get_mut(selected_index)
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

    pub(super) fn cycle_automation_trigger_rule(&mut self, increase: bool) {
        if self.automation_trigger_rules_for_edit().is_empty() {
            return;
        }
        let total = self.automation_trigger_rules_for_edit().len();
        if increase {
            self.profile_edit_automation_trigger =
                (self.profile_edit_automation_trigger + 1) % total;
        } else if self.profile_edit_automation_trigger == 0 {
            self.profile_edit_automation_trigger = total - 1;
        } else {
            self.profile_edit_automation_trigger =
                self.profile_edit_automation_trigger.saturating_sub(1);
        }
    }

    pub(super) fn cycle_automation_trigger_condition(&mut self, increase: bool) {
        let Some(current) = self
            .selected_automation_trigger()
            .map(|rule| rule.trigger.clone())
        else {
            return;
        };
        let conditions = [
            AutomationTriggerConditionConfig::ScheduleWindowStart,
            AutomationTriggerConditionConfig::ScheduleWindowEnd,
            AutomationTriggerConditionConfig::FocusStarted,
            AutomationTriggerConditionConfig::FocusCompleted,
            AutomationTriggerConditionConfig::BreakStarted,
            AutomationTriggerConditionConfig::BreakCompleted,
            AutomationTriggerConditionConfig::Time {
                days: vec![SCHEDULE_DAY_TOKENS[0].to_string()],
                at: "09:00".to_string(),
            },
        ];
        let current_index = conditions
            .iter()
            .position(|candidate| {
                std::mem::discriminant(candidate) == std::mem::discriminant(&current)
            })
            .unwrap_or(0);
        let next_index = if increase {
            (current_index + 1) % conditions.len()
        } else if current_index == 0 {
            conditions.len() - 1
        } else {
            current_index - 1
        };
        let next = match &conditions[next_index] {
            AutomationTriggerConditionConfig::Time { .. } => {
                if let AutomationTriggerConditionConfig::Time { days, at } = current {
                    AutomationTriggerConditionConfig::Time { days, at }
                } else {
                    AutomationTriggerConditionConfig::Time {
                        days: vec![SCHEDULE_DAY_TOKENS[0].to_string()],
                        at: "09:00".to_string(),
                    }
                }
            }
            value => value.clone(),
        };
        if let Some(rule) = self.selected_automation_trigger_mut() {
            rule.trigger = next;
        }
    }

    pub(super) fn cycle_automation_trigger_time_day(&mut self, increase: bool) {
        let Some(AutomationTriggerConditionConfig::Time { days, .. }) = self
            .selected_automation_trigger()
            .map(|rule| rule.trigger.clone())
        else {
            return;
        };
        if days.len() != 1 {
            return;
        }
        let current_day = days[0].clone();
        let current_index = SCHEDULE_DAY_TOKENS
            .iter()
            .position(|token| token.eq_ignore_ascii_case(current_day.as_str()))
            .unwrap_or(0);
        let total = SCHEDULE_DAY_TOKENS.len();
        let next_index = if increase {
            (current_index + 1) % total
        } else if current_index == 0 {
            total - 1
        } else {
            current_index - 1
        };
        if let Some(rule) = self.selected_automation_trigger_mut()
            && let AutomationTriggerConditionConfig::Time { days, .. } = &mut rule.trigger
            && days.len() == 1
        {
            days[0] = SCHEDULE_DAY_TOKENS[next_index].to_string();
        }
    }

    pub(super) fn adjust_automation_trigger_time_at(&mut self, increase: bool) {
        let step_minutes = self.schedule_runtime.time_step_minutes;
        if let Some(rule) = self.selected_automation_trigger_mut()
            && let AutomationTriggerConditionConfig::Time { at, .. } = &mut rule.trigger
        {
            let current = parse_hhmm_minutes(at).unwrap_or(9 * 60);
            let next = if increase {
                current.saturating_add(step_minutes).min(23 * 60 + 59)
            } else {
                current.saturating_sub(step_minutes)
            };
            *at = format_hhmm(next);
        }
    }

    pub(super) fn cycle_automation_trigger_action(&mut self, increase: bool) {
        let Some(current) = self
            .selected_automation_trigger()
            .map(|rule| rule.action.clone())
        else {
            return;
        };
        let actions = [
            AutomationTriggerActionConfig::StartFocus,
            AutomationTriggerActionConfig::DelayScheduleStart {
                delay_secs: self.schedule_runtime.delay_secs,
            },
            AutomationTriggerActionConfig::ApplyDefaults {
                profile: self.selected_profile,
                blocklist_profile: self.active_blocklist_profile_name().to_string(),
                session_template: self.active_session_template_name().map(ToString::to_string),
            },
        ];
        let current_index = actions
            .iter()
            .position(|candidate| {
                std::mem::discriminant(candidate) == std::mem::discriminant(&current)
            })
            .unwrap_or(0);
        let next_index = if increase {
            (current_index + 1) % actions.len()
        } else if current_index == 0 {
            actions.len() - 1
        } else {
            current_index - 1
        };
        let next = match &actions[next_index] {
            AutomationTriggerActionConfig::DelayScheduleStart { .. } => {
                if let AutomationTriggerActionConfig::DelayScheduleStart { delay_secs } = current {
                    AutomationTriggerActionConfig::DelayScheduleStart { delay_secs }
                } else {
                    AutomationTriggerActionConfig::DelayScheduleStart {
                        delay_secs: self.schedule_runtime.delay_secs,
                    }
                }
            }
            AutomationTriggerActionConfig::ApplyDefaults { .. } => {
                if let AutomationTriggerActionConfig::ApplyDefaults {
                    profile,
                    blocklist_profile,
                    session_template,
                } = current
                {
                    AutomationTriggerActionConfig::ApplyDefaults {
                        profile,
                        blocklist_profile,
                        session_template,
                    }
                } else {
                    AutomationTriggerActionConfig::ApplyDefaults {
                        profile: self.selected_profile,
                        blocklist_profile: self.active_blocklist_profile_name().to_string(),
                        session_template: self
                            .active_session_template_name()
                            .map(ToString::to_string),
                    }
                }
            }
            value => value.clone(),
        };
        if let Some(rule) = self.selected_automation_trigger_mut() {
            rule.action = next;
        }
    }

    pub(super) fn cycle_automation_trigger_profile(&mut self, increase: bool) {
        let Some(AutomationTriggerActionConfig::ApplyDefaults { profile, .. }) = self
            .selected_automation_trigger()
            .map(|rule| rule.action.clone())
        else {
            return;
        };
        let current_index = profile_index(profile);
        let total = PROFILE_IDS.len();
        let next_index = if increase {
            (current_index + 1) % total
        } else if current_index == 0 {
            total - 1
        } else {
            current_index - 1
        };
        if let Some(rule) = self.selected_automation_trigger_mut()
            && let AutomationTriggerActionConfig::ApplyDefaults { profile, .. } = &mut rule.action
        {
            *profile = profile_for_index(next_index);
        }
    }

    pub(super) fn cycle_automation_trigger_blocklist(&mut self, increase: bool) {
        if self.blocklist_profiles.is_empty() {
            return;
        }
        let Some(AutomationTriggerActionConfig::ApplyDefaults {
            blocklist_profile, ..
        }) = self
            .selected_automation_trigger()
            .map(|rule| rule.action.clone())
        else {
            return;
        };
        let current_index = self
            .blocklist_profiles
            .iter()
            .position(|profile| {
                profile
                    .name
                    .eq_ignore_ascii_case(blocklist_profile.as_str())
            })
            .unwrap_or(0);
        let total = self.blocklist_profiles.len();
        let next_index = if increase {
            (current_index + 1) % total
        } else if current_index == 0 {
            total - 1
        } else {
            current_index - 1
        };
        let next_name = self.blocklist_profiles[next_index].name.clone();
        if let Some(rule) = self.selected_automation_trigger_mut()
            && let AutomationTriggerActionConfig::ApplyDefaults {
                blocklist_profile, ..
            } = &mut rule.action
        {
            *blocklist_profile = next_name;
        }
    }

    pub(super) fn cycle_automation_trigger_template(&mut self, increase: bool) {
        let Some(AutomationTriggerActionConfig::ApplyDefaults {
            session_template, ..
        }) = self
            .selected_automation_trigger()
            .map(|rule| rule.action.clone())
        else {
            return;
        };
        if self.session_templates.is_empty() {
            if let Some(rule) = self.selected_automation_trigger_mut()
                && let AutomationTriggerActionConfig::ApplyDefaults {
                    session_template, ..
                } = &mut rule.action
            {
                *session_template = None;
            }
            return;
        }
        let none_index = self.session_templates.len();
        let current_index = session_template
            .as_deref()
            .and_then(|name| self.session_template_index_by_name(name))
            .unwrap_or(none_index);
        let total = none_index + 1;
        let next_index = if increase {
            (current_index + 1) % total
        } else if current_index == 0 {
            total - 1
        } else {
            current_index - 1
        };
        let next_template = if next_index == none_index {
            None
        } else {
            self.session_templates
                .get(next_index)
                .map(|template| template.name.clone())
        };
        if let Some(rule) = self.selected_automation_trigger_mut()
            && let AutomationTriggerActionConfig::ApplyDefaults {
                session_template, ..
            } = &mut rule.action
        {
            *session_template = next_template;
        }
    }

    pub(super) fn adjust_automation_trigger_delay(&mut self, increase: bool) {
        if let Some(rule) = self.selected_automation_trigger_mut()
            && let AutomationTriggerActionConfig::DelayScheduleStart { delay_secs } =
                &mut rule.action
        {
            if increase {
                *delay_secs = delay_secs.saturating_add(30);
            } else {
                *delay_secs = delay_secs.saturating_sub(30).max(1);
            }
        }
    }

    pub(super) fn adjust_automation_triggers_collection(&mut self, increase: bool) {
        if increase {
            self.automation_trigger_rules_for_edit_mut()
                .push(AutomationTriggerRuleConfig::default());
            self.profile_edit_automation_trigger = self
                .automation_trigger_rules_for_edit()
                .len()
                .saturating_sub(1);
            return;
        }
        if self.automation_trigger_rules_for_edit().is_empty() {
            return;
        }
        let index = self.profile_edit_automation_trigger;
        self.automation_trigger_rules_for_edit_mut().remove(index);
        self.clamp_profile_edit_schedule_selection();
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

fn adjusted_index_after_removal(removed_index: usize, target_index: usize) -> usize {
    if removed_index < target_index {
        target_index.saturating_sub(1)
    } else {
        target_index
    }
}
