use chrono::{DateTime, Datelike, Local, Timelike};

use crate::app::{App, TimerPhase, TimerState, TimerStatus};
use crate::config::{AutomationTriggerActionConfig, AutomationTriggerConditionConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutomationTriggerEvent {
    ScheduleWindowStart,
    ScheduleWindowEnd,
    FocusStarted,
    FocusCompleted,
    BreakStarted,
    BreakCompleted,
}

impl App {
    pub(super) fn sync_time_based_automation_triggers(&mut self, now: DateTime<Local>) {
        let minute_key = now.timestamp().div_euclid(60);
        let day = weekday_token(now.weekday());
        let now_hhmm = format!("{:02}:{:02}", now.hour(), now.minute());

        let actions = self
            .automation_triggers
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| match &rule.trigger {
                AutomationTriggerConditionConfig::Time { days, at }
                    if at == &now_hhmm
                        && days
                            .iter()
                            .any(|candidate| candidate.eq_ignore_ascii_case(day))
                        && self
                            .automation_trigger_last_fired_minute
                            .get(&index)
                            .copied()
                            .unwrap_or(i64::MIN)
                            != minute_key =>
                {
                    Some((index, rule.action.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        for (index, action) in actions {
            self.automation_trigger_last_fired_minute
                .insert(index, minute_key);
            if let Err(error) = self.execute_automation_trigger_action(&action, now) {
                self.config_error = Some(format!(
                    "automation trigger #{} failed: {error}",
                    index.saturating_add(1)
                ));
            }
        }
    }

    pub(super) fn fire_automation_trigger_event(
        &mut self,
        event: AutomationTriggerEvent,
        now: DateTime<Local>,
    ) {
        let actions = self
            .automation_triggers
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                event_matches_automation_trigger(event, &rule.trigger)
                    .then_some((index, rule.action.clone()))
            })
            .collect::<Vec<_>>();

        for (index, action) in actions {
            if let Err(error) = self.execute_automation_trigger_action(&action, now) {
                self.config_error = Some(format!(
                    "automation trigger #{} failed: {error}",
                    index.saturating_add(1)
                ));
            }
        }
    }

    fn execute_automation_trigger_action(
        &mut self,
        action: &AutomationTriggerActionConfig,
        now: DateTime<Local>,
    ) -> Result<(), String> {
        match action {
            AutomationTriggerActionConfig::StartFocus => {
                if self.timer.phase == TimerPhase::Focus
                    && self.timer.status == TimerStatus::Idle
                    && self.has_selectable_task_label_for_focus()
                {
                    self.update_timer_and_sync(TimerState::toggle_pause);
                }
                Ok(())
            }
            AutomationTriggerActionConfig::DelayScheduleStart { delay_secs } => {
                let delayed_until =
                    self.delay_active_schedule_start_for_workflow_with_secs(now, *delay_secs)?;
                self.phase_notification = Some(format!(
                    "Automation trigger delayed schedule start until {}.",
                    delayed_until.format("%H:%M")
                ));
                self.sync_cli_workflow_state()?;
                Ok(())
            }
            AutomationTriggerActionConfig::ApplyDefaults {
                profile,
                blocklist_profile,
                session_template,
            } => {
                if self.timer.status != TimerStatus::Idle {
                    return Ok(());
                }
                self.apply_profile_defaults_for_automation(
                    *profile,
                    blocklist_profile,
                    session_template.as_deref(),
                    "automation trigger",
                )
            }
        }
    }
}

fn event_matches_automation_trigger(
    event: AutomationTriggerEvent,
    trigger: &AutomationTriggerConditionConfig,
) -> bool {
    matches!(
        (event, trigger),
        (
            AutomationTriggerEvent::ScheduleWindowStart,
            AutomationTriggerConditionConfig::ScheduleWindowStart
        ) | (
            AutomationTriggerEvent::ScheduleWindowEnd,
            AutomationTriggerConditionConfig::ScheduleWindowEnd
        ) | (
            AutomationTriggerEvent::FocusStarted,
            AutomationTriggerConditionConfig::FocusStarted
        ) | (
            AutomationTriggerEvent::FocusCompleted,
            AutomationTriggerConditionConfig::FocusCompleted
        ) | (
            AutomationTriggerEvent::BreakStarted,
            AutomationTriggerConditionConfig::BreakStarted
        ) | (
            AutomationTriggerEvent::BreakCompleted,
            AutomationTriggerConditionConfig::BreakCompleted
        )
    )
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
