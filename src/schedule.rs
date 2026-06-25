use chrono::{DateTime, Local, Weekday};

use crate::config::RecurringFocusWindowConfig;

mod conflicts;
mod occurrence;
mod parsing;

#[allow(unused_imports)]
pub(crate) use conflicts::inspect_schedule_conflicts;
pub(crate) use conflicts::{format_schedule_conflict, inspect_schedule_conflicts_from_config};
pub(crate) use occurrence::{active_occurrence, next_occurrence_after, occurrence_key};

use parsing::{parse_time_minutes, parse_weekdays};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecurringWindow {
    days: Vec<Weekday>,
    start_minutes: u16,
    end_minutes: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowOccurrence {
    pub(crate) window_index: usize,
    pub(crate) start: DateTime<Local>,
    pub(crate) end: DateTime<Local>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScheduleConflict {
    pub(crate) first_window_index: usize,
    pub(crate) second_window_index: usize,
    pub(crate) weekday: Weekday,
    pub(crate) overlap_start_minutes: u16,
    pub(crate) overlap_end_minutes: u16,
}

pub(crate) fn compile_windows(
    config_windows: &[RecurringFocusWindowConfig],
) -> Vec<RecurringWindow> {
    config_windows
        .iter()
        .filter_map(RecurringWindow::from_config)
        .collect()
}

impl RecurringWindow {
    fn from_config(config: &RecurringFocusWindowConfig) -> Option<Self> {
        let days = parse_weekdays(&config.days);
        if days.is_empty() {
            return None;
        }
        let start_minutes = parse_time_minutes(&config.start)?;
        let end_minutes = parse_time_minutes(&config.end)?;
        if start_minutes >= end_minutes {
            return None;
        }
        Some(Self {
            days,
            start_minutes,
            end_minutes,
        })
    }
}

#[cfg(test)]
mod tests;
