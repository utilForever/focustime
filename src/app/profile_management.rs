use crate::app::{
    App, AppMode, BreakTemplateConfig, KeyEvent, Local, NavigationAction,
    PROFILE_EDIT_AUTOMATION_TRIGGER_ACTION_INDEX, PROFILE_EDIT_AUTOMATION_TRIGGER_ADD_REMOVE_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_BLOCKLIST_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_CONDITION_INDEX, PROFILE_EDIT_AUTOMATION_TRIGGER_DELAY_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_INDEX, PROFILE_EDIT_AUTOMATION_TRIGGER_PROFILE_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_TEMPLATE_INDEX, PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_AT_INDEX,
    PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_DAY_INDEX, PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX,
    PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX, PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX,
    PROFILE_EDIT_FIELD_LABELS, PROFILE_EDIT_MONTHLY_GOAL_CARRY_OVER_INDEX,
    PROFILE_EDIT_MONTHLY_GOAL_MINUTES_INDEX, PROFILE_EDIT_MONTHLY_GOAL_POMODOROS_INDEX,
    PROFILE_EDIT_ONE_TIME_ADD_REMOVE_INDEX, PROFILE_EDIT_ONE_TIME_DATE_INDEX,
    PROFILE_EDIT_ONE_TIME_END_INDEX, PROFILE_EDIT_ONE_TIME_START_INDEX,
    PROFILE_EDIT_ONE_TIME_WINDOW_INDEX, PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX,
    PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX, PROFILE_EDIT_SCHEDULE_DAY_INDEX,
    PROFILE_EDIT_SCHEDULE_END_INDEX, PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX,
    PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX, PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX,
    PROFILE_EDIT_SCHEDULE_START_INDEX, PROFILE_EDIT_SCHEDULE_WINDOW_INDEX,
    PROFILE_EDIT_THEME_PRESET_INDEX, PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX,
    PROFILE_EDIT_WAKATIME_PROJECT_INDEX, PROFILE_EDIT_WEEKDAY_RULE_ADD_REMOVE_INDEX,
    PROFILE_EDIT_WEEKDAY_RULE_BLOCKLIST_INDEX, PROFILE_EDIT_WEEKDAY_RULE_DAY_INDEX,
    PROFILE_EDIT_WEEKDAY_RULE_INDEX, PROFILE_EDIT_WEEKDAY_RULE_PROFILE_INDEX,
    PROFILE_EDIT_WEEKDAY_RULE_TEMPLATE_INDEX, PROFILE_EDIT_WEEKLY_GOAL_CARRY_OVER_INDEX,
    PROFILE_EDIT_WEEKLY_GOAL_MINUTES_INDEX, PROFILE_EDIT_WEEKLY_GOAL_POMODOROS_INDEX, PROFILE_IDS,
    ProfileAutomationConfig, ProfileEditSnapshot, ProfileId, ShortcutAction, TimerState,
    WakatimeHeartbeatMetadata, adjust_daily_goal_minutes, adjust_daily_goal_pomodoros,
    adjust_duration_minutes, compile_exception_dates, compile_one_time_windows, compile_windows,
    profile_for_index, profile_index, profile_spec_for,
};
use crate::config::validate_automation_trigger_rules;

const PROFILE_MANAGER_SHORTCUT_ACTIONS: [ShortcutAction; 4] = [
    ShortcutAction::BackProfileManager,
    ShortcutAction::ProfileEdit,
    ShortcutAction::SelectPreviousBreakTemplate,
    ShortcutAction::SelectNextBreakTemplate,
];

impl App {
    pub(super) fn rebuild_recurring_schedule_runtime(&mut self) {
        self.recurring_windows = compile_windows(&self.recurring_schedule.windows);
        self.recurring_exception_dates =
            compile_exception_dates(&self.recurring_schedule.exception_dates);
        self.one_time_windows = compile_one_time_windows(&self.recurring_schedule.one_time_windows);
    }

    pub(super) fn selected_profile_automation(&self) -> ProfileAutomationConfig {
        ProfileAutomationConfig {
            notifications: self.notification_settings,
            auto_start: self.auto_start,
            strict_mode: self.strict_mode,
            recurring_schedule: self.recurring_schedule.clone(),
        }
        .normalized()
    }

