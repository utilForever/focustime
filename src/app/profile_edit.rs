use crate::config::{
    AutoStartConfig, AutomationTriggerRuleConfig, CustomProfileConfig, DailyGoalConfig,
    GoalCarryOverConfig, MonthlyGoalConfig, NotificationConfig, RecurringScheduleConfig,
    ThemePreset, WakatimeMetadataConfig, WeekdayProfileRuleConfig, WeeklyGoalConfig,
};

pub(crate) const PROFILE_EDIT_FIELD_LABELS: [&str; 52] = [
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
    "Schedule exception",
    "Exception date",
    "Exception add/remove",
    "One-time window",
    "One-time date",
    "One-time start",
    "One-time end",
    "One-time add/remove",
    "Schedule conflicts",
    "Weekday rule",
    "Weekday rule day",
    "Weekday rule profile",
    "Weekday rule blocklist",
    "Weekday rule template",
    "Weekday rule add/remove",
    "Theme preset",
    "Automation trigger",
    "Trigger condition",
    "Trigger time day",
    "Trigger time at",
    "Trigger action",
    "Action profile",
    "Action blocklist",
    "Action template",
    "Action delay",
    "Trigger add/remove",
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
pub(crate) const PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX: usize = 26;
pub(crate) const PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX: usize = 27;
pub(crate) const PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX: usize = 28;
pub(crate) const PROFILE_EDIT_ONE_TIME_WINDOW_INDEX: usize = 29;
pub(crate) const PROFILE_EDIT_ONE_TIME_DATE_INDEX: usize = 30;
pub(crate) const PROFILE_EDIT_ONE_TIME_START_INDEX: usize = 31;
pub(crate) const PROFILE_EDIT_ONE_TIME_END_INDEX: usize = 32;
pub(crate) const PROFILE_EDIT_ONE_TIME_ADD_REMOVE_INDEX: usize = 33;
pub(crate) const PROFILE_EDIT_SCHEDULE_CONFLICTS_INDEX: usize = 34;
pub(crate) const PROFILE_EDIT_WEEKDAY_RULE_INDEX: usize = 35;
pub(crate) const PROFILE_EDIT_WEEKDAY_RULE_DAY_INDEX: usize = 36;
pub(crate) const PROFILE_EDIT_WEEKDAY_RULE_PROFILE_INDEX: usize = 37;
pub(crate) const PROFILE_EDIT_WEEKDAY_RULE_BLOCKLIST_INDEX: usize = 38;
pub(crate) const PROFILE_EDIT_WEEKDAY_RULE_TEMPLATE_INDEX: usize = 39;
pub(crate) const PROFILE_EDIT_WEEKDAY_RULE_ADD_REMOVE_INDEX: usize = 40;
pub(crate) const PROFILE_EDIT_THEME_PRESET_INDEX: usize = 41;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_INDEX: usize = 42;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_CONDITION_INDEX: usize = 43;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_DAY_INDEX: usize = 44;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_AT_INDEX: usize = 45;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_ACTION_INDEX: usize = 46;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_PROFILE_INDEX: usize = 47;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_BLOCKLIST_INDEX: usize = 48;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_TEMPLATE_INDEX: usize = 49;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_DELAY_INDEX: usize = 50;
pub(crate) const PROFILE_EDIT_AUTOMATION_TRIGGER_ADD_REMOVE_INDEX: usize = 51;
pub(crate) const CUSTOM_DURATION_STEP_SECS: u64 = 60;
pub(crate) const DAILY_GOAL_MINUTES_STEP: u64 = 5;

#[derive(Debug, Clone)]
pub(crate) struct ProfileEditSnapshot {
    pub(crate) custom_profile: CustomProfileConfig,
    pub(crate) notification_settings: NotificationConfig,
    pub(crate) auto_start: AutoStartConfig,
    pub(crate) recurring_schedule: RecurringScheduleConfig,
    pub(crate) automation_triggers: Vec<AutomationTriggerRuleConfig>,
    pub(crate) weekday_profile_rules: Vec<WeekdayProfileRuleConfig>,
    pub(crate) strict_mode: bool,
    pub(crate) daily_goal: DailyGoalConfig,
    pub(crate) weekly_goal: WeeklyGoalConfig,
    pub(crate) monthly_goal: MonthlyGoalConfig,
    pub(crate) goal_carry_over: GoalCarryOverConfig,
    pub(crate) selected_theme_preset: ThemePreset,
    pub(crate) wakatime_metadata: WakatimeMetadataConfig,
}
