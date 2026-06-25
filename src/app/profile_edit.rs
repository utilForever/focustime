use crate::config::{
    AutoStartConfig, CustomProfileConfig, DailyGoalConfig, GoalCarryOverConfig, MonthlyGoalConfig,
    NotificationConfig, RecurringScheduleConfig, ThemePreset, WakatimeMetadataConfig,
    WeeklyGoalConfig,
};

pub(crate) const PROFILE_EDIT_FIELD_LABELS: [&str; 28] = [
    "Focus",
    "Short Break",
    "Long Break",
    "Long-break cadence",
    "Phase notifications",
    "Sound alert",
    "Auto-start break",
    "Auto-start focus",
    "Strict focus mode",
    "Daily goal minutes",
    "Daily goal pomodoros",
    "Daily goal carry-over",
    "Weekly goal minutes",
    "Weekly goal pomodoros",
    "Weekly goal carry-over",
    "Monthly goal minutes",
    "Monthly goal pomodoros",
    "Monthly goal carry-over",
    "WakaTime project",
    "WakaTime language",
    "Schedule window",
    "Schedule day",
    "Schedule day enabled",
    "Schedule start",
    "Schedule end",
    "Schedule add/remove",
    "Schedule conflicts",
    "Theme preset",
];
pub(crate) const PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX: usize = 9;
pub(crate) const PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX: usize = 10;
pub(crate) const PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX: usize = 11;
pub(crate) const PROFILE_EDIT_WEEKLY_GOAL_MINUTES_INDEX: usize = 12;
pub(crate) const PROFILE_EDIT_WEEKLY_GOAL_POMODOROS_INDEX: usize = 13;
pub(crate) const PROFILE_EDIT_WEEKLY_GOAL_CARRY_OVER_INDEX: usize = 14;
pub(crate) const PROFILE_EDIT_MONTHLY_GOAL_MINUTES_INDEX: usize = 15;
pub(crate) const PROFILE_EDIT_MONTHLY_GOAL_POMODOROS_INDEX: usize = 16;
pub(crate) const PROFILE_EDIT_MONTHLY_GOAL_CARRY_OVER_INDEX: usize = 17;
pub(crate) const PROFILE_EDIT_WAKATIME_PROJECT_INDEX: usize = 18;
pub(crate) const PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX: usize = 19;
pub(crate) const PROFILE_EDIT_SCHEDULE_WINDOW_INDEX: usize = 20;
pub(crate) const PROFILE_EDIT_SCHEDULE_DAY_INDEX: usize = 21;
pub(crate) const PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX: usize = 22;
pub(crate) const PROFILE_EDIT_SCHEDULE_START_INDEX: usize = 23;
pub(crate) const PROFILE_EDIT_SCHEDULE_END_INDEX: usize = 24;
pub(crate) const PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX: usize = 25;
pub(crate) const PROFILE_EDIT_SCHEDULE_CONFLICTS_INDEX: usize = 26;
pub(crate) const PROFILE_EDIT_THEME_PRESET_INDEX: usize = 27;
pub(crate) const CUSTOM_DURATION_STEP_SECS: u64 = 60;
pub(crate) const DAILY_GOAL_MINUTES_STEP: u64 = 5;

#[derive(Debug, Clone)]
pub(crate) struct ProfileEditSnapshot {
    pub(crate) custom_profile: CustomProfileConfig,
    pub(crate) notification_settings: NotificationConfig,
    pub(crate) auto_start: AutoStartConfig,
    pub(crate) recurring_schedule: RecurringScheduleConfig,
    pub(crate) strict_mode: bool,
    pub(crate) daily_goal: DailyGoalConfig,
    pub(crate) weekly_goal: WeeklyGoalConfig,
    pub(crate) monthly_goal: MonthlyGoalConfig,
    pub(crate) goal_carry_over: GoalCarryOverConfig,
    pub(crate) selected_theme_preset: ThemePreset,
    pub(crate) wakatime_metadata: WakatimeMetadataConfig,
}
