use crate::stats::{
    BTreeMap, StatsGrowthSection, UsageSignalEntry, UsageSignalSummary, days_in_month,
    parse_week_label, percentage_round_nearest,
};

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn usage_signal_summary_for_counts(
    counts: &BTreeMap<String, u64>,
    limit: usize,
) -> UsageSignalSummary {
    let total_events = counts
        .values()
        .copied()
        .fold(0_u64, |total, value| total.saturating_add(value));
    let unique_surfaces = counts.len();
    let mut entries: Vec<(String, u64)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();

    let mut top_entries = entries.clone();
    top_entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    top_entries.truncate(limit);

    entries.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
    entries.truncate(limit);

    UsageSignalSummary {
        total_events,
        unique_surfaces,
        top: usage_signal_rows(top_entries, total_events),
        rare: usage_signal_rows(entries, total_events),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn usage_signal_rows(entries: Vec<(String, u64)>, total_events: u64) -> Vec<UsageSignalEntry> {
    entries
        .into_iter()
        .map(|(surface, count)| UsageSignalEntry {
            surface,
            count,
            share_pct: percentage_round_nearest(count, total_events),
        })
        .collect()
}

pub(super) fn stats_growth_section(
    name: &str,
    record_count: usize,
    value: &impl serde::Serialize,
) -> StatsGrowthSection {
    StatsGrowthSection {
        name: name.to_string(),
        record_count,
        estimated_bytes: estimated_serialized_bytes(value),
    }
}

pub(super) fn estimated_serialized_bytes(value: &impl serde::Serialize) -> u64 {
    #[derive(serde::Serialize)]
    struct SizeProbe<'a, T: ?Sized + serde::Serialize> {
        value: &'a T,
    }

    toml::to_string(&SizeProbe { value })
        .expect("stats growth section should be serializable")
        .len() as u64
}

pub(super) fn retention_cutoff_day(
    reference_day: chrono::NaiveDate,
    keep_days: u16,
) -> chrono::NaiveDate {
    let days_to_keep = i64::from(keep_days.max(1));
    reference_day
        .checked_sub_signed(chrono::Duration::days(days_to_keep.saturating_sub(1)))
        .unwrap_or(reference_day)
}

pub(super) fn is_day_key_on_or_after(day_key: &str, cutoff_day: chrono::NaiveDate) -> bool {
    chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d")
        .map(|day| day >= cutoff_day)
        .unwrap_or(true)
}

pub(super) fn is_week_key_on_or_after(week_key: &str, cutoff_day: chrono::NaiveDate) -> bool {
    let Some((year, week)) = parse_week_label(week_key) else {
        return true;
    };
    let Some(week_start) = chrono::NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)
    else {
        return true;
    };
    let week_end = week_start
        .checked_add_signed(chrono::Duration::days(6))
        .unwrap_or(week_start);
    week_end >= cutoff_day
}

pub(super) fn is_month_key_on_or_after(month_key: &str, cutoff_day: chrono::NaiveDate) -> bool {
    let Some((year_token, month_token)) = month_key.split_once('-') else {
        return true;
    };
    let Ok(year) = year_token.parse::<i32>() else {
        return true;
    };
    let Ok(month) = month_token.parse::<u32>() else {
        return true;
    };
    let Some(month_start) = chrono::NaiveDate::from_ymd_opt(year, month, 1) else {
        return true;
    };
    let month_end_day = days_in_month(year, month);
    let month_end =
        chrono::NaiveDate::from_ymd_opt(year, month, month_end_day).unwrap_or(month_start);
    month_end >= cutoff_day
}
