use std::collections::{HashMap, HashSet};

use super::{
    AutomationTriggerActionConfig, AutomationTriggerConditionConfig, AutomationTriggerRuleConfig,
    BlocklistProfileConfig, SCHEDULE_DELAY_MAX_SECS, SCHEDULE_DELAY_MIN_SECS,
    SessionTemplateConfig, parse_schedule_time_minutes,
};

pub(super) fn normalize_automation_triggers(
    rules: &[AutomationTriggerRuleConfig],
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
) -> Vec<AutomationTriggerRuleConfig> {
    rules
        .iter()
        .filter_map(|rule| rule.normalized_with_context(blocklist_profiles, session_templates))
        .collect()
}

pub(crate) fn validate_automation_trigger_rules(
    rules: &[AutomationTriggerRuleConfig],
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
) -> Result<(), String> {
    let mut seen_trigger_keys: HashMap<String, AutomationTriggerConflictRule> = HashMap::new();
    for (index, rule) in rules.iter().enumerate() {
        validate_automation_trigger_rule(
            rule,
            index,
            blocklist_profiles,
            session_templates,
            &mut seen_trigger_keys,
        )?;
    }
    Ok(())
}

fn validate_automation_trigger_rule(
    rule: &AutomationTriggerRuleConfig,
    index: usize,
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
    seen_trigger_keys: &mut HashMap<String, AutomationTriggerConflictRule>,
) -> Result<(), String> {
    validate_automation_trigger_condition(&rule.trigger, index)?;
    validate_automation_trigger_action(&rule.action, index, blocklist_profiles, session_templates)?;
    validate_automation_trigger_conflicts(&rule.trigger, &rule.action, index, seen_trigger_keys)?;
    Ok(())
}

fn validate_automation_trigger_condition(
    trigger: &AutomationTriggerConditionConfig,
    index: usize,
) -> Result<(), String> {
    match trigger {
        AutomationTriggerConditionConfig::ScheduleWindowStart
        | AutomationTriggerConditionConfig::ScheduleWindowEnd
        | AutomationTriggerConditionConfig::FocusStarted
        | AutomationTriggerConditionConfig::FocusCompleted
        | AutomationTriggerConditionConfig::BreakStarted
        | AutomationTriggerConditionConfig::BreakCompleted => Ok(()),
        AutomationTriggerConditionConfig::Time { days, at } => {
            if days.is_empty() {
                return Err(format!(
                    "Invalid automation trigger rule at index {index}: time trigger `days` cannot be empty."
                ));
            }
            for day in days {
                if weekday_token_to_index(day).is_none() {
                    return Err(format!(
                        "Invalid automation trigger rule at index {index}: unknown weekday `{day}` in time trigger."
                    ));
                }
            }
            if parse_schedule_time_minutes(at).is_none() {
                return Err(format!(
                    "Invalid automation trigger rule at index {index}: time trigger `at` must be HH:MM in 24-hour format."
                ));
            }
            Ok(())
        }
    }
}

fn validate_automation_trigger_action(
    action: &AutomationTriggerActionConfig,
    index: usize,
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
) -> Result<(), String> {
    match action {
        AutomationTriggerActionConfig::StartFocus => Ok(()),
        AutomationTriggerActionConfig::DelayScheduleStart { delay_secs } => {
            if *delay_secs < SCHEDULE_DELAY_MIN_SECS || *delay_secs > SCHEDULE_DELAY_MAX_SECS {
                return Err(format!(
                    "Invalid automation trigger rule at index {index}: `delay_secs` must be between {SCHEDULE_DELAY_MIN_SECS} and {SCHEDULE_DELAY_MAX_SECS}."
                ));
            }
            Ok(())
        }
        AutomationTriggerActionConfig::ApplyDefaults {
            blocklist_profile,
            session_template,
            ..
        } => validate_automation_trigger_apply_defaults_action(
            blocklist_profile,
            session_template.as_deref(),
            index,
            blocklist_profiles,
            session_templates,
        ),
    }
}

fn validate_automation_trigger_apply_defaults_action(
    blocklist_profile: &str,
    session_template: Option<&str>,
    index: usize,
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
) -> Result<(), String> {
    if blocklist_profile.trim().is_empty() {
        return Err(format!(
            "Invalid automation trigger rule at index {index}: `blocklist_profile` cannot be empty."
        ));
    }
    if !blocklist_profiles
        .iter()
        .any(|profile| profile.name.eq_ignore_ascii_case(blocklist_profile.trim()))
    {
        return Err(format!(
            "Invalid automation trigger rule at index {index}: blocklist profile `{}` does not exist.",
            blocklist_profile
        ));
    }
    if let Some(template) = session_template {
        if template.trim().is_empty() {
            return Err(format!(
                "Invalid automation trigger rule at index {index}: `session_template` cannot be empty when provided."
            ));
        }
        if !session_templates
            .iter()
            .any(|candidate| candidate.name.eq_ignore_ascii_case(template.trim()))
        {
            return Err(format!(
                "Invalid automation trigger rule at index {index}: session template `{template}` does not exist."
            ));
        }
    }
    Ok(())
}

