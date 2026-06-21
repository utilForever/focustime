use std::fs;
use std::io;
use std::path::PathBuf;

#[cfg(test)]
use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Local, TimeZone};
#[cfg(test)]
use chrono::{Datelike, LocalResult, NaiveDate, NaiveDateTime, Timelike, Weekday};
#[cfg(test)]
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::config::CalendarProviderConfig;
#[cfg(test)]
use crate::config::CalendarSourceConfig;

const CALENDAR_SYNC_CACHE_FILE_NAME: &str = "calendar-sync-cache.json";
#[cfg(test)]
type PropertyParams = HashMap<String, String>;
#[cfg(test)]
type PropertyFieldMap = HashMap<String, Vec<(PropertyParams, String)>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CalendarBusyWindow {
    pub(crate) source_name: String,
    pub(crate) provider: CalendarProviderConfig,
    pub(crate) summary: String,
    pub(crate) start_epoch_secs: i64,
    pub(crate) end_epoch_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CalendarSyncCacheDisk {
    schema_version: u32,
    synced_at_epoch_secs: i64,
    source_count: usize,
    windows: Vec<CalendarBusyWindow>,
    source_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
struct ParsedEvent {
    summary: String,
    start: DateTime<Local>,
    end: DateTime<Local>,
    recurrence: Option<RecurrenceRule>,
    exdate_starts: HashSet<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
struct RecurrenceRule {
    frequency: RecurrenceFrequency,
    interval: u32,
    count: Option<u32>,
    until: Option<DateTime<Local>>,
    byday: Vec<Weekday>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
enum RecurrenceFrequency {
    Daily,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
struct EventDateValue {
    start: DateTime<Local>,
    is_all_day: bool,
}

pub(crate) fn load_cached_windows(
    now: DateTime<Local>,
    lookahead_days: u16,
) -> Result<Vec<CalendarBusyWindow>, String> {
    let path = cache_path()?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "Calendar sync cache read failed at `{}`: {error}",
                path.display()
            ));
        }
    };
    let disk: CalendarSyncCacheDisk = serde_json::from_str(&content).map_err(|error| {
        format!(
            "Calendar sync cache decode failed at `{}`: {error}",
            path.display()
        )
    })?;
    let window_start = now - Duration::days(1);
    let window_end = now + Duration::days(i64::from(lookahead_days));
    Ok(disk
        .windows
        .into_iter()
        .filter(|window| {
            let Some(start) = local_datetime_from_epoch(window.start_epoch_secs) else {
                return false;
            };
            let Some(end) = local_datetime_from_epoch(window.end_epoch_secs) else {
                return false;
            };
            end > window_start && start < window_end
        })
        .collect())
}

pub(crate) fn first_overlap(
    windows: &[CalendarBusyWindow],
    start: DateTime<Local>,
    end: DateTime<Local>,
) -> Option<&CalendarBusyWindow> {
    windows.iter().find(|window| {
        let Some(window_start) = local_datetime_from_epoch(window.start_epoch_secs) else {
            return false;
        };
        let Some(window_end) = local_datetime_from_epoch(window.end_epoch_secs) else {
            return false;
        };
        window_end > start && window_start < end
    })
}

pub(crate) fn active_window_at(
    windows: &[CalendarBusyWindow],
    now: DateTime<Local>,
) -> Option<&CalendarBusyWindow> {
    windows.iter().find(|window| {
        let Some(window_start) = local_datetime_from_epoch(window.start_epoch_secs) else {
            return false;
        };
        let Some(window_end) = local_datetime_from_epoch(window.end_epoch_secs) else {
            return false;
        };
        window_start <= now && now < window_end
    })
}

#[cfg(test)]
fn parse_ics_busy_windows(
    source: &CalendarSourceConfig,
    ics: &str,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
) -> Result<Vec<CalendarBusyWindow>, String> {
    let lines = unfold_ics_lines(ics);
    let events = parse_events(&lines, source)?;
    let mut windows = Vec::new();
    for event in events {
        expand_event(&event, range_start, range_end, &mut windows, source);
    }
    Ok(windows)
}

#[cfg(test)]
fn parse_events(
    lines: &[String],
    source: &CalendarSourceConfig,
) -> Result<Vec<ParsedEvent>, String> {
    let mut events = Vec::new();
    let mut in_event = false;
    let mut fields: PropertyFieldMap = HashMap::new();

    for line in lines {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VEVENT" {
            in_event = true;
            fields.clear();
            continue;
        }
        if upper == "END:VEVENT" {
            in_event = false;
            if let Some(event) = build_event(&fields, source)? {
                events.push(event);
            }
            continue;
        }
        if !in_event {
            continue;
        }
        let Some((property_name, property_params, property_value)) = parse_property(line) else {
            continue;
        };
        fields
            .entry(property_name)
            .or_default()
            .push((property_params, property_value));
    }
    Ok(events)
}

