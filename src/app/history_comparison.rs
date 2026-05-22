use crate::app::{App, HistoryFeedbackLevel};
use crate::config::HistoryKpiCardId;
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

    pub fn history_dashboard_card_order(&self) -> &[HistoryKpiCardId] {
        &self.history_dashboard_card_order
    }

    pub fn history_dashboard_pinned_cards(&self) -> &[HistoryKpiCardId] {
        &self.history_dashboard_pinned_cards
    }

    pub fn history_dashboard_cards(&self) -> Vec<HistoryKpiCardId> {
        let mut cards = self.history_dashboard_pinned_cards.clone();
        for card in &self.history_dashboard_card_order {
            if !cards.contains(card) {
                cards.push(*card);
            }
        }
        cards
    }

    pub fn history_dashboard_selected_card(&self) -> HistoryKpiCardId {
        self.history_dashboard_selected_card
    }

    pub fn history_dashboard_card_is_pinned(&self, card: HistoryKpiCardId) -> bool {
        self.history_dashboard_pinned_cards.contains(&card)
    }

    pub(super) fn cycle_history_dashboard_selected_card(&mut self, forward: bool) {
        let cards = self.history_dashboard_cards();
        if cards.is_empty() {
            return;
        }
        let current_index = cards
            .iter()
            .position(|card| *card == self.history_dashboard_selected_card)
            .unwrap_or(0);
        let next_index = if forward {
            (current_index + 1) % cards.len()
        } else {
            (current_index + cards.len() - 1) % cards.len()
        };
        self.history_dashboard_selected_card = cards[next_index];
    }

    pub(super) fn toggle_history_dashboard_pin_for_selected_card(&mut self) {
        let selected = self.history_dashboard_selected_card;
        if let Some(index) = self
            .history_dashboard_pinned_cards
            .iter()
            .position(|card| *card == selected)
        {
            if self.history_dashboard_pinned_cards.len() <= 1 {
                self.set_history_feedback(
                    HistoryFeedbackLevel::Warning,
                    "At least one KPI card must remain pinned.",
                );
                return;
            }
            self.history_dashboard_pinned_cards.remove(index);
            self.set_history_feedback(
                HistoryFeedbackLevel::Success,
                format!("Unpinned KPI card `{}`.", selected.id()),
            );
            return;
        }

        let order_index = self
            .history_dashboard_card_order
            .iter()
            .position(|card| *card == selected)
            .unwrap_or(usize::MAX);
        let insert_at = self
            .history_dashboard_pinned_cards
            .iter()
            .position(|card| {
                self.history_dashboard_card_order
                    .iter()
                    .position(|candidate| candidate == card)
                    .unwrap_or(usize::MAX)
                    > order_index
            })
            .unwrap_or(self.history_dashboard_pinned_cards.len());
        self.history_dashboard_pinned_cards
            .insert(insert_at, selected);
        self.set_history_feedback(
            HistoryFeedbackLevel::Success,
            format!("Pinned KPI card `{}`.", selected.id()),
        );
    }

    pub(super) fn move_history_dashboard_selected_card(&mut self, right: bool) {
        let selected = self.history_dashboard_selected_card;
        let Some(index) = self
            .history_dashboard_pinned_cards
            .iter()
            .position(|card| *card == selected)
        else {
            self.set_history_feedback(
                HistoryFeedbackLevel::Warning,
                "Pin the selected KPI card before reordering it.",
            );
            return;
        };

        let target_index = if right {
            if index + 1 >= self.history_dashboard_pinned_cards.len() {
                self.set_history_feedback(
                    HistoryFeedbackLevel::Warning,
                    "Selected KPI card is already at the right edge.",
                );
                return;
            }
            index + 1
        } else {
            if index == 0 {
                self.set_history_feedback(
                    HistoryFeedbackLevel::Warning,
                    "Selected KPI card is already at the left edge.",
                );
                return;
            }
            index - 1
        };

        self.history_dashboard_pinned_cards
            .swap(index, target_index);
        self.set_history_feedback(
            HistoryFeedbackLevel::Success,
            format!("Moved KPI card `{}`.", selected.id()),
        );
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
