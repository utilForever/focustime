use std::collections::HashSet;

use chrono::{DateTime, Local, NaiveDate, Weekday};

use crate::config::{OneTimeFocusWindowConfig, RecurringFocusWindowConfig};

mod conflicts;
mod occurrence;
mod parsing;

#[allow(unused_imports)]
pub(crate) use conflicts::inspect_schedule_conflicts;
pub(crate) use conflicts::{format_schedule_conflict, inspect_schedule_conflicts_from_config};
pub(crate) use occurrence::{
    active_occurrence, active_one_time_occurrence, next_occurrence_after,
    next_one_time_occurrence_after, occurrence_key, pick_active_occurrence, pick_next_occurrence,
};

use parsing::{parse_exception_date, parse_time_minutes, parse_weekdays};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecurringWindow {
    days: Vec<Weekday>,
    start_minutes: u16,
    end_minutes: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OneTimeWindow {
    pub(crate) date: NaiveDate,
    start_minutes: u16,
    end_minutes: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowOccurrenceKind {
    Recurring,
    OneTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowOccurrence {
    pub(crate) kind: WindowOccurrenceKind,
    pub(crate) window_index: usize,
    pub(crate) start: DateTime<Local>,
    pub(crate) end: DateTime<Local>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScheduleConflictContext {
    Weekday(Weekday),
    Date(NaiveDate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScheduleConflict {
    pub(crate) first_kind: WindowOccurrenceKind,
    pub(crate) first_window_index: usize,
    pub(crate) second_kind: WindowOccurrenceKind,
    pub(crate) second_window_index: usize,
    pub(crate) context: ScheduleConflictContext,
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

pub(crate) fn compile_one_time_windows(
    config_windows: &[OneTimeFocusWindowConfig],
) -> Vec<OneTimeWindow> {
    config_windows
        .iter()
        .filter_map(OneTimeWindow::from_config)
        .collect()
}

pub(crate) fn compile_exception_dates(config_dates: &[String]) -> HashSet<NaiveDate> {
    config_dates
        .iter()
        .filter_map(|value| parse_exception_date(value))
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

impl OneTimeWindow {
    fn from_config(config: &OneTimeFocusWindowConfig) -> Option<Self> {
        let date = parse_exception_date(&config.date)?;
        let start_minutes = parse_time_minutes(&config.start)?;
        let end_minutes = parse_time_minutes(&config.end)?;
        if start_minutes >= end_minutes {
            return None;
        }
        Some(Self {
            date,
            start_minutes,
            end_minutes,
        })
    }
}

#[cfg(test)]
mod tests;
