use crate::app::App;
use crate::stats::{
    ComparisonDimension, ProductivityComparisonFilter, ProductivityComparisonRow, ProfileBucket,
    TimeOfDayBucket,
};

fn cycle_optional_selection<T: Clone + PartialEq>(
    options: Vec<Option<T>>,
    current: Option<T>,
    forward: bool,
) -> Option<T> {
    if options.is_empty() {
        return current;
    }
    let index = options
        .iter()
        .position(|entry| *entry == current)
        .unwrap_or(0);
    let next = if forward {
        (index + 1) % options.len()
    } else {
        (index + options.len() - 1) % options.len()
    };
    options[next].clone()
}

impl App {
    pub fn history_comparison_dimension(&self) -> ComparisonDimension {
        self.history_comparison_dimension
    }

    pub fn history_comparison_rows(&self, limit: usize) -> Vec<ProductivityComparisonRow> {
        let filter = ProductivityComparisonFilter {
            task_label: self.history_task_filter.clone(),
            profile: self.history_profile_filter,
            time_of_day: self.history_time_of_day_filter,
        };
        self.stats
            .productivity_comparison(self.history_comparison_dimension, &filter, limit)
    }

    pub fn history_comparison_filter_summary(&self) -> String {
        let task = self
            .history_task_filter
            .as_deref()
            .unwrap_or("all")
            .to_string();
        let profile = self
            .history_profile_filter
            .map(|value| value.label().to_string())
            .unwrap_or_else(|| "All".to_string());
        let time_of_day = self
            .history_time_of_day_filter
            .map(|value| value.label().to_string())
            .unwrap_or_else(|| "All".to_string());
        format!("Slices: task {task} · profile {profile} · time {time_of_day}")
    }

    pub(super) fn cycle_history_comparison_dimension(&mut self, forward: bool) {
        const DIMENSIONS: [ComparisonDimension; 3] = [
            ComparisonDimension::TaskLabel,
            ComparisonDimension::Profile,
            ComparisonDimension::TimeOfDay,
        ];
        let index = DIMENSIONS
            .iter()
            .position(|dimension| *dimension == self.history_comparison_dimension)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % DIMENSIONS.len()
        } else {
            (index + DIMENSIONS.len() - 1) % DIMENSIONS.len()
        };
        self.history_comparison_dimension = DIMENSIONS[next];
    }

    pub(super) fn cycle_history_task_filter(&mut self, forward: bool) {
        let mut options: Vec<Option<String>> = vec![None];
        let mut labels = self.task_labels.clone();
        labels.sort_by_key(|label| label.to_ascii_lowercase());
        labels.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        options.extend(labels.into_iter().map(Some));
        self.history_task_filter =
            cycle_optional_selection(options, self.history_task_filter.clone(), forward);
    }

    pub(super) fn cycle_history_profile_filter(&mut self, forward: bool) {
        let mut options: Vec<Option<ProfileBucket>> = vec![None];
        for bucket in [
            ProfileBucket::Classic,
            ProfileBucket::DeepWork,
            ProfileBucket::Custom,
            ProfileBucket::Unknown,
        ] {
            options.push(Some(bucket));
        }
        self.history_profile_filter =
            cycle_optional_selection(options, self.history_profile_filter, forward);
    }

    pub(super) fn cycle_history_time_of_day_filter(&mut self, forward: bool) {
        let options = vec![
            None,
            Some(TimeOfDayBucket::Morning),
            Some(TimeOfDayBucket::Afternoon),
            Some(TimeOfDayBucket::Evening),
            Some(TimeOfDayBucket::Night),
            Some(TimeOfDayBucket::Unknown),
        ];
        self.history_time_of_day_filter =
            cycle_optional_selection(options, self.history_time_of_day_filter, forward);
    }
}
