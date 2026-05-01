use crate::stats::*;

pub(super) fn normalize_task_planner_state(
    labels: Vec<String>,
    selected: Option<String>,
    favorites: Vec<String>,
    archived: Vec<String>,
) -> (
    Vec<String>,
    Option<String>,
    BTreeSet<String>,
    BTreeSet<String>,
) {
    let mut normalized_labels = Vec::new();
    let mut seen = BTreeSet::new();
    for label in labels {
        let Some(label) = normalize_task_label(&label) else {
            continue;
        };
        let key = label.to_ascii_lowercase();
        if seen.insert(key) {
            normalized_labels.push(label);
        }
    }

    let mut normalized_selected = selected
        .and_then(|value| normalize_task_label(&value))
        .map(|value| canonical_task_label(&normalized_labels, &value).unwrap_or(value));
    if let Some(selected_label) = normalized_selected.as_ref() {
        let key = selected_label.to_ascii_lowercase();
        if seen.insert(key) {
            normalized_labels.push(selected_label.clone());
        }
    }

    let task_label_favorites =
        normalize_task_label_state_keys(&mut normalized_labels, &mut seen, favorites);
    let task_label_archived =
        normalize_task_label_state_keys(&mut normalized_labels, &mut seen, archived);
    if normalized_selected.as_ref().is_some_and(|selected_label| {
        task_label_archived.contains(&selected_label.to_ascii_lowercase())
    }) {
        normalized_selected = None;
    }

    (
        normalized_labels,
        normalized_selected,
        task_label_favorites,
        task_label_archived,
    )
}

fn normalize_task_label_state_keys(
    normalized_labels: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    values: Vec<String>,
) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for value in values {
        let Some(normalized) = normalize_task_label(&value) else {
            continue;
        };
        let canonical = canonical_task_label(normalized_labels, &normalized).unwrap_or(normalized);
        let key = canonical.to_ascii_lowercase();
        if seen.insert(key.clone()) {
            normalized_labels.push(canonical);
        }
        keys.insert(key);
    }
    keys
}

pub(super) fn planner_state_labels_for_keys(
    keys: &BTreeSet<String>,
    labels: &[String],
) -> Vec<String> {
    if keys.is_empty() {
        return Vec::new();
    }

    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    for label in labels {
        let key = label.to_ascii_lowercase();
        if keys.contains(&key) && seen.insert(key.clone()) {
            values.push(label.clone());
        }
    }
    for key in keys {
        if seen.insert(key.clone()) {
            values.push(key.clone());
        }
    }
    values
}

pub(super) fn normalize_task_goal_targets(
    task_goal_targets: BTreeMap<String, DailyGoalSnapshot>,
) -> BTreeMap<String, DailyGoalSnapshot> {
    let mut normalized = BTreeMap::new();
    for (label, target) in task_goal_targets {
        if !target.has_any_target() {
            continue;
        }
        let Some(label) = normalize_task_label(&label) else {
            continue;
        };
        normalized.insert(label.to_ascii_lowercase(), target);
    }
    normalized
}

