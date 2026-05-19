use crate::app::{App, Local, TimerStatus};
use crate::config::WeekdayProfileRuleConfig;
use chrono::{DateTime, Datelike};

impl App {
    pub(super) fn sync_weekday_profile_rules(&mut self, now: DateTime<Local>) {
        let today = now.date_naive();
        if self.last_weekday_profile_sync_day == Some(today) {
            return;
        }

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
        let mut changed_after_profile_apply = false;
        let profile_changed = self.selected_profile != rule.profile;
        if profile_changed && !self.apply_profile(rule.profile) {
            return Err(self
                .config_error
                .clone()
                .or_else(|| self.phase_notification.clone())
                .unwrap_or_else(|| "failed to apply profile from weekday rule".to_string()));
        }

        let Some(blocklist_index) = self.blocklist_profiles.iter().position(|profile| {
            profile
                .name
                .eq_ignore_ascii_case(rule.blocklist_profile.as_str())
        }) else {
            return Err(format!(
                "weekday rule for `{}` references missing blocklist profile `{}`",
                rule.day, rule.blocklist_profile
            ));
        };
        if self.active_blocklist_profile != blocklist_index {
            self.active_blocklist_profile = blocklist_index;
            self.recompute_blocker_sites_from_active_profile();
            self.clamp_selection();
            changed_after_profile_apply = true;
        }

        let target_template = match rule.session_template.as_deref() {
            Some(name) => Some(self.session_template_index_by_name(name).ok_or_else(|| {
                format!(
                    "weekday rule for `{}` references missing session template `{}`",
                    rule.day, name
                )
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
