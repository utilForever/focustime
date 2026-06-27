use crate::stats::{
    BTreeMap, DailyGoalSnapshot, FocusSessionMetadata, FocusSessionRecord, FocusStats, ProfileId,
    SessionInterruptionEvent, SessionInterruptionReason, TimeOfDayBucket, month_key_for_day,
    normalize_task_label, task_label_index, week_key_for_day,
};
use chrono::Timelike;

impl FocusStats {
    pub(crate) fn record_focus_elapsed(
        &mut self,
        day_key: &str,
        elapsed_secs: u64,
        goal: DailyGoalSnapshot,
    ) {
        if elapsed_secs == 0 {
            return;
        }

        self.session.focused_seconds = self.session.focused_seconds.saturating_add(elapsed_secs);
        let daily = self.daily.entry(day_key.to_string()).or_default();
        daily.focused_seconds = daily.focused_seconds.saturating_add(elapsed_secs);
        daily.goal = Some(goal);
    }

    pub(crate) fn record_completed_pomodoro(&mut self, day_key: &str, goal: DailyGoalSnapshot) {
        self.record_completed_pomodoro_with_task(day_key, goal, None, 0, None);
    }

    pub(crate) fn record_completed_pomodoro_with_task(
        &mut self,
        day_key: &str,
        goal: DailyGoalSnapshot,
        task_label: Option<&str>,
        focused_seconds: u64,
        profile: Option<ProfileId>,
    ) {
        self.record_completed_pomodoro_with_metadata(
            day_key,
            goal,
            FocusSessionMetadata { task_label },
            focused_seconds,
            profile,
        );
    }

    pub(crate) fn record_completed_pomodoro_with_metadata(
        &mut self,
        day_key: &str,
        goal: DailyGoalSnapshot,
        metadata: FocusSessionMetadata<'_>,
        focused_seconds: u64,
        profile: Option<ProfileId>,
    ) {
        self.record_completed_pomodoro_with_metadata_at(
            day_key,
            goal,
            metadata,
            focused_seconds,
            profile,
            Some(current_timestamp_epoch_secs()),
        );
    }

    pub(crate) fn record_completed_pomodoro_with_metadata_at(
        &mut self,
        day_key: &str,
        goal: DailyGoalSnapshot,
        metadata: FocusSessionMetadata<'_>,
        focused_seconds: u64,
        profile: Option<ProfileId>,
        completion_timestamp_epoch_secs: Option<u64>,
    ) {
        self.session.pomodoros_completed = self.session.pomodoros_completed.saturating_add(1);
        let daily = self.daily.entry(day_key.to_string()).or_default();
        daily.pomodoros_completed = daily.pomodoros_completed.saturating_add(1);
        daily.goal = Some(goal);

        if let Some(task_label) = metadata.task_label.and_then(normalize_task_label) {
            if task_label_index(&self.task_labels, &task_label).is_none() {
                self.task_labels.push(task_label.clone());
            }
            self.selected_task_label = Some(task_label.clone());
            self.focus_sessions.push(FocusSessionRecord {
                date: day_key.to_string(),
                task_label,
                focused_seconds,
                profile,
                completion_timestamp_epoch_secs,
                completion_time_of_day_bucket: completion_time_of_day_bucket(
                    completion_timestamp_epoch_secs,
                ),
            });
        }
    }

    pub(crate) fn record_session_interruption_event(
        &mut self,
        day_key: &str,
        timestamp_epoch_secs: u64,
        reason: SessionInterruptionReason,
        metadata: FocusSessionMetadata<'_>,
        remaining_secs: u64,
        profile: Option<ProfileId>,
    ) {
        let normalized_task_label = metadata.task_label.and_then(normalize_task_label);
        if let Some(task_label) = normalized_task_label.as_ref()
            && task_label_index(&self.task_labels, task_label).is_none()
        {
            self.task_labels.push(task_label.clone());
        }

        self.session_interruptions.push(SessionInterruptionEvent {
            timestamp_epoch_secs,
            date: day_key.to_string(),
            reason,
            task_label: normalized_task_label,
            remaining_secs,
            profile,
        });
    }

    pub(crate) fn sync_goal_snapshot(&mut self, day_key: &str, goal: DailyGoalSnapshot) -> bool {
        let daily = self.daily.entry(day_key.to_string()).or_default();

        if daily.goal == Some(goal) {
            return false;
        }

        daily.goal = Some(goal);
        true
    }

    pub(crate) fn sync_weekly_goal_snapshot(
        &mut self,
        day: chrono::NaiveDate,
        goal: DailyGoalSnapshot,
    ) -> bool {
        let key = week_key_for_day(day);
        if self.weekly_goal_snapshots.get(&key) == Some(&goal) {
            return false;
        }
        self.weekly_goal_snapshots.insert(key, goal);
        true
    }

    pub(crate) fn sync_monthly_goal_snapshot(
        &mut self,
        day: chrono::NaiveDate,
        goal: DailyGoalSnapshot,
    ) -> bool {
        let key = month_key_for_day(day);
        if self.monthly_goal_snapshots.get(&key) == Some(&goal) {
            return false;
        }
        self.monthly_goal_snapshots.insert(key, goal);
        true
    }

    pub(crate) fn record_command_usage(&mut self, surface_id: &str) -> bool {
        record_usage_count(&mut self.command_usage_counts, surface_id)
    }

    pub(crate) fn record_screen_usage(&mut self, surface_id: &str) -> bool {
        record_usage_count(&mut self.screen_usage_counts, surface_id)
    }

    #[cfg(test)]
    pub(crate) fn command_usage_count(&self, surface_id: &str) -> u64 {
        let key = surface_id.trim().to_ascii_lowercase();
        if key.is_empty() {
            return 0;
        }
        self.command_usage_counts.get(&key).copied().unwrap_or(0)
    }
}

fn current_timestamp_epoch_secs() -> u64 {
    chrono::Local::now().timestamp().max(0) as u64
}

fn completion_time_of_day_bucket(epoch_secs: Option<u64>) -> Option<TimeOfDayBucket> {
    let epoch = i64::try_from(epoch_secs?).ok()?;
    let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp(epoch, 0)?;
    let local_time = timestamp.with_timezone(&chrono::Local);
    Some(TimeOfDayBucket::from_hour(local_time.hour()))
}

fn record_usage_count(counts: &mut BTreeMap<String, u64>, surface_id: &str) -> bool {
    let key = surface_id.trim().to_ascii_lowercase();
    if key.is_empty() {
        return false;
    }
    let entry = counts.entry(key).or_insert(0);
    *entry = entry.saturating_add(1);
    true
}