#[cfg(test)]
fn build_event(
    fields: &PropertyFieldMap,
    source: &CalendarSourceConfig,
) -> Result<Option<ParsedEvent>, String> {
    let Some((start_params, start_raw)) = first_property(fields, "DTSTART") else {
        return Ok(None);
    };
    let start_value = parse_event_date_value(start_raw, start_params).map_err(|error| {
        format!(
            "Calendar sync source `{}` has invalid DTSTART `{start_raw}`: {error}",
            source.name
        )
    })?;
    let end = if let Some((end_params, end_raw)) = first_property(fields, "DTEND") {
        let end_value = parse_event_date_value(end_raw, end_params).map_err(|error| {
            format!(
                "Calendar sync source `{}` has invalid DTEND `{end_raw}`: {error}",
                source.name
            )
        })?;
        end_value.start
    } else if start_value.is_all_day {
        start_value.start + Duration::days(1)
    } else {
        start_value.start + Duration::hours(1)
    };

    if end <= start_value.start {
        return Ok(None);
    }

    let summary = first_property(fields, "SUMMARY")
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Busy".to_string());
    let recurrence = first_property(fields, "RRULE")
        .map(|(_, value)| parse_recurrence_rule(value, start_value.start));
    let recurrence = recurrence.transpose().map_err(|error| {
        format!(
            "Calendar sync source `{}` has invalid RRULE: {error}",
            source.name
        )
    })?;
    let exdate_starts = parse_exdates(fields, start_value.start)?;

    Ok(Some(ParsedEvent {
        summary,
        start: start_value.start,
        end,
        recurrence,
        exdate_starts,
    }))
}

#[cfg(test)]
fn parse_exdates(
    fields: &PropertyFieldMap,
    fallback_start: DateTime<Local>,
) -> Result<HashSet<i64>, String> {
    let mut exdates = HashSet::new();
    let Some(values) = fields.get("EXDATE") else {
        return Ok(exdates);
    };
    for (params, raw_value) in values {
        for token in raw_value
            .split(',')
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            let date_value =
                parse_event_date_value_with_fallback_tz(token, params, fallback_start.timezone())?;
            exdates.insert(date_value.start.timestamp());
        }
    }
    Ok(exdates)
}

#[cfg(test)]
fn expand_event(
    event: &ParsedEvent,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
    windows: &mut Vec<CalendarBusyWindow>,
    source: &CalendarSourceConfig,
) {
    let duration = event.end - event.start;
    if duration <= Duration::zero() {
        return;
    }

    if let Some(recurrence) = event.recurrence.as_ref() {
        expand_recurring_event(
            event,
            recurrence,
            duration,
            range_start,
            range_end,
            windows,
            source,
        );
        return;
    }

    push_window_if_in_range(
        event,
        source,
        event.start,
        event.end,
        range_start,
        range_end,
        windows,
    );
}

