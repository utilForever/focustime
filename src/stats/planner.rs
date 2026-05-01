use crate::stats::{
    DailyGoalSnapshot, DailyStats, FocusStats, SessionStats, TaskGoalProgress,
    canonical_task_label, normalize_task_label, normalize_task_planner_state,
    planner_state_labels_for_keys, task_label_index,
};

impl FocusStats {
    pub fn session(&self) -> SessionStats {
        self.session
    }

    pub fn daily_for(&self, day_key: &str) -> DailyStats {
        self.daily.get(day_key).copied().unwrap_or_default()
    }

    pub fn daily_entry(&self, day_key: &str) -> Option<DailyStats> {
        self.daily.get(day_key).copied()
    }

    pub fn task_planner_state(&self) -> (Vec<String>, Option<String>) {
        (self.task_labels.clone(), self.selected_task_label.clone())
    }

    pub fn task_label_favorites(&self) -> Vec<String> {
        planner_state_labels_for_keys(&self.task_label_favorites, &self.task_labels)
    }

    pub fn task_label_archived(&self) -> Vec<String> {
        planner_state_labels_for_keys(&self.task_label_archived, &self.task_labels)
    }

    pub fn set_task_goal_target(
        &mut self,
        label: &str,
        target: DailyGoalSnapshot,
    ) -> Result<String, String> {
        let Some(normalized) = normalize_task_label(label) else {
            return Err("Task label cannot be empty.".to_string());
        };
        let canonical = canonical_task_label(&self.task_labels, &normalized).unwrap_or(normalized);
        if task_label_index(&self.task_labels, &canonical).is_none() {
            self.task_labels.push(canonical.clone());
        }
        self.selected_task_label = Some(canonical.clone());
        let key = canonical.to_ascii_lowercase();
        if target.has_any_target() {
            self.task_goal_targets.insert(key, target);
        } else {
            self.task_goal_targets.remove(&key);
        }
        Ok(canonical)
    }

    pub fn remove_task_goal_target(&mut self, label: &str) -> bool {
        let Some(normalized) = normalize_task_label(label) else {
            return false;
        };
        let canonical = canonical_task_label(&self.task_labels, &normalized).unwrap_or(normalized);
        self.task_goal_targets
            .remove(&canonical.to_ascii_lowercase())
            .is_some()
    }

    pub fn rename_task_goal_target(&mut self, previous_label: &str, next_label: &str) -> bool {
        let Some(previous_normalized) = normalize_task_label(previous_label) else {
            return false;
        };
        let Some(next_normalized) = normalize_task_label(next_label) else {
            return false;
        };
        let previous_canonical = canonical_task_label(&self.task_labels, &previous_normalized)
            .unwrap_or(previous_normalized);
        let next_canonical =
            canonical_task_label(&self.task_labels, &next_normalized).unwrap_or(next_normalized);
        let previous_key = previous_canonical.to_ascii_lowercase();
        let next_key = next_canonical.to_ascii_lowercase();
        if previous_key == next_key {
            return false;
        }

        let Some(target) = self.task_goal_targets.remove(&previous_key) else {
            return false;
        };
        self.task_goal_targets.insert(next_key, target);
        true
    }

    pub fn task_goal_progress_for_label(&self, label: &str) -> Option<TaskGoalProgress> {
        let normalized = normalize_task_label(label)?;
        let task_label = canonical_task_label(&self.task_labels, &normalized).unwrap_or(normalized);
        let key = task_label.to_ascii_lowercase();
        let target = self
            .task_goal_targets
            .get(&key)
            .copied()
            .unwrap_or_default();
        let totals_by_key = self.task_totals_by_key();
        let (pomodoros_completed, focused_seconds) =
            totals_by_key.get(&key).copied().unwrap_or((0, 0));
        Some(TaskGoalProgress {
            task_label,
            target,
            pomodoros_completed,
            focused_seconds,
            met: target.is_met_by_totals(focused_seconds / 60, pomodoros_completed),
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn task_goal_progress(&self, limit: usize) -> Vec<TaskGoalProgress> {
        if limit == 0 {
            return Vec::new();
        }

        let totals_by_key = self.task_totals_by_key();
        let mut progress: Vec<TaskGoalProgress> = self
            .task_goal_targets
            .iter()
            .filter_map(|(key, target)| {
                if !target.has_any_target() {
                    return None;
                }
                let (pomodoros_completed, focused_seconds) =
                    totals_by_key.get(key).copied().unwrap_or((0, 0));
                let task_label = self
                    .task_labels
                    .iter()
                    .find(|label| label.eq_ignore_ascii_case(key))
                    .cloned()
                    .unwrap_or_else(|| key.clone());
                Some(TaskGoalProgress {
                    task_label,
                    target: *target,
                    pomodoros_completed,
                    focused_seconds,
                    met: target.is_met_by_totals(focused_seconds / 60, pomodoros_completed),
                })
            })
            .collect();
        progress.sort_by(|left, right| {
            left.met
                .cmp(&right.met)
                .then_with(|| right.focused_seconds.cmp(&left.focused_seconds))
                .then_with(|| right.pomodoros_completed.cmp(&left.pomodoros_completed))
                .then_with(|| left.task_label.cmp(&right.task_label))
        });
        progress.truncate(limit);
        progress
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn update_task_planner_state(
        &mut self,
        labels: Vec<String>,
        selected: Option<String>,
    ) -> bool {
        self.update_task_planner_state_with_label_states(
            labels,
            selected,
            self.task_label_favorites(),
            self.task_label_archived(),
        )
    }

    pub fn update_task_planner_state_with_label_states(
        &mut self,
        labels: Vec<String>,
        selected: Option<String>,
        favorites: Vec<String>,
        archived: Vec<String>,
    ) -> bool {
        let (task_labels, selected_task_label, task_label_favorites, task_label_archived) =
            normalize_task_planner_state(labels, selected, favorites, archived);
        if self.task_labels == task_labels
            && self.selected_task_label == selected_task_label
            && self.task_label_favorites == task_label_favorites
            && self.task_label_archived == task_label_archived
        {
            return false;
        }

        self.task_labels = task_labels;
        self.selected_task_label = selected_task_label;
        self.task_label_favorites = task_label_favorites;
        self.task_label_archived = task_label_archived;
        true
    }
}
