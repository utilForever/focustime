use super::*;

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
        self.profile_edit_snapshot = None;
        self.profile_selection_index = profile_index(self.selected_profile);
        self.clamp_profile_selection();
        self.clamp_profile_edit_schedule_selection();
        self.clamp_break_template_selection();
    }

    pub(super) fn exit_profile_manager(&mut self) {
        self.mode = AppMode::Timer;
        self.profile_edit_snapshot = None;
    }
}