#[cfg(test)]
fn expand_recurring_event(
    event: &ParsedEvent,
    recurrence: &RecurrenceRule,
    duration: Duration,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
    windows: &mut Vec<CalendarBusyWindow>,
    source: &CalendarSourceConfig,
) {
    let mut generated_count = 0u32;
    let mut date = event.start.date_naive();
    let end_date = range_end.date_naive();
    while date <= end_date {
        if let Some(start) = recurrence_start_for_date(event, recurrence, date) {
            if recurrence.until.is_some_and(|until| start > until) {
                break;
            }
            generated_count = generated_count.saturating_add(1);
            if recurrence
                .count
                .is_some_and(|count| generated_count > count)
            {
                break;
            }
            if !event.exdate_starts.contains(&start.timestamp()) {
                let end = start + duration;
                push_window_if_in_range(event, source, start, end, range_start, range_end, windows);
            }
        }
        let Some(next) = date.succ_opt() else {
            break;
        };
        date = next;
    }
}

#[cfg(test)]
fn recurrence_start_for_date(
    event: &ParsedEvent,
    recurrence: &RecurrenceRule,
    date: NaiveDate,
) -> Option<DateTime<Local>> {
    if !recurrence_occurs_on_date(recurrence, event.start.date_naive(), date) {
        return None;
    }
    let start = local_datetime_on_date_with_time(date, event.start)?;
    (start >= event.start).then_some(start)
}

#[cfg(test)]
fn push_window_if_in_range(
    event: &ParsedEvent,
    source: &CalendarSourceConfig,
    start: DateTime<Local>,
    end: DateTime<Local>,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
    windows: &mut Vec<CalendarBusyWindow>,
) {
    if end <= range_start || start >= range_end {
        return;
    }
    windows.push(CalendarBusyWindow {
        source_name: source.name.clone(),
        provider: source.provider,
        summary: event.summary.clone(),
        start_epoch_secs: start.timestamp(),
        end_epoch_secs: end.timestamp(),
    });
}

#[cfg(test)]
fn recurrence_occurs_on_date(
    rule: &RecurrenceRule,
    start_date: NaiveDate,
    candidate: NaiveDate,
) -> bool {
    if candidate < start_date {
        return false;
    }
    let days = candidate.signed_duration_since(start_date).num_days();
    match rule.frequency {
        RecurrenceFrequency::Daily => days % i64::from(rule.interval) == 0,
        RecurrenceFrequency::Weekly => {
            let weeks = days / 7;
            if weeks % i64::from(rule.interval) != 0 {
                return false;
            }
            let byday = if rule.byday.is_empty() {
                vec![start_date.weekday()]
            } else {
                rule.byday.clone()
            };
            byday.contains(&candidate.weekday())
        }
    }
}

#[cfg(test)]
fn parse_recurrence_rule(raw: &str, start: DateTime<Local>) -> Result<RecurrenceRule, String> {
    let mut freq: Option<RecurrenceFrequency> = None;
    let mut interval = 1u32;
    let mut count: Option<u32> = None;
    let mut until: Option<DateTime<Local>> = None;
    let mut byday = Vec::new();
    for token in raw.split(';') {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_uppercase();
        let value = value.trim();
        match key.as_str() {
            "FREQ" => {
                freq = match value.to_ascii_uppercase().as_str() {
                    "DAILY" => Some(RecurrenceFrequency::Daily),
                    "WEEKLY" => Some(RecurrenceFrequency::Weekly),
                    other => {
                        return Err(format!("unsupported FREQ `{other}` (only DAILY/WEEKLY)"));
                    }
                };
            }
            "INTERVAL" => {
                interval = value.parse::<u32>().unwrap_or(1).max(1);
            }
            "COUNT" => {
                count = value.parse::<u32>().ok();
            }
            "UNTIL" => {
                let until_value = parse_event_date_value_with_fallback_tz(
                    value,
                    &HashMap::new(),
                    start.timezone(),
                )?;
                until = Some(until_value.start);
            }
            "BYDAY" => {
                byday = value
                    .split(',')
                    .filter_map(parse_weekday_token)
                    .collect::<Vec<_>>();
            }
            _ => {}
        }
    }
    let Some(frequency) = freq else {
        return Err("missing FREQ".to_string());
    };
    Ok(RecurrenceRule {
        frequency,
        interval,
        count,
        until,
        byday,
    })
}