fn validate_automation_trigger_conflicts(
    trigger: &AutomationTriggerConditionConfig,
    action: &AutomationTriggerActionConfig,
    index: usize,
    seen_trigger_keys: &mut HashMap<String, AutomationTriggerConflictRule>,
) -> Result<(), String> {
    let conflict_keys = automation_trigger_conflict_keys(trigger);
    for trigger_key in conflict_keys {
        if let Some(previous_rule) = seen_trigger_keys.get(&trigger_key)
            && automation_trigger_actions_conflict(&previous_rule.action, action)
        {
            let previous_action = format_automation_trigger_action(&previous_rule.action);
            let current_action = format_automation_trigger_action(action);
            return Err(format!(
                "Conflicting automation trigger rules: rule #{} (`{previous_action}`) conflicts with rule #{} (`{current_action}`) because both target {}. Keep one rule per trigger condition, or change one trigger so they do not overlap.",
                previous_rule.index + 1,
                index + 1,
                format_automation_trigger_conflict_key(&trigger_key),
            ));
        }
        seen_trigger_keys.insert(
            trigger_key,
            AutomationTriggerConflictRule {
                index,
                action: action.clone(),
            },
        );
    }
    Ok(())
}

fn automation_trigger_conflict_keys(trigger: &AutomationTriggerConditionConfig) -> Vec<String> {
    match trigger {
        AutomationTriggerConditionConfig::ScheduleWindowStart => {
            vec!["schedule_window_start".to_string()]
        }
        AutomationTriggerConditionConfig::ScheduleWindowEnd => {
            vec!["schedule_window_end".to_string()]
        }
        AutomationTriggerConditionConfig::FocusStarted => vec!["focus_started".to_string()],
        AutomationTriggerConditionConfig::FocusCompleted => vec!["focus_completed".to_string()],
        AutomationTriggerConditionConfig::BreakStarted => vec!["break_started".to_string()],
        AutomationTriggerConditionConfig::BreakCompleted => vec!["break_completed".to_string()],
        AutomationTriggerConditionConfig::Time { days, at } => {
            let mut keys = Vec::new();
            for day in days {
                if let Some(day_index) = weekday_token_to_index(day) {
                    keys.push(format!(
                        "time:{}@{}",
                        weekday_token_from_index(day_index),
                        at
                    ));
                }
            }
            keys.sort();
            keys.dedup();
            keys
        }
    }
}

fn format_automation_trigger_conflict_key(key: &str) -> String {
    if let Some(rest) = key.strip_prefix("time:") {
        return format!("time trigger `{rest}`");
    }
    format!("event trigger `{key}`")
}

#[derive(Debug, Clone)]
struct AutomationTriggerConflictRule {
    index: usize,
    action: AutomationTriggerActionConfig,
}

fn automation_trigger_actions_conflict(
    _existing: &AutomationTriggerActionConfig,
    _candidate: &AutomationTriggerActionConfig,
) -> bool {
    // Automation trigger matrix: overlapping trigger identities always conflict today,
    // regardless of action type. We keep this explicit helper to evolve policy later.
    true
}

fn format_automation_trigger_action(action: &AutomationTriggerActionConfig) -> &'static str {
    match action {
        AutomationTriggerActionConfig::StartFocus => "start_focus",
        AutomationTriggerActionConfig::DelayScheduleStart { .. } => "delay_schedule_start",
        AutomationTriggerActionConfig::ApplyDefaults { .. } => "apply_defaults",
    }
}

pub(super) fn normalize_trigger_days(days: &[String]) -> Option<Vec<String>> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    for day in days {
        let Some(index) = weekday_token_to_index(day) else {
            continue;
        };
        let token = weekday_token_from_index(index).to_string();
        if seen.insert(token.clone()) {
            normalized.push(token);
        }
    }
    if normalized.is_empty() {
        return None;
    }
    normalized.sort_by_key(|day| weekday_token_to_index(day).unwrap_or(usize::MAX));
    Some(normalized)
}

fn weekday_token_to_index(value: &str) -> Option<usize> {
    match value.trim().to_ascii_lowercase().as_str() {
        "mon" | "monday" => Some(0),
        "tue" | "tues" | "tuesday" => Some(1),
        "wed" | "wednesday" => Some(2),
        "thu" | "thurs" | "thursday" => Some(3),
        "fri" | "friday" => Some(4),
        "sat" | "saturday" => Some(5),
        "sun" | "sunday" => Some(6),
        _ => None,
    }
}

fn weekday_token_from_index(index: usize) -> &'static str {
    match index {
        0 => "mon",
        1 => "tue",
        2 => "wed",
        3 => "thu",
        4 => "fri",
        5 => "sat",
        6 => "sun",
        _ => "mon",
    }
}