pub(super) fn normalize_session_metadata_text(input: &str) -> Option<String> {
    let trimmed = input.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub(super) fn current_goal_streak(
    completed_days: &BTreeSet<chrono::NaiveDate>,
    today: chrono::NaiveDate,
    current_goal: DailyGoalSnapshot,
    today_stats: DailyStats,
) -> u32 {
    if !current_goal.has_any_target() {
        return 0;
    }

    let mut streak = 0;
    let mut cursor = if current_goal.is_met_by(today_stats) {
        Some(today)
    } else {
        today.pred_opt()
    };

    while let Some(day) = cursor {
        if !completed_days.contains(&day) {
            break;
        }
        streak += 1;
        cursor = day.pred_opt();
    }

    streak
}

pub(super) fn best_goal_streak(completed_days: &BTreeSet<chrono::NaiveDate>) -> u32 {
    let mut best = 0;
    let mut streak = 0;
    let mut previous_day: Option<chrono::NaiveDate> = None;

    for day in completed_days {
        if previous_day.is_some_and(|previous| previous.succ_opt() == Some(*day)) {
            streak += 1;
        } else {
            streak = 1;
        }

        best = best.max(streak);
        previous_day = Some(*day);
    }

    best
}

pub(super) fn profile_bucket_for(profile: Option<ProfileId>) -> ProfileBucket {
    match profile {
        Some(ProfileId::Classic) => ProfileBucket::Classic,
        Some(ProfileId::DeepWork) => ProfileBucket::DeepWork,
        Some(ProfileId::Custom) => ProfileBucket::Custom,
        None => ProfileBucket::Unknown,
    }
}

pub(super) fn daily_has_activity(stats: DailyStats) -> bool {
    stats.focused_seconds > 0 || stats.pomodoros_completed > 0
}

pub(super) fn percentage_round_nearest(part: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    let rounded = (u128::from(part) * 100 + (u128::from(total) / 2)) / u128::from(total);
    rounded.min(u128::from(u8::MAX)) as u8
}

pub(super) fn consistency_score_from_active_days(active_days: u8) -> u8 {
    let capped_days = active_days.min(7);
    let rounded = (u32::from(capped_days) * 100 + 3) / 7;
    rounded.min(u32::from(u8::MAX)) as u8
}

pub(super) fn weekly_completion_score_pct(
    goal: DailyGoalSnapshot,
    totals: WeeklyStats,
) -> Option<u8> {
    let minute_score = if goal.minutes > 0 {
        let completed_minutes = totals.focused_minutes().min(goal.minutes);
        Some(percentage_round_nearest(completed_minutes, goal.minutes))
    } else {
        None
    };
    let pomodoro_score = if goal.pomodoros > 0 {
        let completed_pomodoros = totals.pomodoros_completed.min(goal.pomodoros);
        Some(percentage_round_nearest(
            u64::from(completed_pomodoros),
            u64::from(goal.pomodoros),
        ))
    } else {
        None
    };
    match (minute_score, pomodoro_score) {
        (None, None) => None,
        (Some(score), None) | (None, Some(score)) => Some(score),
        (Some(left), Some(right)) => Some(average_two_percentages(left, right)),
    }
}

pub(super) fn average_two_percentages(left: u8, right: u8) -> u8 {
    let sum = u16::from(left) + u16::from(right);
    sum.div_ceil(2) as u8
}

pub(super) fn format_week_label(year: i32, week: u32) -> String {
    format!("{year:04}-W{week:02}")
}

pub(super) fn parse_week_label(week_label: &str) -> Option<(i32, u32)> {
    let (year, week) = week_label.split_once("-W")?;
    let parsed_year = year.parse::<i32>().ok()?;
    let parsed_week = week.parse::<u32>().ok()?;
    Some((parsed_year, parsed_week))
}

pub(super) fn week_key_for_day(day: chrono::NaiveDate) -> String {
    let week = day.iso_week();
    format_week_label(week.year(), week.week())
}

pub(super) fn month_key_for_day(day: chrono::NaiveDate) -> String {
    format!("{:04}-{:02}", day.year(), day.month())
}

pub(super) fn write_atomic_bytes(path: &Path, content: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let (tmp_path, mut tmp_file) = create_unique_temp_file(path)?;
    tmp_file.write_all(content)?;
    tmp_file.flush()?;
    drop(tmp_file);

    #[cfg(target_os = "windows")]
    {
        match fs::rename(&tmp_path, path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(path)?;
                fs::rename(&tmp_path, path)
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                Err(e)
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        match fs::rename(&tmp_path, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                Err(e)
            }
        }
    }
}

pub(super) fn create_unique_temp_file(path: &Path) -> io::Result<(PathBuf, fs::File)> {
    const MAX_ATTEMPTS: usize = 32;

    for _ in 0..MAX_ATTEMPTS {
        let candidate = create_unique_temp_path(path);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "failed to allocate unique temporary export path",
    ))
}

pub(super) fn create_unique_temp_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let target_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("focustime-export");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let seq = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".{target_name}.{pid}.{nanos}.{seq}.tmp"))
}

pub(super) fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year.saturating_add(1), 1)
    } else {
        (year, month.saturating_add(1))
    };
    let next_month_start = chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("validated month rollover should produce next month start");
    next_month_start
        .pred_opt()
        .expect("month start should have a predecessor")
        .day()
}
