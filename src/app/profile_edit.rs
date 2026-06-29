use crate::config::{
    AutoStartConfig, CustomProfileConfig, DailyGoalConfig, NotificationConfig,
    RecurringScheduleConfig, ThemePreset,
};

pub(crate) const PROFILE_EDIT_FIELD_LABELS: [&str; 18] = [
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
    "Schedule window",
    "Schedule day",
    "Schedule day enabled",
    "Schedule start",
    "Schedule end",
    "Schedule add/remove",
    "Theme preset",
];
pub(crate) const PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX: usize = 9;
pub(crate) const PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX: usize = 10;
pub(crate) const PROFILE_EDIT_SCHEDULE_WINDOW_INDEX: usize = 11;
pub(crate) const PROFILE_EDIT_SCHEDULE_DAY_INDEX: usize = 12;
pub(crate) const PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX: usize = 13;
pub(crate) const PROFILE_EDIT_SCHEDULE_START_INDEX: usize = 14;
pub(crate) const PROFILE_EDIT_SCHEDULE_END_INDEX: usize = 15;
pub(crate) const PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX: usize = 16;
pub(crate) const PROFILE_EDIT_THEME_PRESET_INDEX: usize = 17;
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
    pub(crate) selected_theme_preset: ThemePreset,
}
