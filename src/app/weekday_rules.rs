use crate::app::{App, Local, TimerStatus};
use crate::config::{ProfileId, WeekdayProfileRuleConfig};
use chrono::{DateTime, Datelike};

impl App {
    pub(super) fn sync_weekday_profile_rules(&mut self, now: DateTime<Local>) {
        let today = now.date_naive();
        if self.last_weekday_profile_sync_day == Some(today) {
            return;
        }

        self.apply_weekday_profile_rule_for_current_day(now);
    }

    pub(super) fn apply_weekday_profile_rule_for_current_day(&mut self, now: DateTime<Local>) {
        let today = now.date_naive();

        // Avoid resetting an in-progress timer session at day rollover.
        if self.timer.status != TimerStatus::Idle {
            return;
        }

        let day = weekday_token(now.weekday());
        let Some(rule) = self
            .weekday_profile_rules
            .iter()
            .find(|rule| rule.day.eq_ignore_ascii_case(day))
            .cloned()
        else {
            self.last_weekday_profile_sync_day = Some(today);
            return;
        };

        if let Err(error) = self.apply_weekday_profile_rule(&rule) {
            self.config_error = Some(error);
        }
        self.last_weekday_profile_sync_day = Some(today);
    }

    fn apply_weekday_profile_rule(
        &mut self,
        rule: &WeekdayProfileRuleConfig,
    ) -> Result<(), String> {
        let context = format!("weekday rule for `{}`", rule.day);
        self.apply_profile_defaults_for_automation(
            rule.profile,
            &rule.blocklist_profile,
            rule.session_template.as_deref(),
            &context,
        )
    }

    pub(super) fn apply_profile_defaults_for_automation(
        &mut self,
        profile: ProfileId,
        blocklist_profile: &str,
        session_template: Option<&str>,
        context: &str,
    ) -> Result<(), String> {
        let mut changed_after_profile_apply = false;
        let profile_changed = self.selected_profile != profile;
        if profile_changed && !self.apply_profile(profile) {
            return Err(self
                .config_error
                .clone()
                .or_else(|| self.phase_notification.clone())
                .unwrap_or_else(|| format!("failed to apply profile from {context}")));
        }

        let Some(blocklist_index) = self
            .blocklist_profiles
            .iter()
            .position(|profile| profile.name.eq_ignore_ascii_case(blocklist_profile))
        else {
            return Err(format!(
                "{context} references missing blocklist profile `{blocklist_profile}`"
            ));
        };
        if self.active_blocklist_profile != blocklist_index {
            self.active_blocklist_profile = blocklist_index;
            self.recompute_blocker_sites_from_active_profile();
            self.clamp_selection();
            changed_after_profile_apply = true;
        }

        let target_template = match session_template {
            Some(name) => Some(self.session_template_index_by_name(name).ok_or_else(|| {
                format!("{context} references missing session template `{name}`")
            })?),
            None => None,
        };

        if self.active_session_template != target_template {
            self.active_session_template = target_template;
            self.planner_template_selection_index = self.active_session_template.unwrap_or(0);
            changed_after_profile_apply = true;
        }

        if changed_after_profile_apply {
            self.save_config();
            self.apply_blocking_for_phase();
            self.sync_recovery_snapshot();
        }

        Ok(())
    }
}

fn weekday_token(day: chrono::Weekday) -> &'static str {
    match day {
        chrono::Weekday::Mon => "mon",
        chrono::Weekday::Tue => "tue",
        chrono::Weekday::Wed => "wed",
        chrono::Weekday::Thu => "thu",
        chrono::Weekday::Fri => "fri",
        chrono::Weekday::Sat => "sat",
        chrono::Weekday::Sun => "sun",
    }
}