#[cfg(test)]
fn parse_event_date_value(raw: &str, params: &PropertyParams) -> Result<EventDateValue, String> {
    parse_event_date_value_with_fallback_tz(raw, params, Local)
}

#[cfg(test)]
fn parse_event_date_value_with_fallback_tz<TzLike>(
    raw: &str,
    params: &PropertyParams,
    fallback_tz: TzLike,
) -> Result<EventDateValue, String>
where
    TzLike: TimeZone,
{
    let value = raw.trim();
    let value_type = params
        .get("VALUE")
        .map(|v| v.trim().to_ascii_uppercase())
        .unwrap_or_default();
    if value_type == "DATE" || value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d")
            .map_err(|error| format!("invalid date value `{value}`: {error}"))?;
        let Some(start) = local_datetime_on_day_start(date) else {
            return Err(format!("invalid local date value `{value}`"));
        };
        return Ok(EventDateValue {
            start,
            is_all_day: true,
        });
    }

    if value.ends_with('Z') {
        let utc = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ")
            .map_err(|error| format!("invalid UTC datetime `{value}`: {error}"))?;
        let start = chrono::Utc.from_utc_datetime(&utc).with_timezone(&Local);
        return Ok(EventDateValue {
            start,
            is_all_day: false,
        });
    }

    let naive = parse_naive_local_datetime(value)?;
    if let Some(tzid) = params.get("TZID").map(|value| value.trim()) {
        let tzid = tzid.trim_matches('"');
        let tz: Tz = tzid
            .parse()
            .map_err(|_| format!("unsupported TZID `{tzid}`"))?;
        let start = match tz.from_local_datetime(&naive) {
            LocalResult::Single(date_time) => date_time.with_timezone(&Local),
            LocalResult::Ambiguous(first, _) => first.with_timezone(&Local),
            LocalResult::None => return Err(format!("invalid local datetime for TZID `{tzid}`")),
        };
        return Ok(EventDateValue {
            start,
            is_all_day: false,
        });
    }

    let fallback_dt = match fallback_tz.from_local_datetime(&naive) {
        LocalResult::Single(value) => value.with_timezone(&Local),
        LocalResult::Ambiguous(first, _) => first.with_timezone(&Local),
        LocalResult::None => {
            return Err(format!(
                "invalid local datetime value `{value}` for local timezone"
            ));
        }
    };
    Ok(EventDateValue {
        start: fallback_dt,
        is_all_day: false,
    })
}

#[cfg(test)]
fn parse_naive_local_datetime(value: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M"))
        .map_err(|error| format!("invalid datetime `{value}`: {error}"))
}

#[cfg(test)]
fn parse_weekday_token(token: &str) -> Option<Weekday> {
    match token.trim().to_ascii_uppercase().as_str() {
        "MO" => Some(Weekday::Mon),
        "TU" => Some(Weekday::Tue),
        "WE" => Some(Weekday::Wed),
        "TH" => Some(Weekday::Thu),
        "FR" => Some(Weekday::Fri),
        "SA" => Some(Weekday::Sat),
        "SU" => Some(Weekday::Sun),
        _ => None,
    }
}

#[cfg(test)]
fn parse_property(line: &str) -> Option<(String, PropertyParams, String)> {
    let (raw_head, raw_value) = line.split_once(':')?;
    let mut head_parts = raw_head.split(';');
    let name = head_parts.next()?.trim().to_ascii_uppercase();
    let mut params = HashMap::new();
    for part in head_parts {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        params.insert(key.trim().to_ascii_uppercase(), value.trim().to_string());
    }
    Some((name, params, raw_value.trim().to_string()))
}

#[cfg(test)]
fn first_property<'a>(
    fields: &'a PropertyFieldMap,
    key: &str,
) -> Option<(&'a PropertyParams, &'a str)> {
    fields
        .get(key)
        .and_then(|values| values.first())
        .map(|(params, value)| (params, value.as_str()))
}

#[cfg(test)]
fn unfold_ics_lines(content: &str) -> Vec<String> {
    let mut unfolded: Vec<String> = Vec::new();
    for raw in content.replace("\r\n", "\n").split('\n') {
        let line = raw.trim_end_matches('\r');
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = unfolded.last_mut() {
                last.push_str(line.trim_start());
            }
            continue;
        }
        unfolded.push(line.to_string());
    }
    unfolded
}