    pub(super) fn update_selected_profile_automation(&mut self) {
        self.profile_automation
            .set_for_profile(self.selected_profile, self.selected_profile_automation());
    }

    pub(super) fn load_automation_runtime_for_profile(&mut self, profile: ProfileId) {
        let automation = self
            .profile_automation
            .for_profile(profile, &ProfileAutomationConfig::default());
        self.notification_settings = automation.notifications;
        self.auto_start = automation.auto_start;
        self.strict_mode = automation.strict_mode;
        self.recurring_schedule = automation.recurring_schedule;
        self.rebuild_notifier();
        self.rebuild_recurring_schedule_runtime();
        self.schedule_armed_occurrence_key = None;
        self.clear_schedule_delay_state();
        self.last_schedule_occurrence_key = None;
    }

    fn apply_automation_for_profile(&mut self, profile: ProfileId) {
        self.load_automation_runtime_for_profile(profile);
        let now = Local::now();
        self.current_frame_now = now;
        self.sync_recurring_schedule(now);
    }

    pub(super) fn apply_profile(&mut self, profile: ProfileId) -> bool {
        if self.strict_mode_enforced_for_focus() {
            self.config_error = Some(
                "strict focus active: finish focus before applying profile changes".to_string(),
            );
            return false;
        }
        let profile_spec = profile_spec_for(profile, &self.custom_profile);
        self.timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        self.active_focus_task_label = None;
        self.active_focus_intention = None;
        self.active_focus_task_note = None;
        self.active_focus_profile = None;
        self.selected_profile = profile;
        self.profile_selection_index = profile_index(profile);
        self.apply_automation_for_profile(profile);
        self.last_weekday_profile_sync_day = Some(Local::now().date_naive());
        self.pending_timer_action = None;
        self.save_config();
        self.apply_blocking_for_phase();
        self.sync_recovery_snapshot();
        true
    }

    fn clamp_profile_selection(&mut self) {
        if PROFILE_IDS.is_empty() {
            self.profile_selection_index = 0;
        } else {
            self.profile_selection_index = self
                .profile_selection_index
                .min(PROFILE_IDS.len().saturating_sub(1));
        }
    }

    pub(super) fn open_profile_manager(&mut self) {
        self.mode = AppMode::ProfileManager;
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.profile_edit_one_time_window = 0;
        self.profile_edit_weekday_rule = 0;
        self.profile_edit_automation_trigger = 0;
        self.profile_edit_automation_triggers.clear();
        self.profile_edit_snapshot = None;
        self.profile_selection_index = profile_index(self.selected_profile);
        self.clamp_profile_selection();
        self.clamp_profile_edit_schedule_selection();
        self.clamp_break_template_selection();
    }

    pub(super) fn exit_profile_manager(&mut self) {
        self.mode = AppMode::Timer;
        self.profile_edit_automation_triggers.clear();
        self.profile_edit_snapshot = None;
    }

    pub(super) fn handle_key_profile_manager(&mut self, key: KeyEvent) {
        if self.profile_edit_active {
            self.handle_key_profile_manager_edit_mode(&key);
            return;
        }

        self.handle_key_profile_manager_selection_mode(&key);
    }

    fn handle_key_profile_manager_edit_mode(&mut self, key: &KeyEvent) {
        if self.handle_quit_key(key, false) {
            return;
        }

        if self.handle_profile_edit_metadata_input(key) {
            return;
        }

        match key.code {
            _ if self.navigation_matches(NavigationAction::Cancel, key) => {
                self.cancel_profile_edit()
            }
            _ if self.navigation_matches(NavigationAction::MoveUp, key) => {
                self.profile_edit_field = self.profile_edit_field.saturating_sub(1);
            }
            _ if self.navigation_matches(NavigationAction::MoveDown, key) => {
                self.profile_edit_field = (self.profile_edit_field + 1)
                    .min(PROFILE_EDIT_FIELD_LABELS.len().saturating_sub(1));
            }
            _ if self.navigation_matches(NavigationAction::MoveLeft, key) => {
                self.adjust_profile_edit_field(false);
            }
            _ if self.navigation_matches(NavigationAction::MoveRight, key) => {
                self.adjust_profile_edit_field(true);
            }
            _ if self.navigation_matches(NavigationAction::Confirm, key) => {
                self.commit_profile_edit()
            }
            _ => {}
        }
    }

