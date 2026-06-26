use crate::stats::{
    DailyStats, FocusStats, SessionStats, normalize_task_planner_state,
    planner_state_labels_for_keys,
};

impl FocusStats {
    pub(crate) fn session(&self) -> SessionStats {
        self.session
    }

    pub(crate) fn daily_for(&self, day_key: &str) -> DailyStats {
        self.daily.get(day_key).copied().unwrap_or_default()
    }

    pub(crate) fn daily_entry(&self, day_key: &str) -> Option<DailyStats> {
        self.daily.get(day_key).copied()
    }

    pub(crate) fn task_planner_state(&self) -> (Vec<String>, Option<String>) {
        (self.task_labels.clone(), self.selected_task_label.clone())
    }

    pub(crate) fn task_label_favorites(&self) -> Vec<String> {
        planner_state_labels_for_keys(&self.task_label_favorites, &self.task_labels)
    }

    pub(crate) fn task_label_archived(&self) -> Vec<String> {
        planner_state_labels_for_keys(&self.task_label_archived, &self.task_labels)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn update_task_planner_state(
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

    pub(crate) fn update_task_planner_state_with_label_states(
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