#[cfg(test)]
fn normalized_source_url(source: &CalendarSourceConfig) -> String {
    let url = source.url.trim();
    if let Some(rest) = url.strip_prefix("webcal://") {
        return format!("https://{rest}");
    }
    if let Some(rest) = url.strip_prefix("webcals://") {
        return format!("https://{rest}");
    }
    url.to_string()
}

fn cache_path() -> Result<PathBuf, String> {
    crate::config::app_data_path(CALENDAR_SYNC_CACHE_FILE_NAME).ok_or_else(|| {
        format!(
            "Calendar sync failed: could not determine application data path for `{CALENDAR_SYNC_CACHE_FILE_NAME}`."
        )
    })
}

#[cfg(test)]
fn local_datetime_on_day_start(date: NaiveDate) -> Option<DateTime<Local>> {
    match Local.with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(first, _) => Some(first),
        LocalResult::None => None,
    }
}

#[cfg(test)]
fn local_datetime_on_date_with_time(
    date: NaiveDate,
    sample: DateTime<Local>,
) -> Option<DateTime<Local>> {
    match Local.with_ymd_and_hms(
        date.year(),
        date.month(),
        date.day(),
        sample.hour(),
        sample.minute(),
        sample.second(),
    ) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(first, _) => Some(first),
        LocalResult::None => None,
    }
}