    fn handle_key_profile_manager_selection_mode(&mut self, key: &KeyEvent) {
        if self.handle_quit_key(key, false) {
            return;
        }

        match key.code {
            _ if self.navigation_matches(NavigationAction::Cancel, key) => {
                self.exit_profile_manager()
            }
            _ if self.navigation_matches(NavigationAction::MoveUp, key) => {
                self.profile_selection_index = self.profile_selection_index.saturating_sub(1);
            }
            _ if self.navigation_matches(NavigationAction::MoveDown, key) => {
                self.profile_selection_index =
                    (self.profile_selection_index + 1).min(PROFILE_IDS.len().saturating_sub(1));
            }
            _ if self.navigation_matches(NavigationAction::Confirm, key) => {
                let selected = profile_for_index(self.profile_selection_index);
                if self.apply_profile(selected) {
                    self.exit_profile_manager();
                }
            }
            _ => self.handle_profile_manager_shortcut_action(key),
        }
    }

    fn profile_manager_shortcut_action(&self, key: &KeyEvent) -> Option<ShortcutAction> {
        PROFILE_MANAGER_SHORTCUT_ACTIONS
            .into_iter()
            .find(|action| self.shortcut_matches(*action, key))
    }

    fn handle_profile_manager_shortcut_action(&mut self, key: &KeyEvent) {
        let Some(action) = self.profile_manager_shortcut_action(key) else {
            return;
        };

        match action {
            ShortcutAction::BackProfileManager => self.exit_profile_manager(),
            ShortcutAction::ProfileEdit => self.begin_profile_edit(),
            ShortcutAction::SelectPreviousBreakTemplate => self.select_previous_break_template(),
            ShortcutAction::SelectNextBreakTemplate => self.select_next_break_template(),
            _ => {}
        }
    }

    pub(super) fn begin_profile_edit(&mut self) {
        self.profile_edit_snapshot = Some(ProfileEditSnapshot {
            custom_profile: self.custom_profile.clone(),
            notification_settings: self.notification_settings,
            auto_start: self.auto_start,
            recurring_schedule: self.recurring_schedule.clone(),
            automation_triggers: self.automation_triggers.clone(),
            weekday_profile_rules: self.weekday_profile_rules.clone(),
            strict_mode: self.strict_mode,
            daily_goal: self.daily_goal,
            weekly_goal: self.weekly_goal,
            monthly_goal: self.monthly_goal,
            goal_carry_over: self.goal_carry_over,
            selected_theme_preset: self.selected_theme_preset,
            wakatime_metadata: self.wakatime_metadata.clone(),
        });
        self.profile_edit_active = true;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.profile_edit_one_time_window = 0;
        self.profile_edit_weekday_rule = 0;
        self.profile_edit_automation_trigger = 0;
        self.profile_edit_automation_triggers = self.automation_triggers.clone();
        self.clamp_profile_edit_schedule_selection();
    }

    fn cancel_profile_edit(&mut self) {
        if let Some(snapshot) = self.profile_edit_snapshot.take() {
            self.custom_profile = snapshot.custom_profile;
            self.notification_settings = snapshot.notification_settings;
            self.auto_start = snapshot.auto_start;
            self.recurring_schedule = snapshot.recurring_schedule;
            self.automation_triggers = snapshot.automation_triggers;
            self.weekday_profile_rules = snapshot.weekday_profile_rules;
            self.strict_mode = snapshot.strict_mode;
            self.daily_goal = snapshot.daily_goal;
            self.weekly_goal = snapshot.weekly_goal;
            self.monthly_goal = snapshot.monthly_goal;
            self.goal_carry_over = snapshot.goal_carry_over;
            self.selected_theme_preset = snapshot.selected_theme_preset;
            self.wakatime_metadata = snapshot.wakatime_metadata;
            self.sync_wakatime_metadata_to_tracker();
            self.rebuild_notifier();
            self.rebuild_recurring_schedule_runtime();
        }
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.profile_edit_one_time_window = 0;
        self.profile_edit_weekday_rule = 0;
        self.profile_edit_automation_trigger = 0;
        self.profile_edit_automation_triggers.clear();
        self.clamp_profile_edit_schedule_selection();
    }

