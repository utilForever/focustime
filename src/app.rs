use std::{cell::RefCell, collections::BTreeSet};

use chrono::{DateTime, Datelike, Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::blocker::{
    BlockingIntent, BlockingPreview, BulkAddResult, EditSiteResult, InvalidSiteInput, SiteBlocker,
};
use crate::config::{
    AppConfig, AutoStartConfig, BlocklistProfileConfig, CustomProfileConfig, DailyGoalConfig,
    FeatureFlagsConfig, GoalCarryOverConfig, HistoryDashboardConfig, MonthlyGoalConfig,
    NotificationConfig, ProfileAutomationConfig, ProfileAutomationSettingsConfig, ProfileId,
    RecurringFocusWindowConfig, RecurringScheduleConfig, ScheduleRuntimeConfig,
    StatsRetentionConfig, ThemePreset, WeeklyGoalConfig,
};
use crate::notifications::PhaseNotifier;
use crate::schedule::{
    RecurringWindow, WindowOccurrence, active_occurrence, compile_windows, next_occurrence_after,
    occurrence_key,
};
use crate::stats::{
    ComparisonDimension, DailyGoalSnapshot, DailyStats, ExportedStatsFiles, FocusRiskForecast,
    FocusSessionMetadata, FocusStats, GoalStreak, MonthlyHeatmap, MonthlyStats, ProfileBucket,
    ProfileEffectiveness, ProfileTotals, SessionInterruptionEvent, SessionInterruptionReason,
    SessionStats, StatsGrowthSummary, StatsRetentionPruneResult, TaskTotals, TaskTrend,
    TimeOfDayBucket, WeeklyConsistency, WeeklyFocusScore, WeeklyStats, carry_over_goal_target,
    current_day_key,
};
use crate::task_labels::{normalize_task_label, task_label_index};
use crate::timer::{
    DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS,
    DEFAULT_SHORT_BREAK_SECS, TimerPhase, TimerState, TimerStatus,
};
mod cli_api;
mod error;
mod feedback_diagnostics;
mod history_comparison;
mod history_dashboard_cache;
mod history_goals;
mod mode_keys;
mod persistence;
mod planner_labels;
mod profile_edit;
mod profile_management;
mod schedule_editor;
mod schedule_runtime;
mod session_planner;
mod setup_diagnostics;
mod shortcuts;
mod site_manager;
mod timer_flow;

pub(crate) use error::{AppError, AppResult};
use history_dashboard_cache::HistoryDashboardCache;
#[cfg(test)]
pub(crate) use history_dashboard_cache::HistoryDashboardCacheStats;
pub(crate) use history_dashboard_cache::HistoryDashboardViewData;
pub(crate) use profile_edit::{
    CUSTOM_DURATION_STEP_SECS, DAILY_GOAL_MINUTES_STEP, PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX,
    PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX, PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX,
    PROFILE_EDIT_FIELD_LABELS, PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX,
    PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX, PROFILE_EDIT_SCHEDULE_DAY_INDEX,
    PROFILE_EDIT_SCHEDULE_END_INDEX, PROFILE_EDIT_SCHEDULE_START_INDEX,
    PROFILE_EDIT_SCHEDULE_WINDOW_INDEX, PROFILE_EDIT_THEME_PRESET_INDEX, ProfileEditSnapshot,
};
pub(crate) use setup_diagnostics::{
    BlockingPreviewSnapshot, SetupCheck, SetupCheckLevel, SetupDiagnostics,
};
use shortcuts::ShortcutBindings;
pub(crate) use shortcuts::{NavigationAction, ShortcutAction};

pub(crate) const PROFILE_IDS: [ProfileId; 3] =
    [ProfileId::Classic, ProfileId::DeepWork, ProfileId::Custom];
const DEFAULT_BLOCKLIST_PROFILE_NAME: &str = "Default";
#[cfg(not(test))]
const STATS_FILE_NAME: &str = "stats.toml";
pub(crate) const PLANNER_RECENT_LABEL_LIMIT: usize = 5;
const SCHEDULE_DAY_TOKENS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const SCHEDULE_DAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppMode {
    Timer,
    SiteManager,
    ProfileManager,
    TaskSetup,
    StatsHistory,
    SetupDiagnostics,
}

#[derive(Debug, Clone, Copy)]
struct ProfileSpec {
    focus_secs: u64,
    short_break_secs: u64,
    long_break_secs: u64,
    long_break_interval: u32,
}

impl ProfileSpec {
    fn from_custom(custom: &CustomProfileConfig) -> Self {
        let custom = custom.normalized();
        Self {
            focus_secs: custom.focus_secs,
            short_break_secs: custom.short_break_secs,
            long_break_secs: custom.long_break_secs,
            long_break_interval: custom.long_break_interval,
        }
    }
}

fn built_in_profile_spec(profile: ProfileId) -> Option<ProfileSpec> {
    match profile {
        ProfileId::Classic => Some(ProfileSpec {
            focus_secs: DEFAULT_FOCUS_SECS,
            short_break_secs: DEFAULT_SHORT_BREAK_SECS,
            long_break_secs: DEFAULT_LONG_BREAK_SECS,
            long_break_interval: DEFAULT_LONG_BREAK_INTERVAL,
        }),
        ProfileId::DeepWork => Some(ProfileSpec {
            focus_secs: 50 * 60,
            short_break_secs: 10 * 60,
            long_break_secs: 30 * 60,
            long_break_interval: 3,
        }),
        ProfileId::Custom => None,
    }
}

fn profile_spec_for(profile: ProfileId, custom_profile: &CustomProfileConfig) -> ProfileSpec {
    built_in_profile_spec(profile).unwrap_or_else(|| ProfileSpec::from_custom(custom_profile))
}

fn profile_index(profile: ProfileId) -> usize {
    PROFILE_IDS
        .iter()
        .position(|candidate| *candidate == profile)
        .unwrap_or(PROFILE_IDS.len() - 1)
}

fn profile_for_index(index: usize) -> ProfileId {
    PROFILE_IDS
        .get(index)
        .copied()
        .unwrap_or(PROFILE_IDS[PROFILE_IDS.len() - 1])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimerActivity {
    focus_active: bool,
    focus_running: bool,
}

impl TimerActivity {
    fn from_timer(timer: &TimerState) -> Self {
        Self::from_parts(timer.phase, timer.status)
    }

    fn from_parts(phase: TimerPhase, status: TimerStatus) -> Self {
        let focus_active = phase == TimerPhase::Focus && status != TimerStatus::Idle;
        let focus_running = phase == TimerPhase::Focus && status == TimerStatus::Running;
        debug_assert!(!focus_running || focus_active);
        Self {
            focus_active,
            focus_running,
        }
    }
}

fn blocklist_profile_index(profiles: &[BlocklistProfileConfig], selected_name: &str) -> usize {
    profiles
        .iter()
        .position(|profile| profile.name.eq_ignore_ascii_case(selected_name))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingTimerAction {
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusStartOutcome {
    Started,
    MissingTaskLabel,
    NotIdleFocusPhase,
}

#[derive(Debug, Clone)]
struct ScheduleDisplayState {
    has_schedule_windows: bool,
    active_window: Option<WindowOccurrence>,
    next_window: Option<WindowOccurrence>,
    is_armed: bool,
    has_selected_task: bool,
    timer_phase: TimerPhase,
    timer_status: TimerStatus,
}

#[derive(Debug, Clone)]
struct FocusInterruptionContext {
    day_key: String,
    timestamp_epoch_secs: u64,
    reason: SessionInterruptionReason,
    task_label: Option<String>,
    remaining_secs: u64,
    profile: Option<ProfileId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SiteInputMode {
    Add,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SiteListMode {
    Blocklist,
    Allowlist,
}

impl SiteListMode {
    fn toggle(self) -> Self {
        match self {
            Self::Blocklist => Self::Allowlist,
            Self::Allowlist => Self::Blocklist,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Blocklist => "Blocklist",
            Self::Allowlist => "Allowlist",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlannerInputMode {
    Add,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SiteFeedbackLevel {
    Success,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryFeedbackLevel {
    Success,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlannerFeedbackLevel {
    Success,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SiteFeedback {
    pub(crate) level: SiteFeedbackLevel,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannerFeedback {
    pub(crate) level: PlannerFeedbackLevel,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryFeedback {
    pub(crate) level: HistoryFeedbackLevel,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GoalProgress {
    pub(crate) completed: u64,
    pub(crate) target: u64,
    pub(crate) ratio: f64,
}

impl GoalProgress {
    pub(crate) fn is_configured(self) -> bool {
        self.target > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DailyGoalProgress {
    pub(crate) minutes: GoalProgress,
    pub(crate) pomodoros: GoalProgress,
}

impl DailyGoalProgress {
    pub(crate) fn has_any_target(self) -> bool {
        self.minutes.is_configured() || self.pomodoros.is_configured()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeeklyDailyAllocationDay {
    pub(crate) day: NaiveDate,
    pub(crate) minutes_target: u64,
    pub(crate) pomodoros_target: u32,
    pub(crate) allocatable: bool,
    pub(crate) weight_minutes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeeklyDailyGoalAllocation {
    pub(crate) week_target: DailyGoalSnapshot,
    pub(crate) completed_minutes: u64,
    pub(crate) completed_pomodoros: u32,
    pub(crate) remaining_minutes: u64,
    pub(crate) remaining_pomodoros: u32,
    pub(crate) remaining_days_in_week: usize,
    pub(crate) allocatable_days: usize,
    pub(crate) uses_schedule_weights: bool,
    pub(crate) daily_targets: Vec<WeeklyDailyAllocationDay>,
}

pub(crate) struct App {
    pub(crate) timer: TimerState,
    pub(crate) should_quit: bool,
    pub(crate) mode: AppMode,
    pub(crate) blocker: SiteBlocker,
    /// Text being typed for add/import or edit site input.
    pub(crate) site_input: String,
    /// Whether the user is currently typing a new site.
    pub(crate) site_input_active: bool,
    site_edit_index: Option<usize>,
    pub(crate) blocklist_profiles: Vec<BlocklistProfileConfig>,
    active_blocklist_profile: usize,
    pub(crate) site_feedback: Option<SiteFeedback>,
    pub(crate) task_labels: Vec<String>,
    pub(crate) selected_task_label: Option<String>,
    task_label_favorites: BTreeSet<String>,
    task_label_archived: BTreeSet<String>,
    pub(crate) planner_selection_index: usize,
    pub(crate) planner_input: String,
    pub(crate) planner_input_active: bool,
    pub(crate) planner_input_mode: Option<PlannerInputMode>,
    pub(crate) planner_feedback: Option<PlannerFeedback>,
    active_focus_task_label: Option<String>,
    active_focus_profile: Option<ProfileId>,
    site_list_mode: SiteListMode,
    /// Index of the highlighted site in the SiteManager list.
    pub(crate) selected_site: usize,
    /// Last error from a block/unblock operation (e.g. permission denied).
    pub(crate) block_error: Option<String>,
    pub(crate) setup_diagnostics: SetupDiagnostics,
    pub(crate) blocking_preview: BlockingPreviewSnapshot,
    /// Last error from persisting timer/site configuration.
    pub(crate) config_error: Option<String>,
    /// Last error from persisting focus stats.
    pub(crate) stats_error: Option<String>,
    pub(crate) history_feedback: Option<HistoryFeedback>,
    history_comparison_dimension: ComparisonDimension,
    history_task_filter: Option<String>,
    history_profile_filter: Option<ProfileBucket>,
    history_time_of_day_filter: Option<TimeOfDayBucket>,
    history_dashboard_cache: RefCell<HistoryDashboardCache>,
    pub(crate) phase_notification: Option<String>,
    pub(crate) selected_profile: ProfileId,
    selected_theme_preset: ThemePreset,
    feature_flags: FeatureFlagsConfig,
    config_deprecation_warnings: Vec<String>,
    profile_automation: ProfileAutomationSettingsConfig,
    pub(crate) custom_profile: CustomProfileConfig,
    pub(crate) profile_selection_index: usize,
    pub(crate) profile_edit_active: bool,
    pub(crate) profile_edit_field: usize,
    profile_edit_schedule_window: usize,
    profile_edit_schedule_day: usize,
    profile_edit_snapshot: Option<ProfileEditSnapshot>,
    notification_settings: NotificationConfig,
    auto_start: AutoStartConfig,
    recurring_schedule: RecurringScheduleConfig,
    schedule_runtime: ScheduleRuntimeConfig,
    recurring_windows: Vec<RecurringWindow>,
    schedule_armed_occurrence_key: Option<String>,
    last_schedule_occurrence_key: Option<String>,
    last_active_schedule_occurrence_key: Option<String>,
    current_frame_now: DateTime<Local>,
    pub(crate) strict_mode: bool,
    daily_goal: DailyGoalConfig,
    weekly_goal: WeeklyGoalConfig,
    monthly_goal: MonthlyGoalConfig,
    goal_carry_over: GoalCarryOverConfig,
    stats_retention: StatsRetentionConfig,
    pending_timer_action: Option<PendingTimerAction>,
    notifier: PhaseNotifier,
    stats: FocusStats,
    stats_revision: u64,
    stats_dirty: bool,
    stats_has_unsaved_elapsed: bool,
    shortcuts: ShortcutBindings,
}

impl App {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        {
            Self::from_config(AppConfig::default())
        }
        #[cfg(not(test))]
        {
            let (config, deprecation_warnings) = AppConfig::load_with_deprecation_warnings();
            Self::from_config_with_deprecation_warnings(config, deprecation_warnings)
        }
    }

    #[cfg(test)]
    fn from_config(config: AppConfig) -> Self {
        Self::from_config_with_deprecation_warnings(config, Vec::new())
    }

    fn from_config_with_deprecation_warnings(
        config: AppConfig,
        config_deprecation_warnings: Vec<String>,
    ) -> Self {
        let config = config.normalized();
        let selected_profile = config.selected_profile;
        let selected_theme_preset = config.selected_theme_preset;
        let feature_flags = config.feature_flags.clone();
        let custom_profile = config.effective_custom_profile();
        let profile_automation = config.profile_automation.clone().unwrap_or_default();
        let selected_automation = config.profile_automation_for(selected_profile);
        let notification_settings = selected_automation.notifications;
        let auto_start = selected_automation.auto_start;
        let recurring_schedule = selected_automation.recurring_schedule.clone();
        let recurring_windows = compile_windows(&recurring_schedule.windows);
        let strict_mode = selected_automation.strict_mode;
        let schedule_runtime = config.schedule_runtime;
        let daily_goal = config.daily_goal;
        let weekly_goal = config.weekly_goal;
        let monthly_goal = config.monthly_goal;
        let goal_carry_over = config.goal_carry_over;
        let stats_retention = config.stats_retention;
        let blocklist_profiles = config.blocklist_profiles.clone();
        let active_blocklist_profile =
            blocklist_profile_index(&blocklist_profiles, &config.selected_blocklist_profile);
        let (shortcuts, shortcut_diagnostics) =
            ShortcutBindings::from_config_with_diagnostics(&config.shortcuts);
        let shortcut_config_error = (!shortcut_diagnostics.is_empty()).then(|| {
            format!(
                "shortcut config adjusted: {}",
                shortcut_diagnostics.join(" ")
            )
        });
        let initial_config_error = [shortcut_config_error]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let initial_config_error =
            (!initial_config_error.trim().is_empty()).then_some(initial_config_error);
        let profile_spec = profile_spec_for(selected_profile, &custom_profile);
        let (mut stats, stats_error) =
            match FocusStats::load_with_options(crate::stats::StatsLoadOptions::default()) {
                Ok(stats) => (stats, legacy_stats_path_migration_warning()),
                Err(e) => (FocusStats::default(), Some(e)),
            };
        let retained = stats.apply_retention_policy(stats_retention, Local::now().date_naive());
        let setup_deprecation_warnings =
            setup_deprecation_warnings(&config_deprecation_warnings, &stats);
        let (task_labels, selected_task_label) = stats.task_planner_state();
        let task_label_favorites = task_label_state_keys(stats.task_label_favorites());
        let task_label_archived = task_label_state_keys(stats.task_label_archived());
        let planner_selection_index = 0;
        let timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        let blocker = SiteBlocker::new();
        let setup_diagnostics =
            SetupDiagnostics::collect(&blocker, setup_deprecation_warnings.clone());
        let mut app = Self {
            timer,
            should_quit: false,
            mode: AppMode::Timer,
            blocker,
            site_input: String::new(),
            site_input_active: false,
            site_edit_index: None,
            blocklist_profiles,
            active_blocklist_profile,
            site_feedback: None,
            task_labels,
            selected_task_label,
            task_label_favorites,
            task_label_archived,
            planner_selection_index,
            planner_input: String::new(),
            planner_input_active: false,
            planner_input_mode: None,
            planner_feedback: None,
            active_focus_task_label: None,
            active_focus_profile: None,
            site_list_mode: SiteListMode::Blocklist,
            selected_site: 0,
            block_error: None,
            setup_diagnostics,
            blocking_preview: BlockingPreviewSnapshot::default(),
            config_error: initial_config_error,
            stats_error,
            history_feedback: None,
            history_comparison_dimension: ComparisonDimension::TaskLabel,
            history_task_filter: None,
            history_profile_filter: None,
            history_time_of_day_filter: None,
            history_dashboard_cache: RefCell::new(HistoryDashboardCache::default()),
            phase_notification: None,
            selected_profile,
            selected_theme_preset,
            feature_flags,
            config_deprecation_warnings,
            profile_automation,
            custom_profile,
            profile_selection_index: profile_index(selected_profile),
            profile_edit_active: false,
            profile_edit_field: 0,
            profile_edit_schedule_window: 0,
            profile_edit_schedule_day: 0,
            profile_edit_snapshot: None,
            notification_settings,
            auto_start,
            recurring_schedule,
            schedule_runtime,
            recurring_windows,
            schedule_armed_occurrence_key: None,
            last_schedule_occurrence_key: None,
            last_active_schedule_occurrence_key: None,
            current_frame_now: Local::now(),
            strict_mode,
            daily_goal,
            weekly_goal,
            monthly_goal,
            goal_carry_over,
            stats_retention,
            pending_timer_action: None,
            notifier: PhaseNotifier::new(notification_settings),
            stats,
            stats_revision: 0,
            stats_dirty: retained.any_removed(),
            stats_has_unsaved_elapsed: false,
            shortcuts,
        };
        app.recompute_blocker_sites_from_active_profile();
        app.restore_in_progress_session();
        app.restore_cli_workflow_state();
        app.sync_planner_selection_to_selected_label();
        app.sync_recovery_snapshot();
        app.apply_blocking_for_phase();
        app.refresh_setup_diagnostics();
        app.sync_today_goal_snapshot();
        app
    }

    #[cfg(test)]
    pub(crate) fn from_config_for_tests(config: AppConfig) -> Self {
        Self::from_config(config)
    }

    pub(crate) fn on_runtime_elapsed(&mut self, _elapsed_secs: u64) {}

    /// Intended to be called once per UI frame.
    pub(crate) fn poll_runtime_status(&mut self) {
        let now = Local::now();
        self.current_frame_now = now;
        self.sync_today_goal_snapshot();
        self.sync_recurring_schedule(now);
    }

    pub(crate) fn selected_profile_name(&self) -> &'static str {
        self.selected_profile.label()
    }

    pub(crate) fn selected_theme_preset(&self) -> ThemePreset {
        self.selected_theme_preset
    }

    pub(crate) fn current_task_label(&self) -> Option<&str> {
        if self.focus_session_active_for_current_state() {
            self.active_focus_task_label
                .as_deref()
                .or(self.selected_task_label.as_deref())
        } else {
            self.selected_task_label.as_deref()
        }
    }

    pub(super) fn shortcut_matches(&self, action: ShortcutAction, key: &KeyEvent) -> bool {
        self.shortcuts.matches(action, key)
    }

    pub(crate) fn shortcut_hint(&self, action: ShortcutAction) -> String {
        self.shortcuts.hint(action)
    }

    pub(crate) fn shortcut_label(&self, action: ShortcutAction) -> String {
        self.shortcuts.label(action)
    }

    pub(crate) fn navigation_matches(&self, action: NavigationAction, key: &KeyEvent) -> bool {
        self.shortcuts.navigation_matches(action, key)
    }

    pub(crate) fn navigation_hint(&self, action: NavigationAction) -> String {
        self.shortcuts.navigation_hint(action)
    }

    pub(crate) fn navigation_label(&self, action: NavigationAction) -> String {
        self.shortcuts.navigation_label(action)
    }

    pub(crate) fn profile_values(&self, profile: ProfileId) -> (u64, u64, u64, u32) {
        let spec = profile_spec_for(profile, &self.custom_profile);
        (
            spec.focus_secs,
            spec.short_break_secs,
            spec.long_break_secs,
            spec.long_break_interval,
        )
    }

    pub(crate) fn profile_summary(&self, profile: ProfileId) -> String {
        let (focus, short_break, long_break, cadence) = self.profile_values(profile);
        format!(
            "{}/{}/{} · every {} focus",
            format_duration_label(focus),
            format_duration_label(short_break),
            format_duration_label(long_break),
            cadence
        )
    }

    pub(crate) fn session_stats(&self) -> SessionStats {
        self.stats.session()
    }

    pub(crate) fn today_stats(&self) -> DailyStats {
        self.stats.daily_for(&current_day_key())
    }

    pub(crate) fn today_goal_progress(&self) -> DailyGoalProgress {
        let today = Local::now().date_naive();
        let today_key = today.format("%Y-%m-%d").to_string();
        let today_stats = self.stats.daily_for(&today_key);
        let target = self.effective_daily_goal_snapshot_for_day(today);
        goal_progress_for_totals(
            today_stats.focused_minutes(),
            today_stats.pomodoros_completed,
            target.minutes,
            target.pomodoros,
        )
    }

    pub(crate) fn goal_streak(&self) -> GoalStreak {
        self.goal_streak_for_day_key(&current_day_key())
    }

    fn goal_streak_for_day_key(&self, day_key: &str) -> GoalStreak {
        let Some(day) = parse_day_key(day_key) else {
            return GoalStreak::default();
        };

        let current_goal = self.effective_daily_goal_snapshot_for_day(day);
        self.stats.goal_streak_with_day_goal(
            day,
            current_goal,
            self.stats.daily_for(day_key),
            |day| self.effective_daily_goal_snapshot_for_day(day),
        )
    }

    #[allow(dead_code)]
    pub(crate) fn daily_goal_progress_for(&self, stats: DailyStats) -> DailyGoalProgress {
        goal_progress_for_totals(
            stats.focused_minutes(),
            stats.pomodoros_completed,
            self.daily_goal.minutes,
            self.daily_goal.pomodoros,
        )
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn recent_daily_stats(&self, limit: usize) -> Vec<(String, DailyStats)> {
        self.stats.recent_daily(limit)
    }

    #[allow(dead_code)]
    pub(crate) fn recent_weekly_stats(&self, limit: usize) -> Vec<WeeklyStats> {
        self.stats.recent_weekly(limit)
    }

    #[allow(dead_code)]
    pub(crate) fn recent_weekly_consistency(&self, limit: usize) -> Vec<WeeklyConsistency> {
        self.stats.recent_weekly_consistency(limit)
    }

    #[allow(dead_code)]
    pub(crate) fn latest_weekly_focus_score(&self) -> Option<WeeklyFocusScore> {
        self.stats.latest_weekly_focus_score()
    }

    #[allow(dead_code)]
    pub(crate) fn focus_risk_forecast(&self) -> FocusRiskForecast {
        let today = Local::now().date_naive();
        self.stats.focus_risk_forecast_for_day(
            today,
            self.effective_daily_goal_snapshot_for_day(today),
            self.effective_weekly_goal_snapshot_for_day(today),
            self.effective_monthly_goal_snapshot_for_day(today),
        )
    }

    #[allow(dead_code)]
    pub(crate) fn recent_monthly_stats(&self, limit: usize) -> Vec<MonthlyStats> {
        self.stats.recent_monthly(limit)
    }

    #[allow(dead_code)]
    pub(crate) fn latest_monthly_heatmap(&self) -> MonthlyHeatmap {
        self.stats.latest_monthly_heatmap()
    }

    #[allow(dead_code)]
    pub(crate) fn stats_growth_summary(&self) -> StatsGrowthSummary {
        self.stats.growth_summary()
    }

    #[allow(dead_code)]
    pub(crate) fn stats_retention_config(&self) -> StatsRetentionConfig {
        self.stats_retention
    }

    #[allow(dead_code)]
    pub(crate) fn stats_retention_preview(&self) -> StatsRetentionPruneResult {
        self.stats
            .retention_preview(self.stats_retention, Local::now().date_naive())
    }

    #[allow(dead_code)]
    pub(crate) fn profile_focus_totals(&self) -> Vec<ProfileTotals> {
        self.stats.profile_totals()
    }

    #[allow(dead_code)]
    pub(crate) fn profile_effectiveness(&self) -> Vec<ProfileEffectiveness> {
        self.stats.profile_effectiveness()
    }

    #[allow(dead_code)]
    pub(crate) fn task_focus_totals(&self, limit: usize) -> Vec<TaskTotals> {
        self.stats.task_totals(limit)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn recent_task_trends(&self, limit: usize) -> Vec<TaskTrend> {
        self.stats.recent_task_trends(limit)
    }

    #[cfg(test)]
    pub(crate) fn insert_daily_stats_for_tests(&mut self, day_key: &str, stats: DailyStats) {
        self.stats.insert_daily_for_tests(day_key, stats);
    }

    pub(crate) fn profile_edit_field_value(&self, field_index: usize) -> String {
        if (PROFILE_EDIT_SCHEDULE_WINDOW_INDEX..=PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX)
            .contains(&field_index)
        {
            return self.profile_edit_schedule_field_value(field_index);
        }

        match field_index {
            0 => format_duration_label(self.custom_profile.focus_secs),
            1 => format_duration_label(self.custom_profile.short_break_secs),
            2 => format_duration_label(self.custom_profile.long_break_secs),
            3 => format!(
                "every {} focus sessions",
                self.custom_profile.long_break_interval
            ),
            4 => bool_label(self.notification_settings.enabled).to_string(),
            5 => bool_label(self.notification_settings.sound).to_string(),
            6 => bool_label(self.auto_start.focus_to_break).to_string(),
            7 => bool_label(self.auto_start.break_to_focus).to_string(),
            8 => bool_label(self.strict_mode).to_string(),
            PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX => {
                format_daily_goal_minutes_label(self.daily_goal.minutes)
            }
            PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX => {
                format_daily_goal_pomodoros_label(self.daily_goal.pomodoros)
            }
            PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX => {
                bool_label(self.goal_carry_over.daily).to_string()
            }
            PROFILE_EDIT_THEME_PRESET_INDEX => self.selected_theme_preset.label().to_string(),
            _ => String::new(),
        }
    }

    fn profile_edit_metadata_field_mut(&mut self) -> Option<&mut String> {
        None
    }

    pub(super) fn handle_profile_edit_metadata_input(&mut self, key: &KeyEvent) -> bool {
        if self.navigation_matches(NavigationAction::Backspace, key) {
            let Some(field_value) = self.profile_edit_metadata_field_mut() else {
                return false;
            };
            field_value.pop();
            return true;
        }

        let Some(field_value) = self.profile_edit_metadata_field_mut() else {
            return false;
        };

        match key.code {
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                field_value.push(c);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn strict_mode_enforced_for_focus(&self) -> bool {
        self.strict_mode && self.focus_session_active_for_current_state()
    }

    pub(crate) fn strict_reset_confirmation_pending(&self) -> bool {
        self.pending_timer_action == Some(PendingTimerAction::Reset)
    }

    pub(crate) fn site_input_mode(&self) -> SiteInputMode {
        if self.site_edit_index.is_some() {
            SiteInputMode::Edit
        } else {
            SiteInputMode::Add
        }
    }

    pub(crate) fn site_list_mode(&self) -> SiteListMode {
        self.site_list_mode
    }

    pub(crate) fn active_policy_sites(&self) -> &[String] {
        self.active_profile_sites_for_mode(self.site_list_mode)
    }

    pub(crate) fn active_policy_site_count(&self) -> usize {
        self.active_policy_sites().len()
    }

    pub(crate) fn effective_blocked_site_count(&self) -> usize {
        self.blocker.sites.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn active_blocklist_profile_name(&self) -> &str {
        self.blocklist_profiles
            .get(self.active_blocklist_profile)
            .map(|profile| profile.name.as_str())
            .unwrap_or(DEFAULT_BLOCKLIST_PROFILE_NAME)
    }

    #[allow(dead_code)]
    pub(crate) fn active_blocklist_profile_position(&self) -> usize {
        self.active_blocklist_profile.saturating_add(1)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn blocklist_profile_count(&self) -> usize {
        self.blocklist_profiles.len()
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::Timer => self.handle_key_timer(key),
            AppMode::SiteManager => self.handle_key_site_manager(key),
            AppMode::ProfileManager => self.handle_key_profile_manager(key),
            AppMode::TaskSetup => self.handle_key_session_planner(key),
            AppMode::StatsHistory => self.handle_key_stats_history(key),
            AppMode::SetupDiagnostics => self.handle_key_setup_diagnostics(key),
        }
    }

    pub(crate) fn handle_paste(&mut self, text: String) {
        if self.mode != AppMode::SiteManager {
            return;
        }

        if !self.site_input_active {
            self.start_site_input(SiteInputMode::Add);
        }
        self.site_input.push_str(&text);
    }

    pub(crate) fn is_running(&self) -> bool {
        self.timer.status == TimerStatus::Running
    }

    /// Apply or remove blocks based on the current timer phase and status.
    ///
    /// Blocks whenever the focus phase is active (Running or Paused) so that
    /// pausing the timer cannot be used to bypass the block.
    /// Unblocks when the phase is a break or the timer has not yet started (Idle).
    fn apply_blocking_for_phase(&mut self) {
        let block_result = if self.should_block_for_current_state() {
            self.blocker.block()
        } else {
            self.blocker.unblock()
        };
        self.set_block_error_from_result(block_result);
    }

    fn handle_quit_key(&mut self, key: &KeyEvent, esc_quits: bool) -> bool {
        let is_quit_char = self.shortcut_matches(ShortcutAction::Quit, key);
        let is_esc_quit = key.code == KeyCode::Esc && esc_quits;
        let is_ctrl_c =
            matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL);
        let is_quit_key = is_quit_char || is_esc_quit || is_ctrl_c;
        if is_quit_key && self.strict_mode_enforced_for_focus() {
            self.phase_notification =
                Some("Strict mode active. Finish or stop focus before quitting.".to_string());
            return true;
        }

        if is_quit_char || is_esc_quit || is_ctrl_c {
            self.should_quit = true;
            true
        } else {
            false
        }
    }

    fn open_setup_diagnostics(&mut self) {
        self.pending_timer_action = None;
        self.refresh_setup_diagnostics();
        self.set_mode(AppMode::SetupDiagnostics);
    }

    pub(crate) fn record_current_screen_usage(&mut self) {
        self.record_screen_usage_for_mode(self.mode);
    }

    pub(super) fn set_mode(&mut self, mode: AppMode) {
        self.mode = mode;
        self.record_screen_usage_for_mode(mode);
    }

    fn record_screen_usage_for_mode(&mut self, mode: AppMode) {
        if self
            .stats
            .record_screen_usage(mode.screen_usage_surface_id())
        {
            self.mark_stats_dirty();
        }
    }

    fn should_block_for_current_state(&self) -> bool {
        self.focus_session_active_for_current_state()
    }

    fn timer_activity(&self) -> TimerActivity {
        TimerActivity::from_timer(&self.timer)
    }

    fn focus_running_for_current_state(&self) -> bool {
        self.timer_activity().focus_running
    }

    fn focus_session_active_for_current_state(&self) -> bool {
        self.timer_activity().focus_active
    }

    fn should_auto_start_transition(
        &self,
        completed_phase: TimerPhase,
        next_phase: TimerPhase,
    ) -> bool {
        match (completed_phase, next_phase) {
            (TimerPhase::Focus, TimerPhase::ShortBreak | TimerPhase::LongBreak) => {
                self.auto_start.focus_to_break
            }
            (TimerPhase::ShortBreak | TimerPhase::LongBreak, TimerPhase::Focus) => {
                self.auto_start.break_to_focus
            }
            _ => false,
        }
    }
}

impl AppMode {
    fn screen_usage_surface_id(self) -> &'static str {
        match self {
            Self::Timer => "timer",
            Self::SiteManager => "site-manager",
            Self::ProfileManager => "profile-manager",
            Self::TaskSetup => "task-setup",
            Self::StatsHistory => "stats-history",
            Self::SetupDiagnostics => "setup-diagnostics",
        }
    }
}

fn adjust_duration_minutes(value: &mut u64, increase: bool) {
    if increase {
        *value = value.saturating_add(CUSTOM_DURATION_STEP_SECS);
    } else {
        *value = value
            .saturating_sub(CUSTOM_DURATION_STEP_SECS)
            .max(CUSTOM_DURATION_STEP_SECS);
    }
}

fn adjust_daily_goal_minutes(value: &mut u64, increase: bool) {
    if increase {
        *value = if *value == 0 {
            DAILY_GOAL_MINUTES_STEP
        } else {
            value.saturating_add(DAILY_GOAL_MINUTES_STEP)
        };
    } else if *value <= DAILY_GOAL_MINUTES_STEP {
        *value = 0;
    } else {
        *value = value.saturating_sub(DAILY_GOAL_MINUTES_STEP);
    }
}

fn adjust_daily_goal_pomodoros(value: &mut u32, increase: bool) {
    if increase {
        *value = value.saturating_add(1);
    } else {
        *value = value.saturating_sub(1);
    }
}

fn format_duration_label(seconds: u64) -> String {
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    if remaining_seconds == 0 {
        format!("{minutes}m")
    } else {
        format!("{minutes}:{remaining_seconds:02}")
    }
}

fn format_daily_goal_minutes_label(minutes: u64) -> String {
    if minutes == 0 {
        "Off".to_string()
    } else {
        format!("{minutes}m")
    }
}

fn format_daily_goal_pomodoros_label(pomodoros: u32) -> String {
    if pomodoros == 0 {
        "Off".to_string()
    } else {
        format!("{pomodoros}")
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn parse_hhmm_minutes(value: &str) -> Option<u16> {
    let (hours, minutes) = value.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    let hour = hours.parse::<u16>().ok()?;
    let minute = minutes.parse::<u16>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hour * 60 + minute)
}

fn format_hhmm(total_minutes: u16) -> String {
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{hours:02}:{minutes:02}")
}

fn sort_schedule_days(days: &mut Vec<String>) {
    let mut sorted = Vec::new();
    for token in SCHEDULE_DAY_TOKENS {
        if days.iter().any(|day| day.eq_ignore_ascii_case(token)) {
            sorted.push(token.to_string());
        }
    }
    if sorted.is_empty() {
        sorted.push(SCHEDULE_DAY_TOKENS[0].to_string());
    }
    *days = sorted;
}

fn format_schedule_days_for_display(days: &[String]) -> String {
    let mut labels = Vec::new();
    for (index, token) in SCHEDULE_DAY_TOKENS.iter().enumerate() {
        if days.iter().any(|day| day.eq_ignore_ascii_case(token)) {
            labels.push(SCHEDULE_DAY_LABELS[index]);
        }
    }
    if labels.is_empty() {
        "none".to_string()
    } else {
        labels.join(",")
    }
}

fn parse_day_key(day_key: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d").ok()
}

fn previous_month_reference_day(day: NaiveDate) -> Option<NaiveDate> {
    let month_start = NaiveDate::from_ymd_opt(day.year(), day.month(), 1)?;
    month_start.pred_opt()
}

fn goal_progress(completed: u64, target: u64) -> GoalProgress {
    let ratio = if target == 0 {
        0.0
    } else {
        (completed as f64 / target as f64).clamp(0.0, 1.0)
    };
    GoalProgress {
        completed,
        target,
        ratio,
    }
}

fn goal_progress_for_totals(
    focused_minutes: u64,
    pomodoros_completed: u32,
    target_minutes: u64,
    target_pomodoros: u32,
) -> DailyGoalProgress {
    DailyGoalProgress {
        minutes: goal_progress(focused_minutes, target_minutes),
        pomodoros: goal_progress(u64::from(pomodoros_completed), u64::from(target_pomodoros)),
    }
}

fn format_count(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

fn summarize_invalid_inputs(invalid: &[InvalidSiteInput]) -> String {
    const PREVIEW_LIMIT: usize = 3;
    let preview_count = invalid.len().min(PREVIEW_LIMIT);
    let mut details = invalid
        .iter()
        .take(preview_count)
        .map(|entry| {
            format!(
                "`{}`: {}",
                display_input_value(&entry.input),
                entry.reason.message()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    if invalid.len() > PREVIEW_LIMIT {
        details.push_str(&format!(", +{} more", invalid.len() - PREVIEW_LIMIT));
    }
    details
}

fn display_input_value(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        "<empty>".to_string()
    } else {
        trimmed.to_string()
    }
}

fn effective_blocked_sites_for_profile(profile: &BlocklistProfileConfig) -> Vec<String> {
    crate::config::effective_blocked_sites_for_profile(profile)
}

pub(super) fn setup_deprecation_warnings(
    config_deprecation_warnings: &[String],
    _stats: &FocusStats,
) -> Vec<String> {
    let mut warnings = config_deprecation_warnings.to_vec();
    if let Some(stats_warning) = legacy_stats_path_migration_warning() {
        warnings.push(stats_warning);
    }
    warnings
}

pub(super) fn format_legacy_stats_path_migration_warning(
    canonical_path: &std::path::Path,
    legacy_path: &std::path::Path,
) -> String {
    format!(
        "Legacy stats path `{}` is still in use while canonical stats `{}` are missing. Copy `{}` to `{}` (you can use `focustime --backup` and `focustime --restore` to assist).",
        legacy_path.display(),
        canonical_path.display(),
        legacy_path.display(),
        canonical_path.display()
    )
}

#[cfg(not(test))]
fn legacy_stats_path_migration_warning() -> Option<String> {
    let canonical_path = crate::config::stats_data_path(STATS_FILE_NAME)?;
    if canonical_path.exists() {
        return None;
    }
    let legacy_path =
        crate::config::app_data_path(STATS_FILE_NAME).filter(|path| path != &canonical_path)?;
    if !legacy_path.exists() {
        return None;
    }
    Some(format_legacy_stats_path_migration_warning(
        &canonical_path,
        &legacy_path,
    ))
}

#[cfg(test)]
fn legacy_stats_path_migration_warning() -> Option<String> {
    None
}

impl Drop for App {
    fn drop(&mut self) {
        self.flush_stats_if_dirty(true);
        // Ensure hosts-file block entries are removed on every exit path,
        // including early returns caused by I/O errors in run_app.
        self.blocker.cleanup();
    }
}

fn task_label_key(label: &str) -> String {
    label.to_ascii_lowercase()
}

fn task_label_state_keys(labels: Vec<String>) -> BTreeSet<String> {
    labels
        .into_iter()
        .filter_map(|label| normalize_task_label(&label))
        .map(|label| task_label_key(&label))
        .collect()
}

fn task_label_state_labels(keys: &BTreeSet<String>, labels: &[String]) -> Vec<String> {
    if keys.is_empty() {
        return Vec::new();
    }

    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    for label in labels {
        let key = task_label_key(label);
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

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn should_handle_key(key: &KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
}

#[cfg(test)]
mod tests;