fn local_datetime_from_epoch(epoch_secs: i64) -> Option<DateTime<Local>> {
    Local.timestamp_opt(epoch_secs, 0).single()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> CalendarSourceConfig {
        CalendarSourceConfig {
            name: "work".to_string(),
            provider: CalendarProviderConfig::Ics,
            url: "https://example.com/calendar.ics".to_string(),
            enabled: true,
        }
    }

    fn local_datetime(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Local> {
        match Local.with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0) {
            LocalResult::Single(dt) => dt,
            LocalResult::Ambiguous(first, _) => first,
            LocalResult::None => panic!("expected representable local datetime"),
        }
    }

    #[test]
    fn parse_ics_includes_single_event_window() {
        let now = Local::now();
        let start = now + Duration::hours(2);
        let end = start + Duration::hours(1);
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:{}\r\nDTEND:{}\r\nSUMMARY:Deep Work\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            start.format("%Y%m%dT%H%M%S"),
            end.format("%Y%m%dT%H%M%S")
        );

        let windows = parse_ics_busy_windows(
            &source(),
            &ics,
            now - Duration::hours(1),
            now + Duration::days(1),
        )
        .expect("ICS parse should succeed");

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].summary, "Deep Work");
        assert_eq!(windows[0].start_epoch_secs, start.timestamp());
        assert_eq!(windows[0].end_epoch_secs, end.timestamp());
    }

    #[test]
    fn parse_ics_expands_daily_recurrence_with_count() {
        let day = Local::now().date_naive();
        let start = local_datetime(day, 9, 0);
        let end = local_datetime(day, 10, 0);
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:{}\r\nDTEND:{}\r\nRRULE:FREQ=DAILY;INTERVAL=1;COUNT=3\r\nSUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            start.format("%Y%m%dT%H%M%S"),
            end.format("%Y%m%dT%H%M%S")
        );

        let windows = parse_ics_busy_windows(
            &source(),
            &ics,
            start - Duration::hours(1),
            start + Duration::days(5),
        )
        .expect("ICS parse should succeed");

        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].start_epoch_secs, start.timestamp());
        assert_eq!(
            windows[1].start_epoch_secs,
            (start + Duration::days(1)).timestamp()
        );
        assert_eq!(
            windows[2].start_epoch_secs,
            (start + Duration::days(2)).timestamp()
        );
    }

    #[test]
    fn parse_ics_applies_weekly_byday_filter() {
        let start_day = Local::now().date_naive();
        let weekday = start_day.weekday();
        let other_day = if weekday == Weekday::Mon { "TU" } else { "MO" };
        let start = local_datetime(start_day, 14, 0);
        let end = local_datetime(start_day, 15, 0);
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:{}\r\nDTEND:{}\r\nRRULE:FREQ=WEEKLY;INTERVAL=1;COUNT=2;BYDAY={}\r\nSUMMARY:Weekly Call\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            start.format("%Y%m%dT%H%M%S"),
            end.format("%Y%m%dT%H%M%S"),
            weekday_to_token(weekday)
        );
        let no_match = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:{}\r\nDTEND:{}\r\nRRULE:FREQ=WEEKLY;INTERVAL=1;COUNT=2;BYDAY={}\r\nSUMMARY:No Match\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            start.format("%Y%m%dT%H%M%S"),
            end.format("%Y%m%dT%H%M%S"),
            other_day
        );

        let with_match = parse_ics_busy_windows(
            &source(),
            &ics,
            start - Duration::hours(1),
            start + Duration::days(14),
        )
        .expect("ICS parse should succeed");
        let shifted_weekday = parse_ics_busy_windows(
            &source(),
            &no_match,
            start - Duration::hours(1),
            start + Duration::days(14),
        )
        .expect("ICS parse should succeed");
        let expected_weekday = parse_weekday_token(other_day).expect("valid weekday token");

        assert_eq!(with_match.len(), 2);
        assert_eq!(shifted_weekday.len(), 2);
        assert!(shifted_weekday.iter().all(|window| {
            local_datetime_from_epoch(window.start_epoch_secs)
                .is_some_and(|dt| dt.weekday() == expected_weekday)
        }));
    }

    #[test]
    fn parse_ics_includes_weekly_occurrence_on_range_end_date() {
        let start_day = Local::now().date_naive();
        let Some(next_day) = start_day.succ_opt() else {
            panic!("expected next day");
        };
        let start = local_datetime(start_day, 10, 0);
        let end = local_datetime(start_day, 11, 0);
        let next_day_token = weekday_to_token(next_day.weekday());
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:{}\r\nDTEND:{}\r\nRRULE:FREQ=WEEKLY;INTERVAL=1;COUNT=1;BYDAY={}\r\nSUMMARY:Range End Weekly\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            start.format("%Y%m%dT%H%M%S"),
            end.format("%Y%m%dT%H%M%S"),
            next_day_token
        );

        let windows = parse_ics_busy_windows(
            &source(),
            &ics,
            start - Duration::hours(1),
            start + Duration::days(1) + Duration::hours(2),
        )
        .expect("ICS parse should succeed");

        let expected_start = local_datetime(next_day, 10, 0);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].start_epoch_secs, expected_start.timestamp());
    }

    #[test]
    fn parse_ics_converts_utc_timezone_values() {
        let utc_start = chrono::Utc::now() + Duration::hours(3);
        let utc_end = utc_start + Duration::hours(2);
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:{}\r\nDTEND:{}\r\nSUMMARY:UTC event\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            utc_start.format("%Y%m%dT%H%M%SZ"),
            utc_end.format("%Y%m%dT%H%M%SZ")
        );
        let local_start = utc_start.with_timezone(&Local);
        let windows = parse_ics_busy_windows(
            &source(),
            &ics,
            local_start - Duration::hours(1),
            local_start + Duration::days(1),
        )
        .expect("ICS parse should succeed");

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].start_epoch_secs, local_start.timestamp());
    }

    #[test]
    fn normalized_source_url_converts_webcal_scheme() {
        let source = CalendarSourceConfig {
            url: "webcal://example.com/cal.ics".to_string(),
            ..source()
        };
        assert_eq!(
            normalized_source_url(&source),
            "https://example.com/cal.ics".to_string()
        );
    }

    fn weekday_to_token(weekday: Weekday) -> &'static str {
        match weekday {
            Weekday::Mon => "MO",
            Weekday::Tue => "TU",
            Weekday::Wed => "WE",
            Weekday::Thu => "TH",
            Weekday::Fri => "FR",
            Weekday::Sat => "SA",
            Weekday::Sun => "SU",
        }
    }
}