    pub(super) fn commit_profile_edit(&mut self) {
        let edited_automation_triggers = self.profile_edit_automation_triggers.clone();
        let custom_profile_changed = self.profile_edit_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.custom_profile.normalized() != self.custom_profile.normalized()
        });
        let normalized_schedule = self.recurring_schedule.normalized();
        let schedule_changed = self.profile_edit_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.recurring_schedule.normalized() != normalized_schedule
        });
        let weekday_rules_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.weekday_profile_rules != self.weekday_profile_rules);
        let automation_triggers_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.automation_triggers != edited_automation_triggers);
        let daily_goal_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.daily_goal != self.daily_goal);
        let weekly_goal_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.weekly_goal != self.weekly_goal);
        let monthly_goal_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.monthly_goal != self.monthly_goal);
        let goal_carry_over_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.goal_carry_over != self.goal_carry_over);
        self.custom_profile = self.custom_profile.normalized();
        self.sync_active_break_template_to_custom_profile();
        self.recurring_schedule = normalized_schedule;
        self.wakatime_metadata = self.wakatime_metadata.normalized();
        if automation_triggers_changed
            && let Err(error) = validate_automation_trigger_rules(
                &edited_automation_triggers,
                &self.blocklist_profiles,
                &self.session_templates,
            )
        {
            self.config_error = Some(format!("Invalid automation trigger rules: {error}"));
            return;
        }
        if automation_triggers_changed {
            self.automation_triggers = edited_automation_triggers;
        }
        if self.selected_profile == ProfileId::Custom {
            if custom_profile_changed {
                let original_profile_automation = self.profile_automation.clone();
                self.update_selected_profile_automation();
                if !self.apply_profile(ProfileId::Custom) {
                    self.profile_automation = original_profile_automation;
                    return;
                }
            } else {
                self.update_selected_profile_automation();
                self.save_config();
            }
        } else {
            self.update_selected_profile_automation();
            self.save_config();
        }
        self.sync_wakatime_metadata_to_tracker();
        self.rebuild_notifier();
        self.rebuild_recurring_schedule_runtime();
        if schedule_changed {
            self.schedule_armed_occurrence_key = None;
            self.clear_schedule_delay_state();
            self.last_schedule_occurrence_key = None;
            let now = Local::now();
            self.current_frame_now = now;
            self.sync_recurring_schedule(now);
        }
        if weekday_rules_changed {
            self.last_weekday_profile_sync_day = None;
            let now = Local::now();
            self.current_frame_now = now;
            self.sync_weekday_profile_rules(now);
        }
        if automation_triggers_changed {
            self.automation_trigger_last_fired_minute.clear();
        }
        if daily_goal_changed
            || weekly_goal_changed
            || monthly_goal_changed
            || goal_carry_over_changed
        {
            self.sync_today_goal_snapshot();
        }
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.profile_edit_one_time_window = 0;
        self.profile_edit_weekday_rule = 0;
        self.profile_edit_automation_trigger = 0;
        self.profile_edit_automation_triggers.clear();
        self.clamp_profile_edit_schedule_selection();
        self.profile_edit_snapshot = None;
    }

    fn adjust_profile_edit_field(&mut self, increase: bool) {
        match self.profile_edit_field {
            0 => adjust_duration_minutes(&mut self.custom_profile.focus_secs, increase),
            1 => adjust_duration_minutes(&mut self.custom_profile.short_break_secs, increase),
            2 => adjust_duration_minutes(&mut self.custom_profile.long_break_secs, increase),
            3 => {
                if increase {
                    self.custom_profile.long_break_interval =
                        self.custom_profile.long_break_interval.saturating_add(1);
                } else {
                    self.custom_profile.long_break_interval = self
                        .custom_profile
                        .long_break_interval
                        .saturating_sub(1)
                        .max(1);
                }
            }
            4 => {
                self.notification_settings.enabled = increase;
            }
            5 => {
                self.notification_settings.sound = increase;
            }
            6 => {
                self.auto_start.focus_to_break = increase;
            }
            7 => {
                self.auto_start.break_to_focus = increase;
            }
            8 => {
                if !increase && self.strict_mode_enforced_for_focus() {
                    return;
                }
                self.strict_mode = increase;
            }
            PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX => {
                adjust_daily_goal_minutes(&mut self.daily_goal.minutes, increase);
            }
            PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX => {
                adjust_daily_goal_pomodoros(&mut self.daily_goal.pomodoros, increase);
            }
            PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX => {
                self.goal_carry_over.daily = increase;
            }
            PROFILE_EDIT_WEEKLY_GOAL_MINUTES_INDEX => {
                adjust_daily_goal_minutes(&mut self.weekly_goal.minutes, increase);
            }
            PROFILE_EDIT_WEEKLY_GOAL_POMODOROS_INDEX => {
                adjust_daily_goal_pomodoros(&mut self.weekly_goal.pomodoros, increase);
            }
            PROFILE_EDIT_WEEKLY_GOAL_CARRY_OVER_INDEX => {
                self.goal_carry_over.weekly = increase;
            }
            PROFILE_EDIT_MONTHLY_GOAL_MINUTES_INDEX => {
                adjust_daily_goal_minutes(&mut self.monthly_goal.minutes, increase);
            }
            PROFILE_EDIT_MONTHLY_GOAL_POMODOROS_INDEX => {
                adjust_daily_goal_pomodoros(&mut self.monthly_goal.pomodoros, increase);
            }
            PROFILE_EDIT_MONTHLY_GOAL_CARRY_OVER_INDEX => {
                self.goal_carry_over.monthly = increase;
            }
            PROFILE_EDIT_THEME_PRESET_INDEX => {
                self.selected_theme_preset = if increase {
                    self.selected_theme_preset.next()
                } else {
                    self.selected_theme_preset.previous()
                };
            }
            PROFILE_EDIT_SCHEDULE_WINDOW_INDEX => {
                self.cycle_schedule_window(increase);
            }
            PROFILE_EDIT_SCHEDULE_DAY_INDEX => {
                self.cycle_schedule_day(increase);
            }
            PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX => {
                self.set_schedule_day_enabled(increase);
            }
            PROFILE_EDIT_SCHEDULE_START_INDEX => {
                self.adjust_selected_schedule_time(true, increase);
            }
            PROFILE_EDIT_SCHEDULE_END_INDEX => {
                self.adjust_selected_schedule_time(false, increase);
            }
            PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX => {
                self.adjust_schedule_windows_collection(increase);
            }
            PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX => {
                self.cycle_schedule_exception(increase);
            }
            PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX => {
                self.adjust_selected_schedule_exception_date(increase);
            }
            PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX => {
                self.adjust_schedule_exceptions_collection(increase);
            }
            PROFILE_EDIT_ONE_TIME_WINDOW_INDEX => {
                self.cycle_one_time_window(increase);
            }
            PROFILE_EDIT_ONE_TIME_DATE_INDEX => {
                self.adjust_selected_one_time_date(increase);
            }
            PROFILE_EDIT_ONE_TIME_START_INDEX => {
                self.adjust_selected_one_time_time(true, increase);
            }
            PROFILE_EDIT_ONE_TIME_END_INDEX => {
                self.adjust_selected_one_time_time(false, increase);
            }
            PROFILE_EDIT_ONE_TIME_ADD_REMOVE_INDEX => {
                self.adjust_one_time_windows_collection(increase);
            }
            PROFILE_EDIT_WEEKDAY_RULE_INDEX => {
                self.cycle_weekday_profile_rule(increase);
            }
            PROFILE_EDIT_WEEKDAY_RULE_DAY_INDEX => {
                self.cycle_weekday_profile_rule_day(increase);
            }
            PROFILE_EDIT_WEEKDAY_RULE_PROFILE_INDEX => {
                self.cycle_weekday_profile_rule_profile(increase);
            }
            PROFILE_EDIT_WEEKDAY_RULE_BLOCKLIST_INDEX => {
                self.cycle_weekday_profile_rule_blocklist(increase);
            }
            PROFILE_EDIT_WEEKDAY_RULE_TEMPLATE_INDEX => {
                self.cycle_weekday_profile_rule_template(increase);
            }
            PROFILE_EDIT_WEEKDAY_RULE_ADD_REMOVE_INDEX => {
                self.adjust_weekday_profile_rules_collection(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_INDEX => {
                self.cycle_automation_trigger_rule(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_CONDITION_INDEX => {
                self.cycle_automation_trigger_condition(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_DAY_INDEX => {
                self.cycle_automation_trigger_time_day(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_AT_INDEX => {
                self.adjust_automation_trigger_time_at(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_ACTION_INDEX => {
                self.cycle_automation_trigger_action(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_PROFILE_INDEX => {
                self.cycle_automation_trigger_profile(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_BLOCKLIST_INDEX => {
                self.cycle_automation_trigger_blocklist(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_TEMPLATE_INDEX => {
                self.cycle_automation_trigger_template(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_DELAY_INDEX => {
                self.adjust_automation_trigger_delay(increase);
            }
            PROFILE_EDIT_AUTOMATION_TRIGGER_ADD_REMOVE_INDEX => {
                self.adjust_automation_triggers_collection(increase);
            }
            PROFILE_EDIT_WAKATIME_PROJECT_INDEX | PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX => {}
            _ => {}
        }
    }

    pub(super) fn sync_wakatime_metadata_to_tracker(&mut self) {
        let (project, language) = self
            .wakatime_metadata
            .resolved_project_language_for_task_label(self.current_task_label());
        self.integrations
            .set_wakatime_metadata(WakatimeHeartbeatMetadata { project, language });
    }

    pub(super) fn clamp_break_template_selection(&mut self) {
        if self.break_templates.is_empty() {
            self.break_templates.push(BreakTemplateConfig::default());
            return;
        }
        if let Some(active_break_template) = self.active_break_template {
            self.active_break_template =
                Some(active_break_template.min(self.break_templates.len().saturating_sub(1)));
        }
    }

    pub(super) fn select_previous_break_template(&mut self) {
        self.clamp_break_template_selection();
        if self.break_templates.is_empty() {
            return;
        }
        let last = self.break_templates.len().saturating_sub(1);
        let next = match self.active_break_template {
            None | Some(0) => last,
            Some(current) => current.min(last).saturating_sub(1),
        };
        self.switch_break_template(next);
    }

    pub(super) fn select_next_break_template(&mut self) {
        self.clamp_break_template_selection();
        if self.break_templates.is_empty() {
            return;
        }
        let last = self.break_templates.len().saturating_sub(1);
        let next = match self.active_break_template {
            None => 0,
            Some(current) => (current.min(last) + 1) % self.break_templates.len(),
        };
        self.switch_break_template(next);
    }

    fn switch_break_template(&mut self, next_index: usize) {
        if next_index >= self.break_templates.len()
            || self.active_break_template == Some(next_index)
        {
            return;
        }

        let previous_index = self.active_break_template;
        let previous_custom_profile = self.custom_profile.clone();
        let Some(template) = self.break_templates.get(next_index).cloned() else {
            return;
        };
        self.active_break_template = Some(next_index);
        let template = template.normalized();
        self.custom_profile.short_break_secs = template.short_break_secs;
        self.custom_profile.long_break_secs = template.long_break_secs;
        self.custom_profile.long_break_interval = template.long_break_interval;
        self.custom_profile = self.custom_profile.normalized();
        let custom_profile_changed = self.custom_profile != previous_custom_profile;

        if self.selected_profile == ProfileId::Custom && custom_profile_changed {
            let original_profile_automation = self.profile_automation.clone();
            if !self.apply_profile(ProfileId::Custom) {
                self.profile_automation = original_profile_automation;
                self.active_break_template = previous_index;
                self.custom_profile = previous_custom_profile;
                return;
            }
        } else {
            self.save_config();
        }

        self.phase_notification = Some(format!(
            "Break template selected: {} ({})",
            self.active_break_template_name(),
            self.active_break_template_summary()
        ));
    }
}
