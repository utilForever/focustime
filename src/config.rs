use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Deserializer, Serialize};

mod paths;
#[cfg(all(test, not(target_os = "windows")))]
use paths::env_path_from_value;
use paths::{app_dir_with_env, stats_app_dir_with_env};
#[cfg(test)]
use paths::{config_dir_from_env, stats_state_dir_from_env};

const CURRENT_CONFIG_SCHEMA_VERSION: u32 = 1;
const LEGACY_CONFIG_SCHEMA_VERSION: u32 = 0;
const SCHEDULE_TIME_STEP_MIN_MINUTES: u16 = 1;
const SCHEDULE_TIME_STEP_MAX_MINUTES: u16 = 60;
const SCHEDULE_DELAY_MIN_SECS: u64 = 60;
const SCHEDULE_DELAY_MAX_SECS: u64 = 12 * 60 * 60;
const WAKATIME_QUEUE_CAPACITY_MIN: usize = 1;
const WAKATIME_QUEUE_CAPACITY_MAX: usize = 4096;
const WAKATIME_RETRY_DELAY_MIN_SECS: u64 = 1;
const WAKATIME_RETRY_DELAY_MAX_SECS: u64 = 60 * 60;
const WAKATIME_RETRY_BACKOFF_MIN_SECS: u64 = 1;
const WAKATIME_RETRY_BACKOFF_MAX_SECS: u64 = 300;
const WAKATIME_RETRY_BACKOFF_MAX_ENTRIES: usize = 8;

/// Persistent application configuration stored as TOML.
///
/// File locations:
/// - Unix / macOS: `$XDG_CONFIG_HOME/focustime/config.toml` if set,
///   otherwise `~/.config/focustime/config.toml`
/// - Windows:      `%APPDATA%\focustime\config.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Duration of a focus session in seconds (legacy load-time compatibility field).
    #[serde(default = "default_focus_secs", skip_serializing)]
    pub focus_secs: u64,
    /// Duration of a short-break session in seconds (legacy load-time compatibility field).
    #[serde(default = "default_short_break_secs", skip_serializing)]
    pub short_break_secs: u64,
    /// Duration of a long-break session in seconds (legacy load-time compatibility field).
    #[serde(default = "default_long_break_secs", skip_serializing)]
    pub long_break_secs: u64,
    /// Number of completed focus sessions before a long break.
    #[serde(default = "default_long_break_interval", skip_serializing)]
    pub long_break_interval: u32,
    /// Deprecated blocked-sites mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub blocked_sites: Vec<String>,
    /// Named blocklist profiles.
    ///
    /// Each profile stores a separate blocked-sites list. This field supports
    /// issue #110 and supersedes `blocked_sites` as the primary representation.
    #[serde(default)]
    pub blocklist_profiles: Vec<BlocklistProfileConfig>,
    /// Name of the active blocklist profile.
    #[serde(default = "default_blocklist_profile_name")]
    pub selected_blocklist_profile: String,
    /// Blocking backend selection and fallback behavior.
    #[serde(default)]
    pub blocking_backend: BlockingBackendConfig,
    /// Selected profile identifier.
    #[serde(default)]
    pub selected_profile: ProfileId,
    /// Editable custom profile persisted by the app.
    ///
    /// When this is absent, the app derives it from the legacy duration fields.
    #[serde(default)]
    pub custom_profile: Option<CustomProfileConfig>,
    /// Globally reusable break templates for quick selection.
    #[serde(default = "default_break_templates")]
    pub break_templates: Vec<BreakTemplateConfig>,
    /// Name of the active break template.
    #[serde(default)]
    pub selected_break_template: String,
    /// Weekday smart-switch rules for profile and planning defaults.
    #[serde(default)]
    pub weekday_profile_rules: Vec<WeekdayProfileRuleConfig>,
    /// Reusable session templates bundling task/profile/blocklist/schedule settings.
    #[serde(default)]
    pub session_templates: Vec<SessionTemplateConfig>,
    /// Name of the active session template (empty = none selected).
    #[serde(default)]
    pub selected_session_template: String,
    /// Rule-based automation triggers for time/schedule/runtime events.
    #[serde(default)]
    pub automation_triggers: Vec<AutomationTriggerRuleConfig>,
    /// Selected UI theme preset.
    #[serde(default)]
    pub selected_theme_preset: ThemePreset,
    /// Deprecated top-level automation mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub notifications: NotificationConfig,
    /// Deprecated top-level automation mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub auto_start: AutoStartConfig,
    /// Deprecated top-level automation mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub recurring_schedule: RecurringScheduleConfig,
    /// Runtime tuning knobs for schedule editing and delay behavior.
    #[serde(default)]
    pub schedule_runtime: ScheduleRuntimeConfig,
    /// Profile-scoped automation settings.
    ///
    /// When absent, legacy global automation fields are used as shared defaults
    /// for all profiles during normalization.
    #[serde(default)]
    pub profile_automation: Option<ProfileAutomationSettingsConfig>,
    /// Deprecated top-level automation mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub strict_mode: bool,
    /// Duration of a break-glass unblock override in seconds.
    ///
    /// This value is clamped to a non-zero default during normalization.
    #[serde(default = "default_break_glass_duration_secs")]
    pub break_glass_duration_secs: u64,
    /// Daily goal settings for focus minutes and completed pomodoros.
    ///
    /// A value of `0` disables the corresponding goal.
    #[serde(default)]
    pub daily_goal: DailyGoalConfig,
    /// Weekly goal settings for focus minutes and completed pomodoros.
    ///
    /// A value of `0` disables the corresponding goal.
    #[serde(default)]
    pub weekly_goal: WeeklyGoalConfig,
    /// Monthly goal settings for focus minutes and completed pomodoros.
    ///
    /// A value of `0` disables the corresponding goal.
    #[serde(default)]
    pub monthly_goal: MonthlyGoalConfig,
    /// Carry-over behavior for unmet daily/weekly/monthly targets.
    #[serde(default)]
    pub goal_carry_over: GoalCarryOverConfig,
    /// Retention policy for persisted stats history.
    #[serde(default)]
    pub stats_retention: StatsRetentionConfig,
    /// WakaTime heartbeat metadata labels.
    #[serde(default)]
    pub wakatime: WakatimeMetadataConfig,
    /// Runtime tuning knobs for WakaTime retry/queue behavior.
    #[serde(default)]
    pub wakatime_runtime: WakatimeRuntimeConfig,
    /// Feature flags used to safely gate compatibility-sensitive behavior.
    #[serde(default)]
    pub feature_flags: FeatureFlagsConfig,
    /// User-configurable keyboard shortcuts for core TUI command actions.
    #[serde(default)]
    pub shortcuts: ShortcutConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppConfigDisk {
    #[serde(default = "default_legacy_config_schema_version")]
    schema_version: u32,
    #[serde(flatten)]
    config: AppConfig,
}

impl AppConfigDisk {
    fn from_config(config: AppConfig) -> Self {
        Self {
            schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
            config,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FeatureFlagsConfig {}

impl FeatureFlagsConfig {
    pub fn normalized(&self) -> Self {
        *self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BlockingBackendConfig {
    #[serde(default)]
    pub policy: BlockingBackendPolicyConfig,
    #[serde(default)]
    pub command: CommandBlockingBackendConfig,
}

impl BlockingBackendConfig {
    pub fn normalized(&self) -> Self {
        Self {
            policy: self.policy,
            command: self.command.normalized(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockingBackendPolicyConfig {
    HostsOnly,
    #[default]
    HostsThenCommand,
    CommandThenHosts,
    CommandOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CommandBlockingBackendConfig {
    #[serde(default)]
    pub block_command: String,
    #[serde(default)]
    pub unblock_command: String,
    #[serde(default)]
    pub diagnostics_command: String,
}

impl CommandBlockingBackendConfig {
    pub fn normalized(&self) -> Self {
        Self {
            block_command: self.block_command.trim().to_string(),
            unblock_command: self.unblock_command.trim().to_string(),
            diagnostics_command: self.diagnostics_command.trim().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShortcutConfig {
    #[serde(default = "default_shortcut_quit")]
    pub quit: String,
    #[serde(default = "default_shortcut_timer_toggle_pause")]
    pub timer_toggle_pause: String,
    #[serde(default = "default_shortcut_timer_stop_reset")]
    pub timer_stop_reset: String,
    #[serde(default = "default_shortcut_timer_next_phase")]
    pub timer_next_phase: String,
    #[serde(default = "default_shortcut_open_site_manager")]
    pub open_site_manager: String,
    #[serde(default = "default_shortcut_open_profile_manager")]
    pub open_profile_manager: String,
    #[serde(default = "default_shortcut_open_session_planner")]
    pub open_session_planner: String,
    #[serde(default = "default_shortcut_open_stats_history")]
    pub open_stats_history: String,
    #[serde(default = "default_shortcut_open_setup_diagnostics")]
    pub open_setup_diagnostics: String,
    #[serde(default = "default_shortcut_timer_edit_note")]
    pub timer_edit_note: String,
    #[serde(default = "default_shortcut_break_glass_override")]
    pub break_glass_override: String,
    #[serde(default = "default_shortcut_delay_schedule_start")]
    pub delay_schedule_start: String,
    #[serde(default = "default_shortcut_back_site_manager")]
    pub back_site_manager: String,
    #[serde(default = "default_shortcut_toggle_site_list_mode")]
    pub toggle_site_list_mode: String,
    #[serde(default = "default_shortcut_site_add")]
    pub site_add: String,
    #[serde(default = "default_shortcut_site_edit")]
    pub site_edit: String,
    #[serde(default = "default_shortcut_site_delete")]
    pub site_delete: String,
    #[serde(default = "default_shortcut_select_previous_blocklist_profile")]
    pub select_previous_blocklist_profile: String,
    #[serde(default = "default_shortcut_select_next_blocklist_profile")]
    pub select_next_blocklist_profile: String,
    #[serde(default = "default_shortcut_create_blocklist_profile")]
    pub create_blocklist_profile: String,
    #[serde(default = "default_shortcut_rename_blocklist_profile")]
    pub rename_blocklist_profile: String,
    #[serde(default = "default_shortcut_delete_blocklist_profile")]
    pub delete_blocklist_profile: String,
    #[serde(default = "default_shortcut_back_session_planner")]
    pub back_session_planner: String,
    #[serde(default = "default_shortcut_planner_add")]
    pub planner_add: String,
    #[serde(default = "default_shortcut_planner_rename")]
    pub planner_rename: String,
    #[serde(default = "default_shortcut_planner_favorite")]
    pub planner_favorite: String,
    #[serde(default = "default_shortcut_planner_archive")]
    pub planner_archive: String,
    #[serde(default = "default_shortcut_planner_delete")]
    pub planner_delete: String,
    #[serde(default = "default_shortcut_planner_select_recent")]
    pub planner_select_recent: String,
    #[serde(default = "default_shortcut_back_profile_manager")]
    pub back_profile_manager: String,
    #[serde(default = "default_shortcut_profile_edit")]
    pub profile_edit: String,
    #[serde(default = "default_shortcut_select_previous_break_template")]
    pub select_previous_break_template: String,
    #[serde(default = "default_shortcut_select_next_break_template")]
    pub select_next_break_template: String,
    #[serde(default = "default_shortcut_back_stats_history")]
    pub back_stats_history: String,
    #[serde(default = "default_shortcut_export_stats_history")]
    pub export_stats_history: String,
    #[serde(default = "default_shortcut_back_setup_diagnostics")]
    pub back_setup_diagnostics: String,
    #[serde(default = "default_shortcut_refresh_setup_diagnostics")]
    pub refresh_setup_diagnostics: String,
    #[serde(default = "default_shortcut_navigate_up")]
    pub navigate_up: String,
    #[serde(default = "default_shortcut_navigate_down")]
    pub navigate_down: String,
    #[serde(default = "default_shortcut_navigate_left")]
    pub navigate_left: String,
    #[serde(default = "default_shortcut_navigate_right")]
    pub navigate_right: String,
    #[serde(default = "default_shortcut_confirm")]
    pub confirm: String,
    #[serde(default = "default_shortcut_cancel")]
    pub cancel: String,
    #[serde(default = "default_shortcut_delete")]
    pub delete: String,
    #[serde(default = "default_shortcut_backspace")]
    pub backspace: String,
}

impl ShortcutConfig {
    pub fn normalized(&self) -> Self {
        Self {
            quit: normalize_shortcut_token(&self.quit, &default_shortcut_quit()),
            timer_toggle_pause: normalize_shortcut_token(
                &self.timer_toggle_pause,
                &default_shortcut_timer_toggle_pause(),
            ),
            timer_stop_reset: normalize_shortcut_token(
                &self.timer_stop_reset,
                &default_shortcut_timer_stop_reset(),
            ),
            timer_next_phase: normalize_shortcut_token(
                &self.timer_next_phase,
                &default_shortcut_timer_next_phase(),
            ),
            open_site_manager: normalize_shortcut_token(
                &self.open_site_manager,
                &default_shortcut_open_site_manager(),
            ),
            open_profile_manager: normalize_shortcut_token(
                &self.open_profile_manager,
                &default_shortcut_open_profile_manager(),
            ),
            open_session_planner: normalize_shortcut_token(
                &self.open_session_planner,
                &default_shortcut_open_session_planner(),
            ),
            open_stats_history: normalize_shortcut_token(
                &self.open_stats_history,
                &default_shortcut_open_stats_history(),
            ),
            open_setup_diagnostics: normalize_shortcut_token(
                &self.open_setup_diagnostics,
                &default_shortcut_open_setup_diagnostics(),
            ),
            timer_edit_note: normalize_shortcut_token(
                &self.timer_edit_note,
                &default_shortcut_timer_edit_note(),
            ),
            break_glass_override: normalize_shortcut_token(
                &self.break_glass_override,
                &default_shortcut_break_glass_override(),
            ),
            delay_schedule_start: normalize_shortcut_token(
                &self.delay_schedule_start,
                &default_shortcut_delay_schedule_start(),
            ),
            back_site_manager: normalize_shortcut_token(
                &self.back_site_manager,
                &default_shortcut_back_site_manager(),
            ),
            toggle_site_list_mode: normalize_shortcut_token(
                &self.toggle_site_list_mode,
                &default_shortcut_toggle_site_list_mode(),
            ),
            site_add: normalize_shortcut_token(&self.site_add, &default_shortcut_site_add()),
            site_edit: normalize_shortcut_token(&self.site_edit, &default_shortcut_site_edit()),
            site_delete: normalize_shortcut_token(
                &self.site_delete,
                &default_shortcut_site_delete(),
            ),
            select_previous_blocklist_profile: normalize_shortcut_token(
                &self.select_previous_blocklist_profile,
                &default_shortcut_select_previous_blocklist_profile(),
            ),
            select_next_blocklist_profile: normalize_shortcut_token(
                &self.select_next_blocklist_profile,
                &default_shortcut_select_next_blocklist_profile(),
            ),
            create_blocklist_profile: normalize_shortcut_token(
                &self.create_blocklist_profile,
                &default_shortcut_create_blocklist_profile(),
            ),
            rename_blocklist_profile: normalize_shortcut_token(
                &self.rename_blocklist_profile,
                &default_shortcut_rename_blocklist_profile(),
            ),
            delete_blocklist_profile: normalize_shortcut_token(
                &self.delete_blocklist_profile,
                &default_shortcut_delete_blocklist_profile(),
            ),
            back_session_planner: normalize_shortcut_token(
                &self.back_session_planner,
                &default_shortcut_back_session_planner(),
            ),
            planner_add: normalize_shortcut_token(
                &self.planner_add,
                &default_shortcut_planner_add(),
            ),
            planner_rename: normalize_shortcut_token(
                &self.planner_rename,
                &default_shortcut_planner_rename(),
            ),
            planner_favorite: normalize_shortcut_token(
                &self.planner_favorite,
                &default_shortcut_planner_favorite(),
            ),
            planner_archive: normalize_shortcut_token(
                &self.planner_archive,
                &default_shortcut_planner_archive(),
            ),
            planner_delete: normalize_shortcut_token(
                &self.planner_delete,
                &default_shortcut_planner_delete(),
            ),
            planner_select_recent: normalize_shortcut_token(
                &self.planner_select_recent,
                &default_shortcut_planner_select_recent(),
            ),
            back_profile_manager: normalize_shortcut_token(
                &self.back_profile_manager,
                &default_shortcut_back_profile_manager(),
            ),
            profile_edit: normalize_shortcut_token(
                &self.profile_edit,
                &default_shortcut_profile_edit(),
            ),
            select_previous_break_template: normalize_shortcut_token(
                &self.select_previous_break_template,
                &default_shortcut_select_previous_break_template(),
            ),
            select_next_break_template: normalize_shortcut_token(
                &self.select_next_break_template,
                &default_shortcut_select_next_break_template(),
            ),
            back_stats_history: normalize_shortcut_token(
                &self.back_stats_history,
                &default_shortcut_back_stats_history(),
            ),
            export_stats_history: normalize_shortcut_token(
                &self.export_stats_history,
                &default_shortcut_export_stats_history(),
            ),
            back_setup_diagnostics: normalize_shortcut_token(
                &self.back_setup_diagnostics,
                &default_shortcut_back_setup_diagnostics(),
            ),
            refresh_setup_diagnostics: normalize_shortcut_token(
                &self.refresh_setup_diagnostics,
                &default_shortcut_refresh_setup_diagnostics(),
            ),
            navigate_up: normalize_navigation_shortcut_token(
                &self.navigate_up,
                &default_shortcut_navigate_up(),
            ),
            navigate_down: normalize_navigation_shortcut_token(
                &self.navigate_down,
                &default_shortcut_navigate_down(),
            ),
            navigate_left: normalize_navigation_shortcut_token(
                &self.navigate_left,
                &default_shortcut_navigate_left(),
            ),
            navigate_right: normalize_navigation_shortcut_token(
                &self.navigate_right,
                &default_shortcut_navigate_right(),
            ),
            confirm: normalize_navigation_shortcut_token(
                &self.confirm,
                &default_shortcut_confirm(),
            ),
            cancel: normalize_navigation_shortcut_token(&self.cancel, &default_shortcut_cancel()),
            delete: normalize_navigation_shortcut_token(&self.delete, &default_shortcut_delete()),
            backspace: normalize_navigation_shortcut_token(
                &self.backspace,
                &default_shortcut_backspace(),
            ),
        }
    }
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            quit: default_shortcut_quit(),
            timer_toggle_pause: default_shortcut_timer_toggle_pause(),
            timer_stop_reset: default_shortcut_timer_stop_reset(),
            timer_next_phase: default_shortcut_timer_next_phase(),
            open_site_manager: default_shortcut_open_site_manager(),
            open_profile_manager: default_shortcut_open_profile_manager(),
            open_session_planner: default_shortcut_open_session_planner(),
            open_stats_history: default_shortcut_open_stats_history(),
            open_setup_diagnostics: default_shortcut_open_setup_diagnostics(),
            timer_edit_note: default_shortcut_timer_edit_note(),
            break_glass_override: default_shortcut_break_glass_override(),
            delay_schedule_start: default_shortcut_delay_schedule_start(),
            back_site_manager: default_shortcut_back_site_manager(),
            toggle_site_list_mode: default_shortcut_toggle_site_list_mode(),
            site_add: default_shortcut_site_add(),
            site_edit: default_shortcut_site_edit(),
            site_delete: default_shortcut_site_delete(),
            select_previous_blocklist_profile: default_shortcut_select_previous_blocklist_profile(),
            select_next_blocklist_profile: default_shortcut_select_next_blocklist_profile(),
            create_blocklist_profile: default_shortcut_create_blocklist_profile(),
            rename_blocklist_profile: default_shortcut_rename_blocklist_profile(),
            delete_blocklist_profile: default_shortcut_delete_blocklist_profile(),
            back_session_planner: default_shortcut_back_session_planner(),
            planner_add: default_shortcut_planner_add(),
            planner_rename: default_shortcut_planner_rename(),
            planner_favorite: default_shortcut_planner_favorite(),
            planner_archive: default_shortcut_planner_archive(),
            planner_delete: default_shortcut_planner_delete(),
            planner_select_recent: default_shortcut_planner_select_recent(),
            back_profile_manager: default_shortcut_back_profile_manager(),
            profile_edit: default_shortcut_profile_edit(),
            select_previous_break_template: default_shortcut_select_previous_break_template(),
            select_next_break_template: default_shortcut_select_next_break_template(),
            back_stats_history: default_shortcut_back_stats_history(),
            export_stats_history: default_shortcut_export_stats_history(),
            back_setup_diagnostics: default_shortcut_back_setup_diagnostics(),
            refresh_setup_diagnostics: default_shortcut_refresh_setup_diagnostics(),
            navigate_up: default_shortcut_navigate_up(),
            navigate_down: default_shortcut_navigate_down(),
            navigate_left: default_shortcut_navigate_left(),
            navigate_right: default_shortcut_navigate_right(),
            confirm: default_shortcut_confirm(),
            cancel: default_shortcut_cancel(),
            delete: default_shortcut_delete(),
            backspace: default_shortcut_backspace(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlocklistProfileConfig {
    #[serde(default = "default_blocklist_profile_name")]
    pub name: String,
    /// Deprecated flat mirror of categorized blocklist rules.
    ///
    /// Canonical rules live in `categories[*].sites`; this field is maintained
    /// for load-time compatibility and helper surfaces still reading flat lists.
    #[serde(default)]
    pub sites: Vec<String>,
    /// Deprecated flat mirror of categorized allowlist rules.
    ///
    /// Canonical rules live in `categories[*].allowlist_sites`.
    /// Sites that are explicitly excluded from blocking.
    ///
    /// Effective focus blocking is computed as `sites - allowlist_sites`.
    #[serde(default)]
    pub allowlist_sites: Vec<String>,
    /// Category-organized block/allow rules.
    #[serde(default)]
    pub categories: Vec<BlocklistCategoryConfig>,
    /// Name of the selected category inside this profile.
    #[serde(default = "default_blocklist_category_name")]
    pub selected_category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlocklistCategoryConfig {
    #[serde(default = "default_blocklist_category_name")]
    pub name: String,
    #[serde(default)]
    pub sites: Vec<String>,
    #[serde(default)]
    pub allowlist_sites: Vec<String>,
}

impl Default for BlocklistProfileConfig {
    fn default() -> Self {
        Self {
            name: default_blocklist_profile_name(),
            sites: Vec::new(),
            allowlist_sites: Vec::new(),
            categories: Vec::new(),
            selected_category: default_blocklist_category_name(),
        }
    }
}

impl Default for BlocklistCategoryConfig {
    fn default() -> Self {
        Self {
            name: default_blocklist_category_name(),
            sites: Vec::new(),
            allowlist_sites: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationConfig {
    #[serde(default = "default_notification_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub sound: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AutoStartConfig {
    #[serde(default)]
    pub focus_to_break: bool,
    #[serde(default)]
    pub break_to_focus: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RecurringScheduleConfig {
    #[serde(default)]
    pub windows: Vec<RecurringFocusWindowConfig>,
    #[serde(default)]
    pub exception_dates: Vec<String>,
    #[serde(default)]
    pub one_time_windows: Vec<OneTimeFocusWindowConfig>,
}

impl RecurringScheduleConfig {
    pub fn normalized(&self) -> Self {
        let windows = self
            .windows
            .iter()
            .map(RecurringFocusWindowConfig::normalized)
            .filter(|window| {
                let Some(start_minutes) = parse_schedule_time_minutes(&window.start) else {
                    return false;
                };
                let Some(end_minutes) = parse_schedule_time_minutes(&window.end) else {
                    return false;
                };
                start_minutes < end_minutes
            })
            .collect();
        let exception_dates = normalize_schedule_exception_dates(&self.exception_dates);
        let one_time_windows = self
            .one_time_windows
            .iter()
            .map(OneTimeFocusWindowConfig::normalized)
            .filter(|window| {
                if parse_schedule_exception_date(&window.date).is_none() {
                    return false;
                }
                let Some(start_minutes) = parse_schedule_time_minutes(&window.start) else {
                    return false;
                };
                let Some(end_minutes) = parse_schedule_time_minutes(&window.end) else {
                    return false;
                };
                start_minutes < end_minutes
            })
            .collect();
        Self {
            windows,
            exception_dates,
            one_time_windows,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleRuntimeConfig {
    #[serde(default = "default_schedule_time_step_minutes")]
    pub time_step_minutes: u16,
    #[serde(default = "default_schedule_delay_secs")]
    pub delay_secs: u64,
}

impl ScheduleRuntimeConfig {
    pub fn normalized(&self) -> Self {
        Self {
            time_step_minutes: self.time_step_minutes.clamp(
                SCHEDULE_TIME_STEP_MIN_MINUTES,
                SCHEDULE_TIME_STEP_MAX_MINUTES,
            ),
            delay_secs: self
                .delay_secs
                .clamp(SCHEDULE_DELAY_MIN_SECS, SCHEDULE_DELAY_MAX_SECS),
        }
    }
}

impl Default for ScheduleRuntimeConfig {
    fn default() -> Self {
        Self {
            time_step_minutes: default_schedule_time_step_minutes(),
            delay_secs: default_schedule_delay_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProfileAutomationConfig {
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub auto_start: AutoStartConfig,
    #[serde(default)]
    pub strict_mode: bool,
    #[serde(default)]
    pub recurring_schedule: RecurringScheduleConfig,
}

impl ProfileAutomationConfig {
    pub fn normalized(&self) -> Self {
        Self {
            notifications: self.notifications,
            auto_start: self.auto_start,
            strict_mode: self.strict_mode,
            recurring_schedule: self.recurring_schedule.normalized(),
        }
    }

    fn from_legacy(
        notifications: NotificationConfig,
        auto_start: AutoStartConfig,
        strict_mode: bool,
        recurring_schedule: RecurringScheduleConfig,
    ) -> Self {
        Self {
            notifications,
            auto_start,
            strict_mode,
            recurring_schedule,
        }
        .normalized()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProfileAutomationSettingsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classic: Option<ProfileAutomationConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deep_work: Option<ProfileAutomationConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<ProfileAutomationConfig>,
}

impl ProfileAutomationSettingsConfig {
    fn with_shared_defaults(shared: ProfileAutomationConfig) -> Self {
        let shared = Some(shared.normalized());
        Self {
            classic: shared.clone(),
            deep_work: shared.clone(),
            custom: shared,
        }
    }

    fn normalized_with_fallback(&self, fallback: &ProfileAutomationConfig) -> Self {
        Self {
            classic: Some(self.for_profile(ProfileId::Classic, fallback).normalized()),
            deep_work: Some(self.for_profile(ProfileId::DeepWork, fallback).normalized()),
            custom: Some(self.for_profile(ProfileId::Custom, fallback).normalized()),
        }
    }

    pub fn for_profile(
        &self,
        profile: ProfileId,
        fallback: &ProfileAutomationConfig,
    ) -> ProfileAutomationConfig {
        let configured = match profile {
            ProfileId::Classic => self.classic.clone(),
            ProfileId::DeepWork => self.deep_work.clone(),
            ProfileId::Custom => self.custom.clone(),
        };
        configured.unwrap_or_else(|| fallback.clone()).normalized()
    }

    pub fn set_for_profile(&mut self, profile: ProfileId, config: ProfileAutomationConfig) {
        let value = Some(config.normalized());
        match profile {
            ProfileId::Classic => self.classic = value,
            ProfileId::DeepWork => self.deep_work = value,
            ProfileId::Custom => self.custom = value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WeekdayProfileRuleConfig {
    #[serde(default = "default_weekday_profile_rule_day")]
    pub day: String,
    #[serde(default)]
    pub profile: ProfileId,
    #[serde(default = "default_blocklist_profile_name")]
    pub blocklist_profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_template: Option<String>,
}

impl WeekdayProfileRuleConfig {
    fn normalized_with_context(
        &self,
        blocklist_profiles: &[BlocklistProfileConfig],
        session_templates: &[SessionTemplateConfig],
    ) -> Option<Self> {
        let day = normalize_weekday_token(&self.day)?;
        let blocklist_profile =
            normalize_selected_blocklist_profile(&self.blocklist_profile, blocklist_profiles);
        let session_template = normalize_optional_selected_session_template(
            self.session_template.as_deref(),
            session_templates,
        );
        Some(Self {
            day,
            profile: self.profile,
            blocklist_profile,
            session_template,
        })
    }
}

impl Default for WeekdayProfileRuleConfig {
    fn default() -> Self {
        Self {
            day: default_weekday_profile_rule_day(),
            profile: ProfileId::default(),
            blocklist_profile: default_blocklist_profile_name(),
            session_template: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AutomationTriggerRuleConfig {
    pub trigger: AutomationTriggerConditionConfig,
    pub action: AutomationTriggerActionConfig,
}

impl AutomationTriggerRuleConfig {
    fn normalized_with_context(
        &self,
        blocklist_profiles: &[BlocklistProfileConfig],
        session_templates: &[SessionTemplateConfig],
    ) -> Option<Self> {
        let trigger = self.trigger.normalized()?;
        let action = self
            .action
            .normalized_with_context(blocklist_profiles, session_templates)?;
        Some(Self { trigger, action })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutomationTriggerConditionConfig {
    #[default]
    ScheduleWindowStart,
    ScheduleWindowEnd,
    FocusStarted,
    FocusCompleted,
    BreakStarted,
    BreakCompleted,
    Time {
        #[serde(default = "default_schedule_window_days")]
        days: Vec<String>,
        #[serde(default = "default_schedule_window_start")]
        at: String,
    },
}

impl AutomationTriggerConditionConfig {
    fn normalized(&self) -> Option<Self> {
        match self {
            Self::ScheduleWindowStart => Some(Self::ScheduleWindowStart),
            Self::ScheduleWindowEnd => Some(Self::ScheduleWindowEnd),
            Self::FocusStarted => Some(Self::FocusStarted),
            Self::FocusCompleted => Some(Self::FocusCompleted),
            Self::BreakStarted => Some(Self::BreakStarted),
            Self::BreakCompleted => Some(Self::BreakCompleted),
            Self::Time { days, at } => {
                let days = normalize_trigger_days(days)?;
                let at = normalize_schedule_time_or_default(at, default_schedule_window_start);
                Some(Self::Time { days, at })
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutomationTriggerActionConfig {
    #[default]
    StartFocus,
    DelayScheduleStart {
        #[serde(default = "default_schedule_delay_secs")]
        delay_secs: u64,
    },
    ApplyDefaults {
        #[serde(default)]
        profile: ProfileId,
        #[serde(default = "default_blocklist_profile_name")]
        blocklist_profile: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_template: Option<String>,
    },
}

impl AutomationTriggerActionConfig {
    fn normalized_with_context(
        &self,
        blocklist_profiles: &[BlocklistProfileConfig],
        session_templates: &[SessionTemplateConfig],
    ) -> Option<Self> {
        match self {
            Self::StartFocus => Some(Self::StartFocus),
            Self::DelayScheduleStart { delay_secs } => Some(Self::DelayScheduleStart {
                delay_secs: (*delay_secs).clamp(SCHEDULE_DELAY_MIN_SECS, SCHEDULE_DELAY_MAX_SECS),
            }),
            Self::ApplyDefaults {
                profile,
                blocklist_profile,
                session_template,
            } => Some(Self::ApplyDefaults {
                profile: *profile,
                blocklist_profile: normalize_selected_blocklist_profile(
                    blocklist_profile,
                    blocklist_profiles,
                ),
                session_template: normalize_optional_selected_session_template(
                    session_template.as_deref(),
                    session_templates,
                ),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecurringFocusWindowConfig {
    #[serde(default = "default_schedule_window_days")]
    pub days: Vec<String>,
    #[serde(default = "default_schedule_window_start")]
    pub start: String,
    #[serde(default = "default_schedule_window_end")]
    pub end: String,
}

impl RecurringFocusWindowConfig {
    pub fn normalized(&self) -> Self {
        Self {
            days: normalize_schedule_days(&self.days),
            start: normalize_schedule_time_or_default(&self.start, default_schedule_window_start),
            end: normalize_schedule_time_or_default(&self.end, default_schedule_window_end),
        }
    }
}

impl Default for RecurringFocusWindowConfig {
    fn default() -> Self {
        Self {
            days: default_schedule_window_days(),
            start: default_schedule_window_start(),
            end: default_schedule_window_end(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OneTimeFocusWindowConfig {
    #[serde(default)]
    pub date: String,
    #[serde(default = "default_schedule_window_start")]
    pub start: String,
    #[serde(default = "default_schedule_window_end")]
    pub end: String,
}

impl OneTimeFocusWindowConfig {
    pub fn normalized(&self) -> Self {
        Self {
            date: self.date.trim().to_string(),
            start: self.start.trim().to_string(),
            end: self.end.trim().to_string(),
        }
    }
}

impl Default for OneTimeFocusWindowConfig {
    fn default() -> Self {
        Self {
            date: default_one_time_schedule_date(),
            start: default_schedule_window_start(),
            end: default_schedule_window_end(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DailyGoalConfig {
    /// Target focused minutes for the current day.
    #[serde(default)]
    pub minutes: u64,
    /// Target completed pomodoros for the current day.
    #[serde(default)]
    pub pomodoros: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WeeklyGoalConfig {
    /// Target focused minutes for the current week.
    #[serde(default)]
    pub minutes: u64,
    /// Target completed pomodoros for the current week.
    #[serde(default)]
    pub pomodoros: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MonthlyGoalConfig {
    /// Target focused minutes for the current month.
    #[serde(default)]
    pub minutes: u64,
    /// Target completed pomodoros for the current month.
    #[serde(default)]
    pub pomodoros: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GoalCarryOverConfig {
    /// When enabled, unmet daily targets are added to the next day's target.
    #[serde(default)]
    pub daily: bool,
    /// When enabled, unmet weekly targets are added to the next week's target.
    #[serde(default)]
    pub weekly: bool,
    /// When enabled, unmet monthly targets are added to the next month's target.
    #[serde(default)]
    pub monthly: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatsRetentionPreset {
    KeepAll,
    Aggressive,
    #[default]
    Balanced,
}

impl StatsRetentionPreset {
    pub fn id(self) -> &'static str {
        match self {
            Self::KeepAll => "keep_all",
            Self::Balanced => "balanced",
            Self::Aggressive => "aggressive",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StatsRetentionConfig {
    #[serde(default)]
    pub preset: StatsRetentionPreset,
}

impl StatsRetentionConfig {
    pub fn windows(self) -> StatsRetentionWindows {
        match self.preset {
            StatsRetentionPreset::KeepAll => StatsRetentionWindows {
                keep_daily_days: None,
                keep_focus_sessions_days: None,
                keep_session_interruptions_days: None,
                keep_break_glass_overrides_days: None,
                keep_weekly_goal_snapshots_days: None,
                keep_monthly_goal_snapshots_days: None,
            },
            StatsRetentionPreset::Balanced => StatsRetentionWindows {
                keep_daily_days: None,
                keep_focus_sessions_days: Some(365),
                keep_session_interruptions_days: Some(180),
                keep_break_glass_overrides_days: Some(180),
                keep_weekly_goal_snapshots_days: Some(365),
                keep_monthly_goal_snapshots_days: Some(365),
            },
            StatsRetentionPreset::Aggressive => StatsRetentionWindows {
                keep_daily_days: Some(365),
                keep_focus_sessions_days: Some(180),
                keep_session_interruptions_days: Some(90),
                keep_break_glass_overrides_days: Some(90),
                keep_weekly_goal_snapshots_days: Some(180),
                keep_monthly_goal_snapshots_days: Some(180),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsRetentionWindows {
    pub keep_daily_days: Option<u16>,
    pub keep_focus_sessions_days: Option<u16>,
    pub keep_session_interruptions_days: Option<u16>,
    pub keep_break_glass_overrides_days: Option<u16>,
    pub keep_weekly_goal_snapshots_days: Option<u16>,
    pub keep_monthly_goal_snapshots_days: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WakatimeTaskMappingConfig {
    #[serde(default)]
    pub task_label: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WakatimeRuntimeConfig {
    #[serde(default = "default_wakatime_retry_backoff_secs")]
    pub retry_backoff_secs: Vec<u64>,
    #[serde(default = "default_wakatime_queue_capacity")]
    pub queue_capacity: usize,
    #[serde(default = "default_wakatime_queue_retry_delay_secs")]
    pub queue_retry_delay_secs: u64,
}

impl WakatimeRuntimeConfig {
    pub fn normalized(&self) -> Self {
        Self {
            retry_backoff_secs: normalize_wakatime_retry_backoff_secs(&self.retry_backoff_secs),
            queue_capacity: self
                .queue_capacity
                .clamp(WAKATIME_QUEUE_CAPACITY_MIN, WAKATIME_QUEUE_CAPACITY_MAX),
            queue_retry_delay_secs: self
                .queue_retry_delay_secs
                .clamp(WAKATIME_RETRY_DELAY_MIN_SECS, WAKATIME_RETRY_DELAY_MAX_SECS),
        }
    }
}

impl Default for WakatimeRuntimeConfig {
    fn default() -> Self {
        Self {
            retry_backoff_secs: default_wakatime_retry_backoff_secs(),
            queue_capacity: default_wakatime_queue_capacity(),
            queue_retry_delay_secs: default_wakatime_queue_retry_delay_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WakatimeMetadataConfig {
    #[serde(default = "default_wakatime_project")]
    pub project: String,
    #[serde(default = "default_wakatime_language")]
    pub language: String,
    #[serde(default)]
    pub task_mappings: Vec<WakatimeTaskMappingConfig>,
}

impl WakatimeMetadataConfig {
    pub fn normalized(&self) -> Self {
        Self {
            project: normalize_nonempty_or_default_string(
                &self.project,
                &default_wakatime_project(),
            ),
            language: normalize_nonempty_or_default_string(
                &self.language,
                &default_wakatime_language(),
            ),
            task_mappings: normalize_wakatime_task_mappings(&self.task_mappings),
        }
    }

    pub fn task_mapping_for_label(&self, task_label: &str) -> Option<&WakatimeTaskMappingConfig> {
        let task_label = task_label.trim();
        if task_label.is_empty() {
            return None;
        }
        self.task_mappings
            .iter()
            .find(|mapping| mapping.task_label.eq_ignore_ascii_case(task_label))
    }

    pub fn resolved_project_language_for_task_label(
        &self,
        task_label: Option<&str>,
    ) -> (String, String) {
        let Some(task_label) = task_label.map(str::trim).filter(|label| !label.is_empty()) else {
            return (self.project.clone(), self.language.clone());
        };
        let Some(mapping) = self.task_mapping_for_label(task_label) else {
            return (self.project.clone(), self.language.clone());
        };
        (
            mapping
                .project
                .clone()
                .unwrap_or_else(|| self.project.clone()),
            mapping
                .language
                .clone()
                .unwrap_or_else(|| self.language.clone()),
        )
    }
}

impl Default for WakatimeMetadataConfig {
    fn default() -> Self {
        Self {
            project: default_wakatime_project(),
            language: default_wakatime_language(),
            task_mappings: Vec::new(),
        }
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: default_notification_enabled(),
            sound: false,
        }
    }
}

fn default_notification_enabled() -> bool {
    true
}

fn default_wakatime_project() -> String {
    "focustime".to_string()
}

fn default_wakatime_language() -> String {
    "Pomodoro".to_string()
}

fn default_wakatime_retry_backoff_secs() -> Vec<u64> {
    vec![2, 5, 10]
}

fn default_wakatime_queue_capacity() -> usize {
    512
}

fn default_wakatime_queue_retry_delay_secs() -> u64 {
    30
}

fn default_schedule_time_step_minutes() -> u16 {
    15
}

fn default_schedule_delay_secs() -> u64 {
    10 * 60
}

fn default_schedule_window_days() -> Vec<String> {
    vec![
        "mon".to_string(),
        "tue".to_string(),
        "wed".to_string(),
        "thu".to_string(),
        "fri".to_string(),
    ]
}

fn default_weekday_profile_rule_day() -> String {
    "mon".to_string()
}

fn default_schedule_window_start() -> String {
    "09:00".to_string()
}

fn default_schedule_window_end() -> String {
    "10:00".to_string()
}

fn default_one_time_schedule_date() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn default_blocklist_profile_name() -> String {
    "Default".to_string()
}

fn default_blocklist_category_name() -> String {
    "General".to_string()
}

fn default_break_glass_duration_secs() -> u64 {
    5 * 60
}

fn default_break_template_name() -> String {
    "Classic".to_string()
}

fn default_session_template_name() -> String {
    "Template".to_string()
}

fn default_shortcut_quit() -> String {
    "q".to_string()
}

fn default_shortcut_timer_toggle_pause() -> String {
    "space".to_string()
}

fn default_shortcut_timer_stop_reset() -> String {
    "s".to_string()
}

fn default_shortcut_timer_next_phase() -> String {
    "n".to_string()
}

fn default_shortcut_open_site_manager() -> String {
    "b".to_string()
}

fn default_shortcut_open_profile_manager() -> String {
    "p".to_string()
}

fn default_shortcut_open_session_planner() -> String {
    "t".to_string()
}

fn default_shortcut_open_stats_history() -> String {
    "h".to_string()
}

fn default_shortcut_open_setup_diagnostics() -> String {
    "d".to_string()
}

fn default_shortcut_timer_edit_note() -> String {
    "m".to_string()
}

fn default_shortcut_break_glass_override() -> String {
    "u".to_string()
}

fn default_shortcut_delay_schedule_start() -> String {
    "z".to_string()
}

fn default_shortcut_back_site_manager() -> String {
    "b".to_string()
}

fn default_shortcut_toggle_site_list_mode() -> String {
    "m".to_string()
}

fn default_shortcut_site_add() -> String {
    "a".to_string()
}

fn default_shortcut_site_edit() -> String {
    "e".to_string()
}

fn default_shortcut_site_delete() -> String {
    "d".to_string()
}

fn default_shortcut_select_previous_blocklist_profile() -> String {
    "[".to_string()
}

fn default_shortcut_select_next_blocklist_profile() -> String {
    "]".to_string()
}

fn default_shortcut_create_blocklist_profile() -> String {
    "n".to_string()
}

fn default_shortcut_rename_blocklist_profile() -> String {
    "r".to_string()
}

fn default_shortcut_delete_blocklist_profile() -> String {
    "x".to_string()
}

fn default_shortcut_back_session_planner() -> String {
    "t".to_string()
}

fn default_shortcut_planner_add() -> String {
    "a".to_string()
}

fn default_shortcut_planner_rename() -> String {
    "e".to_string()
}

fn default_shortcut_planner_favorite() -> String {
    "f".to_string()
}

fn default_shortcut_planner_archive() -> String {
    "x".to_string()
}

fn default_shortcut_planner_delete() -> String {
    "d".to_string()
}

fn default_shortcut_planner_select_recent() -> String {
    "r".to_string()
}

fn default_shortcut_back_profile_manager() -> String {
    "p".to_string()
}

fn default_shortcut_profile_edit() -> String {
    "e".to_string()
}

fn default_shortcut_select_previous_break_template() -> String {
    "[".to_string()
}

fn default_shortcut_select_next_break_template() -> String {
    "]".to_string()
}

fn default_shortcut_back_stats_history() -> String {
    "h".to_string()
}

fn default_shortcut_export_stats_history() -> String {
    "e".to_string()
}

fn default_shortcut_back_setup_diagnostics() -> String {
    "d".to_string()
}

fn default_shortcut_refresh_setup_diagnostics() -> String {
    "r".to_string()
}

fn default_shortcut_navigate_up() -> String {
    "up".to_string()
}

fn default_shortcut_navigate_down() -> String {
    "down".to_string()
}

fn default_shortcut_navigate_left() -> String {
    "left".to_string()
}

fn default_shortcut_navigate_right() -> String {
    "right".to_string()
}

fn default_shortcut_confirm() -> String {
    "enter".to_string()
}

fn default_shortcut_cancel() -> String {
    "esc".to_string()
}

fn default_shortcut_delete() -> String {
    "delete".to_string()
}

fn default_shortcut_backspace() -> String {
    "backspace".to_string()
}

fn default_break_templates() -> Vec<BreakTemplateConfig> {
    vec![
        BreakTemplateConfig {
            name: "Classic".to_string(),
            short_break_secs: default_short_break_secs(),
            long_break_secs: default_long_break_secs(),
            long_break_interval: default_long_break_interval(),
        },
        BreakTemplateConfig {
            name: "Deep Work".to_string(),
            short_break_secs: 10 * 60,
            long_break_secs: 30 * 60,
            long_break_interval: 3,
        },
    ]
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileId {
    Classic,
    DeepWork,
    #[default]
    Custom,
}

impl ProfileId {
    fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "classic" => Self::Classic,
            "deep-work" | "deep_work" | "deepwork" => Self::DeepWork,
            "custom" => Self::Custom,
            _ => Self::Custom,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ProfileId::Classic => "Classic",
            ProfileId::DeepWork => "Deep Work",
            ProfileId::Custom => "Custom",
        }
    }
}

impl<'de> Deserialize<'de> for ProfileId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_config_value(&value))
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreset {
    #[default]
    Classic,
    HighContrast,
    DeuteranopiaFriendly,
}

impl ThemePreset {
    fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "classic" => Self::Classic,
            "high-contrast" | "high_contrast" | "highcontrast" => Self::HighContrast,
            "deuteranopia-friendly"
            | "deuteranopia_friendly"
            | "deuteranopiafriendly"
            | "colorblind-friendly"
            | "colorblind_friendly"
            | "colorblindfriendly" => Self::DeuteranopiaFriendly,
            _ => Self::Classic,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::HighContrast => "high-contrast",
            Self::DeuteranopiaFriendly => "deuteranopia-friendly",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::HighContrast => "High Contrast",
            Self::DeuteranopiaFriendly => "Deuteranopia Friendly",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Classic => Self::HighContrast,
            Self::HighContrast => Self::DeuteranopiaFriendly,
            Self::DeuteranopiaFriendly => Self::Classic,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Classic => Self::DeuteranopiaFriendly,
            Self::HighContrast => Self::Classic,
            Self::DeuteranopiaFriendly => Self::HighContrast,
        }
    }
}

impl<'de> Deserialize<'de> for ThemePreset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_config_value(&value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomProfileConfig {
    #[serde(default = "default_focus_secs")]
    pub focus_secs: u64,
    #[serde(default = "default_short_break_secs")]
    pub short_break_secs: u64,
    #[serde(default = "default_long_break_secs")]
    pub long_break_secs: u64,
    #[serde(default = "default_long_break_interval")]
    pub long_break_interval: u32,
}

impl CustomProfileConfig {
    pub fn normalized(&self) -> Self {
        Self {
            focus_secs: nonzero_or_default_u64(self.focus_secs, default_focus_secs()),
            short_break_secs: nonzero_or_default_u64(
                self.short_break_secs,
                default_short_break_secs(),
            ),
            long_break_secs: nonzero_or_default_u64(
                self.long_break_secs,
                default_long_break_secs(),
            ),
            long_break_interval: nonzero_or_default_u32(
                self.long_break_interval,
                default_long_break_interval(),
            ),
        }
    }
}

impl Default for CustomProfileConfig {
    fn default() -> Self {
        Self {
            focus_secs: default_focus_secs(),
            short_break_secs: default_short_break_secs(),
            long_break_secs: default_long_break_secs(),
            long_break_interval: default_long_break_interval(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BreakTemplateConfig {
    #[serde(default = "default_break_template_name")]
    pub name: String,
    #[serde(default = "default_short_break_secs")]
    pub short_break_secs: u64,
    #[serde(default = "default_long_break_secs")]
    pub long_break_secs: u64,
    #[serde(default = "default_long_break_interval")]
    pub long_break_interval: u32,
}

impl BreakTemplateConfig {
    pub fn normalized(&self) -> Self {
        Self {
            name: normalize_nonempty_or_default_string(&self.name, &default_break_template_name()),
            short_break_secs: nonzero_or_default_u64(
                self.short_break_secs,
                default_short_break_secs(),
            ),
            long_break_secs: nonzero_or_default_u64(
                self.long_break_secs,
                default_long_break_secs(),
            ),
            long_break_interval: nonzero_or_default_u32(
                self.long_break_interval,
                default_long_break_interval(),
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionTemplateConfig {
    #[serde(default = "default_session_template_name")]
    pub name: String,
    #[serde(default)]
    pub task_label: String,
    #[serde(default)]
    pub profile: ProfileId,
    #[serde(default = "default_blocklist_profile_name")]
    pub blocklist_profile: String,
    #[serde(default)]
    pub schedule: RecurringScheduleConfig,
}

impl SessionTemplateConfig {
    pub fn normalized_with_blocklists(
        &self,
        blocklist_profiles: &[BlocklistProfileConfig],
    ) -> Option<Self> {
        let name =
            normalize_nonempty_or_default_string(&self.name, &default_session_template_name());
        let task_label = normalize_optional_nonempty_string(Some(&self.task_label))?;
        let blocklist_profile =
            normalize_selected_blocklist_profile(&self.blocklist_profile, blocklist_profiles);
        Some(Self {
            name,
            task_label,
            profile: self.profile,
            blocklist_profile,
            schedule: self.schedule.normalized(),
        })
    }
}

impl Default for BreakTemplateConfig {
    fn default() -> Self {
        Self {
            name: default_break_template_name(),
            short_break_secs: default_short_break_secs(),
            long_break_secs: default_long_break_secs(),
            long_break_interval: default_long_break_interval(),
        }
    }
}

fn default_focus_secs() -> u64 {
    crate::timer::DEFAULT_FOCUS_SECS
}
fn default_short_break_secs() -> u64 {
    crate::timer::DEFAULT_SHORT_BREAK_SECS
}
fn default_long_break_secs() -> u64 {
    crate::timer::DEFAULT_LONG_BREAK_SECS
}
fn default_long_break_interval() -> u32 {
    crate::timer::DEFAULT_LONG_BREAK_INTERVAL
}
fn default_legacy_config_schema_version() -> u32 {
    LEGACY_CONFIG_SCHEMA_VERSION
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            focus_secs: default_focus_secs(),
            short_break_secs: default_short_break_secs(),
            long_break_secs: default_long_break_secs(),
            long_break_interval: default_long_break_interval(),
            blocked_sites: Vec::new(),
            blocklist_profiles: Vec::new(),
            selected_blocklist_profile: default_blocklist_profile_name(),
            blocking_backend: BlockingBackendConfig::default(),
            selected_profile: ProfileId::default(),
            custom_profile: None,
            break_templates: default_break_templates(),
            selected_break_template: default_break_template_name(),
            weekday_profile_rules: Vec::new(),
            session_templates: Vec::new(),
            selected_session_template: String::new(),
            automation_triggers: Vec::new(),
            selected_theme_preset: ThemePreset::default(),
            notifications: NotificationConfig::default(),
            auto_start: AutoStartConfig::default(),
            recurring_schedule: RecurringScheduleConfig::default(),
            schedule_runtime: ScheduleRuntimeConfig::default(),
            profile_automation: None,
            strict_mode: false,
            break_glass_duration_secs: default_break_glass_duration_secs(),
            daily_goal: DailyGoalConfig::default(),
            weekly_goal: WeeklyGoalConfig::default(),
            monthly_goal: MonthlyGoalConfig::default(),
            goal_carry_over: GoalCarryOverConfig::default(),
            stats_retention: StatsRetentionConfig::default(),
            wakatime: WakatimeMetadataConfig::default(),
            wakatime_runtime: WakatimeRuntimeConfig::default(),
            feature_flags: FeatureFlagsConfig::default(),
            shortcuts: ShortcutConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn normalized(self) -> Self {
        self.normalize()
    }

    pub(crate) fn load_with_deprecation_warnings() -> (Self, Vec<String>) {
        Self::try_load_with_deprecation_warnings().unwrap_or_else(|| (Self::default(), Vec::new()))
    }

    /// Load the config from disk, falling back to [`AppConfig::default`] on any
    /// error (missing file, parse error, corrupt data, etc.).
    #[cfg_attr(test, allow(dead_code))]
    pub fn load() -> Self {
        Self::load_with_deprecation_warnings().0
    }

    /// Returns the effective custom profile configuration.
    ///
    /// If `custom_profile` is not present in the config file, this derives
    /// values from legacy duration fields to preserve user settings.
    pub fn effective_custom_profile(&self) -> CustomProfileConfig {
        self.custom_profile
            .clone()
            .unwrap_or(CustomProfileConfig {
                focus_secs: self.focus_secs,
                short_break_secs: self.short_break_secs,
                long_break_secs: self.long_break_secs,
                long_break_interval: self.long_break_interval,
            })
            .normalized()
    }

    pub fn profile_automation_for(&self, profile: ProfileId) -> ProfileAutomationConfig {
        let fallback = ProfileAutomationConfig::default();
        self.profile_automation
            .as_ref()
            .map(|settings| settings.for_profile(profile, &fallback))
            .unwrap_or(fallback)
    }

    pub fn set_profile_automation_for(
        &mut self,
        profile: ProfileId,
        automation: ProfileAutomationConfig,
    ) {
        let fallback = ProfileAutomationConfig::default();
        let mut settings = self
            .profile_automation
            .clone()
            .unwrap_or_else(|| {
                ProfileAutomationSettingsConfig::with_shared_defaults(fallback.clone())
            })
            .normalized_with_fallback(&fallback);
        settings.set_for_profile(profile, automation);
        self.profile_automation = Some(settings.normalized_with_fallback(&fallback));
    }

    fn try_load_with_deprecation_warnings() -> Option<(Self, Vec<String>)> {
        Self::try_load_with_env_and_deprecation_warnings(|key| std::env::var_os(key))
    }

    #[cfg(test)]
    fn load_with_env(get_var: impl FnMut(&str) -> Option<OsString>) -> Self {
        Self::load_with_env_and_deprecation_warnings(get_var).0
    }

    #[cfg(test)]
    fn load_with_env_and_deprecation_warnings(
        get_var: impl FnMut(&str) -> Option<OsString>,
    ) -> (Self, Vec<String>) {
        Self::try_load_with_env_and_deprecation_warnings(get_var)
            .unwrap_or_else(|| (Self::default(), Vec::new()))
    }

    fn try_load_with_env_and_deprecation_warnings(
        get_var: impl FnMut(&str) -> Option<OsString>,
    ) -> Option<(Self, Vec<String>)> {
        let path = Self::config_path_with_env(get_var)?;
        let content = fs::read_to_string(path).ok()?;
        let config_toml: toml::Value = toml::from_str(&content).ok()?;
        let migrated_toml = migrate_config_toml_to_current(config_toml)?;
        let disk: AppConfigDisk = migrated_toml.try_into().ok()?;
        let deprecation_warnings = detect_legacy_config_deprecation_warnings(&disk.config);
        Some((disk.config.normalize(), deprecation_warnings))
    }

    /// Persist the current config to disk.
    /// Creates parent directories as needed.
    #[cfg_attr(test, allow(dead_code))]
    pub fn save(&self) -> io::Result<()> {
        let path = Self::config_path().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "cannot determine config directory")
        })?;
        self.save_to_path(path)
    }

    #[cfg(test)]
    fn save_with_env(&self, get_var: impl FnMut(&str) -> Option<OsString>) -> io::Result<()> {
        let path = Self::config_path_with_env(get_var).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "cannot determine config directory")
        })?;
        self.save_to_path(path)
    }

    fn save_to_path(&self, path: PathBuf) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(&AppConfigDisk::from_config(self.clone()))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Best-effort atomic write: temp file + rename.
        // On Windows, rename cannot replace an existing file; we fall back to
        // remove+rename when the destination already exists.
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, &content)?;
        #[cfg(target_os = "windows")]
        {
            match fs::rename(&tmp, &path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    fs::remove_file(&path)?;
                    fs::rename(&tmp, &path)
                }
                Err(e) => Err(e),
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            fs::rename(&tmp, &path)
        }
    }

    fn normalize(mut self) -> Self {
        self.focus_secs = nonzero_or_default_u64(self.focus_secs, default_focus_secs());
        self.short_break_secs =
            nonzero_or_default_u64(self.short_break_secs, default_short_break_secs());
        self.long_break_secs =
            nonzero_or_default_u64(self.long_break_secs, default_long_break_secs());
        self.long_break_interval =
            nonzero_or_default_u32(self.long_break_interval, default_long_break_interval());
        self.break_glass_duration_secs = nonzero_or_default_u64(
            self.break_glass_duration_secs,
            default_break_glass_duration_secs(),
        );
        self.feature_flags = self.feature_flags.normalized();
        self.custom_profile = self.custom_profile.map(|profile| profile.normalized());
        self.break_templates = normalize_break_templates(&self.break_templates);
        let effective_custom_profile = self.effective_custom_profile();
        self.selected_break_template = normalize_selected_break_template(
            &self.selected_break_template,
            &self.break_templates,
            &effective_custom_profile,
        );
        self.recurring_schedule = self.recurring_schedule.normalized();
        let legacy_automation = ProfileAutomationConfig::from_legacy(
            self.notifications,
            self.auto_start,
            self.strict_mode,
            self.recurring_schedule.clone(),
        );
        let fallback_automation = ProfileAutomationConfig::default();
        let normalized_profile_automation = self
            .profile_automation
            .clone()
            .unwrap_or_else(|| {
                ProfileAutomationSettingsConfig::with_shared_defaults(legacy_automation)
            })
            .normalized_with_fallback(&fallback_automation);
        self.profile_automation = Some(normalized_profile_automation);
        self.blocklist_profiles =
            normalize_blocklist_profiles(&self.blocklist_profiles, &self.blocked_sites);
        self.selected_blocklist_profile = normalize_selected_blocklist_profile(
            &self.selected_blocklist_profile,
            &self.blocklist_profiles,
        );
        self.session_templates =
            normalize_session_templates(&self.session_templates, &self.blocklist_profiles);
        self.selected_session_template = normalize_selected_session_template(
            &self.selected_session_template,
            &self.session_templates,
        );
        self.automation_triggers = normalize_automation_triggers(
            &self.automation_triggers,
            &self.blocklist_profiles,
            &self.session_templates,
        );
        self.weekday_profile_rules = normalize_weekday_profile_rules(
            &self.weekday_profile_rules,
            &self.blocklist_profiles,
            &self.session_templates,
        );
        self.blocking_backend = self.blocking_backend.normalized();
        self.schedule_runtime = self.schedule_runtime.normalized();
        self.wakatime = self.wakatime.normalized();
        self.wakatime_runtime = self.wakatime_runtime.normalized();
        self.shortcuts = self.shortcuts.normalized();
        self
    }

    #[cfg_attr(test, allow(dead_code))]
    fn config_path() -> Option<PathBuf> {
        app_data_path("config.toml")
    }

    fn config_path_with_env(get_var: impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
        let app_dir = app_dir_with_env(get_var)?;
        Some(app_dir.join("config.toml"))
    }
}

fn migrate_config_toml_to_current(mut config_toml: toml::Value) -> Option<toml::Value> {
    let schema_version = detect_config_schema_version(&config_toml)?;
    if schema_version > CURRENT_CONFIG_SCHEMA_VERSION {
        // Forward-compatibility mode: try best-effort deserialization with known fields.
        return Some(config_toml);
    }

    let mut from_schema_version = schema_version;
    while from_schema_version < CURRENT_CONFIG_SCHEMA_VERSION {
        config_toml = migrate_config_toml_step(config_toml, from_schema_version)?;
        from_schema_version += 1;
    }
    Some(config_toml)
}

fn detect_legacy_config_deprecation_warnings(config: &AppConfig) -> Vec<String> {
    let mut warnings = Vec::new();
    let duration_override_without_custom_profile = config.custom_profile.is_none()
        && (config.focus_secs != default_focus_secs()
            || config.short_break_secs != default_short_break_secs()
            || config.long_break_secs != default_long_break_secs()
            || config.long_break_interval != default_long_break_interval());
    if duration_override_without_custom_profile {
        warnings.push(
            "Deprecated top-level timer fields (`focus_secs`, `short_break_secs`, `long_break_secs`, `long_break_interval`) are in use. Move these values into `[custom_profile]`.".to_string(),
        );
    }

    let legacy_automation_values_configured = config.notifications != NotificationConfig::default()
        || config.auto_start != AutoStartConfig::default()
        || config.strict_mode
        || config.recurring_schedule != RecurringScheduleConfig::default();
    let profile_automation_incomplete = config
        .profile_automation
        .as_ref()
        .map(|settings| {
            settings.classic.is_none() || settings.deep_work.is_none() || settings.custom.is_none()
        })
        .unwrap_or(true);
    let legacy_automation_in_use =
        legacy_automation_values_configured && profile_automation_incomplete;
    if legacy_automation_in_use {
        warnings.push(
            "Deprecated top-level automation fields (`notifications`, `auto_start`, `strict_mode`, `recurring_schedule`) are in use. Move them under `[profile_automation.<profile>]`.".to_string(),
        );
    }

    if config.blocklist_profiles.is_empty() && !config.blocked_sites.is_empty() {
        warnings.push(
            "Deprecated `blocked_sites` is in use without `[[blocklist_profiles]]`. Move entries into a blocklist profile (for example `Default`).".to_string(),
        );
    }

    warnings
}

fn detect_config_schema_version(config_toml: &toml::Value) -> Option<u32> {
    let table = config_toml.as_table()?;
    table
        .get("schema_version")
        .map(|value| value.as_integer().and_then(|raw| u32::try_from(raw).ok()))
        .unwrap_or(Some(LEGACY_CONFIG_SCHEMA_VERSION))
}

fn migrate_config_toml_step(
    config_toml: toml::Value,
    from_schema_version: u32,
) -> Option<toml::Value> {
    match from_schema_version {
        LEGACY_CONFIG_SCHEMA_VERSION => migrate_config_toml_legacy_to_v1(config_toml),
        _ => None,
    }
}

fn migrate_config_toml_legacy_to_v1(mut config_toml: toml::Value) -> Option<toml::Value> {
    let table = config_toml.as_table_mut()?;
    table.insert(
        "schema_version".to_string(),
        toml::Value::Integer(i64::from(CURRENT_CONFIG_SCHEMA_VERSION)),
    );
    Some(config_toml)
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn app_data_path(file_name: &str) -> Option<PathBuf> {
    let app_dir = app_dir()?;
    Some(app_dir.join(file_name))
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn stats_data_path(file_name: &str) -> Option<PathBuf> {
    let app_dir = stats_app_dir()?;
    Some(app_dir.join(file_name))
}

fn app_dir() -> Option<PathBuf> {
    app_dir_with_env(|key| std::env::var_os(key))
}

fn stats_app_dir() -> Option<PathBuf> {
    stats_app_dir_with_env(|key| std::env::var_os(key))
}

fn nonzero_or_default_u64(value: u64, default: u64) -> u64 {
    if value == 0 { default } else { value }
}

fn nonzero_or_default_u32(value: u32, default: u32) -> u32 {
    if value == 0 { default } else { value }
}

fn normalize_nonempty_or_default_string(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_optional_nonempty_string(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_wakatime_task_mappings(
    mappings: &[WakatimeTaskMappingConfig],
) -> Vec<WakatimeTaskMappingConfig> {
    let mut normalized = Vec::new();
    let mut seen_labels = HashSet::new();
    for mapping in mappings {
        let Some(task_label) = normalize_optional_nonempty_string(Some(&mapping.task_label)) else {
            continue;
        };
        let project = normalize_optional_nonempty_string(mapping.project.as_deref());
        let language = normalize_optional_nonempty_string(mapping.language.as_deref());
        if project.is_none() && language.is_none() {
            continue;
        }
        let key = task_label.to_ascii_lowercase();
        if seen_labels.insert(key) {
            normalized.push(WakatimeTaskMappingConfig {
                task_label,
                project,
                language,
            });
        }
    }
    normalized
}

fn normalize_wakatime_retry_backoff_secs(backoff_secs: &[u64]) -> Vec<u64> {
    let normalized = backoff_secs
        .iter()
        .filter_map(|secs| {
            if *secs == 0 {
                None
            } else {
                Some((*secs).clamp(
                    WAKATIME_RETRY_BACKOFF_MIN_SECS,
                    WAKATIME_RETRY_BACKOFF_MAX_SECS,
                ))
            }
        })
        .take(WAKATIME_RETRY_BACKOFF_MAX_ENTRIES)
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        default_wakatime_retry_backoff_secs()
    } else {
        normalized
    }
}

fn parse_shortcut_char(value: &str) -> Option<char> {
    let token = value.trim();
    if token.eq_ignore_ascii_case("space") {
        return Some(' ');
    }

    let mut chars = token.chars();
    let candidate = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    if candidate.is_ascii_alphabetic() {
        Some(candidate.to_ascii_lowercase())
    } else {
        Some(candidate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortcutKeyToken {
    Char(char),
    Enter,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Backspace,
    Delete,
}

fn parse_shortcut_key_token(value: &str) -> Option<ShortcutKeyToken> {
    let token = value.trim();
    if token.is_empty() {
        return None;
    }

    match token.to_ascii_lowercase().as_str() {
        "enter" => Some(ShortcutKeyToken::Enter),
        "esc" | "escape" => Some(ShortcutKeyToken::Esc),
        "up" => Some(ShortcutKeyToken::Up),
        "down" => Some(ShortcutKeyToken::Down),
        "left" => Some(ShortcutKeyToken::Left),
        "right" => Some(ShortcutKeyToken::Right),
        "backspace" => Some(ShortcutKeyToken::Backspace),
        "delete" | "del" => Some(ShortcutKeyToken::Delete),
        _ => parse_shortcut_char(token).map(ShortcutKeyToken::Char),
    }
}

fn normalize_shortcut_token(value: &str, default: &str) -> String {
    let Some(parsed) = parse_shortcut_char(value) else {
        return default.to_string();
    };
    if parsed == ' ' {
        "space".to_string()
    } else if parsed.is_ascii_alphabetic() {
        parsed.to_ascii_lowercase().to_string()
    } else {
        parsed.to_string()
    }
}

fn normalize_navigation_shortcut_token(value: &str, default: &str) -> String {
    let Some(parsed) = parse_shortcut_key_token(value) else {
        return default.to_string();
    };

    match parsed {
        ShortcutKeyToken::Char(' ') => "space".to_string(),
        ShortcutKeyToken::Char(c) if c.is_ascii_alphabetic() => c.to_ascii_lowercase().to_string(),
        ShortcutKeyToken::Char(c) => c.to_string(),
        ShortcutKeyToken::Enter => "enter".to_string(),
        ShortcutKeyToken::Esc => "esc".to_string(),
        ShortcutKeyToken::Up => "up".to_string(),
        ShortcutKeyToken::Down => "down".to_string(),
        ShortcutKeyToken::Left => "left".to_string(),
        ShortcutKeyToken::Right => "right".to_string(),
        ShortcutKeyToken::Backspace => "backspace".to_string(),
        ShortcutKeyToken::Delete => "delete".to_string(),
    }
}

fn normalize_schedule_days(days: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    for day in days {
        let token = day.trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }
        if seen.insert(token.clone()) {
            normalized.push(token);
        }
    }
    if normalized.is_empty() {
        return default_schedule_window_days();
    }
    normalized
}

fn normalize_schedule_time_or_default(value: &str, default: fn() -> String) -> String {
    let candidate = value.trim();
    if parse_schedule_time_minutes(candidate).is_some() {
        candidate.to_string()
    } else {
        default()
    }
}

fn parse_schedule_time_minutes(value: &str) -> Option<u16> {
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

fn normalize_schedule_exception_dates(dates: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    for date in dates {
        let Some(parsed) = parse_schedule_exception_date(date) else {
            continue;
        };
        let canonical = parsed.format("%Y-%m-%d").to_string();
        if seen.insert(canonical.clone()) {
            normalized.push(canonical);
        }
    }
    normalized.sort();
    normalized
}

fn parse_schedule_exception_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

fn normalize_break_templates(templates: &[BreakTemplateConfig]) -> Vec<BreakTemplateConfig> {
    let mut normalized = Vec::new();
    let mut seen_names = HashSet::new();

    for template in templates {
        let template = template.normalized();
        let name = make_unique_profile_name(&template.name, &mut seen_names);
        normalized.push(BreakTemplateConfig { name, ..template });
    }

    if normalized.is_empty() {
        return default_break_templates();
    }

    normalized
}

fn normalize_selected_break_template(
    selected_name: &str,
    templates: &[BreakTemplateConfig],
    custom_profile: &CustomProfileConfig,
) -> String {
    let selected_name = selected_name.trim();
    if selected_name.is_empty() {
        return String::new();
    }

    if let Some(template) = templates.iter().find(|template| {
        template.name.eq_ignore_ascii_case(selected_name)
            && break_template_matches_custom_profile(template, custom_profile)
    }) {
        template.name.clone()
    } else {
        templates
            .iter()
            .find(|template| break_template_matches_custom_profile(template, custom_profile))
            .map(|template| template.name.clone())
            .unwrap_or_default()
    }
}

fn normalize_session_templates(
    templates: &[SessionTemplateConfig],
    blocklist_profiles: &[BlocklistProfileConfig],
) -> Vec<SessionTemplateConfig> {
    let mut normalized = Vec::new();
    let mut seen_names = HashSet::new();

    for template in templates {
        let Some(template) = template.normalized_with_blocklists(blocklist_profiles) else {
            continue;
        };
        let name = make_unique_profile_name(&template.name, &mut seen_names);
        normalized.push(SessionTemplateConfig { name, ..template });
    }

    normalized
}

fn normalize_selected_session_template(
    selected_name: &str,
    templates: &[SessionTemplateConfig],
) -> String {
    let selected_name = selected_name.trim();
    if selected_name.is_empty() {
        return String::new();
    }

    templates
        .iter()
        .find(|template| template.name.eq_ignore_ascii_case(selected_name))
        .map(|template| template.name.clone())
        .unwrap_or_default()
}

fn normalize_optional_selected_session_template(
    value: Option<&str>,
    session_templates: &[SessionTemplateConfig],
) -> Option<String> {
    let normalized = normalize_optional_nonempty_string(value)?;
    session_templates
        .iter()
        .find(|template| template.name.eq_ignore_ascii_case(&normalized))
        .map(|template| template.name.clone())
}

fn normalize_automation_triggers(
    rules: &[AutomationTriggerRuleConfig],
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
) -> Vec<AutomationTriggerRuleConfig> {
    rules
        .iter()
        .filter_map(|rule| rule.normalized_with_context(blocklist_profiles, session_templates))
        .collect()
}

pub fn validate_automation_trigger_rules(
    rules: &[AutomationTriggerRuleConfig],
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
) -> Result<(), String> {
    let mut seen_trigger_keys: HashMap<String, usize> = HashMap::new();
    for (index, rule) in rules.iter().enumerate() {
        validate_automation_trigger_rule(
            rule,
            index,
            blocklist_profiles,
            session_templates,
            &mut seen_trigger_keys,
        )?;
    }
    Ok(())
}

fn validate_automation_trigger_rule(
    rule: &AutomationTriggerRuleConfig,
    index: usize,
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
    seen_trigger_keys: &mut HashMap<String, usize>,
) -> Result<(), String> {
    validate_automation_trigger_condition(&rule.trigger, index)?;
    validate_automation_trigger_action(&rule.action, index, blocklist_profiles, session_templates)?;
    validate_automation_trigger_conflicts(&rule.trigger, index, seen_trigger_keys)?;
    Ok(())
}

fn validate_automation_trigger_condition(
    trigger: &AutomationTriggerConditionConfig,
    index: usize,
) -> Result<(), String> {
    match trigger {
        AutomationTriggerConditionConfig::ScheduleWindowStart
        | AutomationTriggerConditionConfig::ScheduleWindowEnd
        | AutomationTriggerConditionConfig::FocusStarted
        | AutomationTriggerConditionConfig::FocusCompleted
        | AutomationTriggerConditionConfig::BreakStarted
        | AutomationTriggerConditionConfig::BreakCompleted => Ok(()),
        AutomationTriggerConditionConfig::Time { days, at } => {
            if days.is_empty() {
                return Err(format!(
                    "Invalid automation trigger rule at index {index}: time trigger `days` cannot be empty."
                ));
            }
            for day in days {
                if weekday_token_to_index(day).is_none() {
                    return Err(format!(
                        "Invalid automation trigger rule at index {index}: unknown weekday `{day}` in time trigger."
                    ));
                }
            }
            if parse_schedule_time_minutes(at).is_none() {
                return Err(format!(
                    "Invalid automation trigger rule at index {index}: time trigger `at` must be HH:MM in 24-hour format."
                ));
            }
            Ok(())
        }
    }
}

fn validate_automation_trigger_action(
    action: &AutomationTriggerActionConfig,
    index: usize,
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
) -> Result<(), String> {
    match action {
        AutomationTriggerActionConfig::StartFocus => Ok(()),
        AutomationTriggerActionConfig::DelayScheduleStart { delay_secs } => {
            if *delay_secs < SCHEDULE_DELAY_MIN_SECS || *delay_secs > SCHEDULE_DELAY_MAX_SECS {
                return Err(format!(
                    "Invalid automation trigger rule at index {index}: `delay_secs` must be between {SCHEDULE_DELAY_MIN_SECS} and {SCHEDULE_DELAY_MAX_SECS}."
                ));
            }
            Ok(())
        }
        AutomationTriggerActionConfig::ApplyDefaults {
            blocklist_profile,
            session_template,
            ..
        } => validate_automation_trigger_apply_defaults_action(
            blocklist_profile,
            session_template.as_deref(),
            index,
            blocklist_profiles,
            session_templates,
        ),
    }
}

fn validate_automation_trigger_apply_defaults_action(
    blocklist_profile: &str,
    session_template: Option<&str>,
    index: usize,
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
) -> Result<(), String> {
    if blocklist_profile.trim().is_empty() {
        return Err(format!(
            "Invalid automation trigger rule at index {index}: `blocklist_profile` cannot be empty."
        ));
    }
    if !blocklist_profiles
        .iter()
        .any(|profile| profile.name.eq_ignore_ascii_case(blocklist_profile.trim()))
    {
        return Err(format!(
            "Invalid automation trigger rule at index {index}: blocklist profile `{}` does not exist.",
            blocklist_profile
        ));
    }
    if let Some(template) = session_template {
        if template.trim().is_empty() {
            return Err(format!(
                "Invalid automation trigger rule at index {index}: `session_template` cannot be empty when provided."
            ));
        }
        if !session_templates
            .iter()
            .any(|candidate| candidate.name.eq_ignore_ascii_case(template.trim()))
        {
            return Err(format!(
                "Invalid automation trigger rule at index {index}: session template `{template}` does not exist."
            ));
        }
    }
    Ok(())
}

fn validate_automation_trigger_conflicts(
    trigger: &AutomationTriggerConditionConfig,
    index: usize,
    seen_trigger_keys: &mut HashMap<String, usize>,
) -> Result<(), String> {
    let conflict_keys = automation_trigger_conflict_keys(trigger);
    for trigger_key in conflict_keys {
        if let Some(previous_index) = seen_trigger_keys.insert(trigger_key.clone(), index) {
            return Err(format!(
                "Conflicting automation trigger rules at indexes {previous_index} and {index}: both target `{}`. Keep one rule per trigger condition.",
                format_automation_trigger_conflict_key(&trigger_key)
            ));
        }
    }
    Ok(())
}

fn automation_trigger_conflict_keys(trigger: &AutomationTriggerConditionConfig) -> Vec<String> {
    match trigger {
        AutomationTriggerConditionConfig::ScheduleWindowStart => {
            vec!["schedule_window_start".to_string()]
        }
        AutomationTriggerConditionConfig::ScheduleWindowEnd => {
            vec!["schedule_window_end".to_string()]
        }
        AutomationTriggerConditionConfig::FocusStarted => vec!["focus_started".to_string()],
        AutomationTriggerConditionConfig::FocusCompleted => vec!["focus_completed".to_string()],
        AutomationTriggerConditionConfig::BreakStarted => vec!["break_started".to_string()],
        AutomationTriggerConditionConfig::BreakCompleted => vec!["break_completed".to_string()],
        AutomationTriggerConditionConfig::Time { days, at } => {
            let mut keys = Vec::new();
            for day in days {
                if let Some(day_index) = weekday_token_to_index(day) {
                    keys.push(format!(
                        "time:{}@{}",
                        weekday_token_from_index(day_index),
                        at
                    ));
                }
            }
            keys.sort();
            keys.dedup();
            keys
        }
    }
}

fn format_automation_trigger_conflict_key(key: &str) -> String {
    if let Some(rest) = key.strip_prefix("time:") {
        return format!("time trigger `{rest}`");
    }
    format!("event trigger `{key}`")
}

fn normalize_weekday_profile_rules(
    rules: &[WeekdayProfileRuleConfig],
    blocklist_profiles: &[BlocklistProfileConfig],
    session_templates: &[SessionTemplateConfig],
) -> Vec<WeekdayProfileRuleConfig> {
    let mut normalized_by_day: [Option<WeekdayProfileRuleConfig>; 7] =
        std::array::from_fn(|_| None);
    for rule in rules {
        let Some(normalized) = rule.normalized_with_context(blocklist_profiles, session_templates)
        else {
            continue;
        };
        let Some(day_index) = weekday_token_to_index(&normalized.day) else {
            continue;
        };
        normalized_by_day[day_index] = Some(normalized);
    }
    normalized_by_day.into_iter().flatten().collect()
}

fn normalize_weekday_token(value: &str) -> Option<String> {
    weekday_token_to_index(value).map(|index| weekday_token_from_index(index).to_string())
}

fn normalize_trigger_days(days: &[String]) -> Option<Vec<String>> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    for day in days {
        let Some(index) = weekday_token_to_index(day) else {
            continue;
        };
        let token = weekday_token_from_index(index).to_string();
        if seen.insert(token.clone()) {
            normalized.push(token);
        }
    }
    if normalized.is_empty() {
        return None;
    }
    normalized.sort_by_key(|day| weekday_token_to_index(day).unwrap_or(usize::MAX));
    Some(normalized)
}

fn weekday_token_to_index(value: &str) -> Option<usize> {
    match value.trim().to_ascii_lowercase().as_str() {
        "mon" | "monday" => Some(0),
        "tue" | "tues" | "tuesday" => Some(1),
        "wed" | "wednesday" => Some(2),
        "thu" | "thurs" | "thursday" => Some(3),
        "fri" | "friday" => Some(4),
        "sat" | "saturday" => Some(5),
        "sun" | "sunday" => Some(6),
        _ => None,
    }
}

fn weekday_token_from_index(index: usize) -> &'static str {
    match index {
        0 => "mon",
        1 => "tue",
        2 => "wed",
        3 => "thu",
        4 => "fri",
        5 => "sat",
        6 => "sun",
        _ => "mon",
    }
}

fn break_template_matches_custom_profile(
    template: &BreakTemplateConfig,
    custom_profile: &CustomProfileConfig,
) -> bool {
    let template = template.normalized();
    let custom_profile = custom_profile.normalized();
    template.short_break_secs == custom_profile.short_break_secs
        && template.long_break_secs == custom_profile.long_break_secs
        && template.long_break_interval == custom_profile.long_break_interval
}

fn normalize_blocklist_profiles(
    profiles: &[BlocklistProfileConfig],
    legacy_blocked_sites: &[String],
) -> Vec<BlocklistProfileConfig> {
    let mut normalized = Vec::new();
    let mut seen_names = HashSet::new();

    for profile in profiles {
        let base_name =
            normalize_nonempty_or_default_string(&profile.name, &default_blocklist_profile_name());
        let name = make_unique_profile_name(&base_name, &mut seen_names);
        let categories = normalize_blocklist_categories(
            &profile.categories,
            &profile.sites,
            &profile.allowlist_sites,
        );
        let selected_category =
            normalize_selected_blocklist_category(&profile.selected_category, &categories);
        let (sites, allowlist_sites) = flatten_blocklist_categories(&categories);
        normalized.push(BlocklistProfileConfig {
            name,
            sites,
            allowlist_sites,
            categories,
            selected_category,
        });
    }

    if normalized.is_empty() {
        let categories = normalize_blocklist_categories(&[], legacy_blocked_sites, &[]);
        let selected_category = normalize_selected_blocklist_category("", categories.as_slice());
        let (sites, allowlist_sites) = flatten_blocklist_categories(&categories);
        return vec![BlocklistProfileConfig {
            name: default_blocklist_profile_name(),
            sites,
            allowlist_sites,
            categories,
            selected_category,
        }];
    }

    normalized
}

fn normalize_selected_blocklist_profile(
    selected_name: &str,
    profiles: &[BlocklistProfileConfig],
) -> String {
    let selected_name = selected_name.trim();
    if selected_name.is_empty() {
        return profiles
            .first()
            .map(|profile| profile.name.clone())
            .unwrap_or_else(default_blocklist_profile_name);
    }

    if let Some(profile) = profiles
        .iter()
        .find(|profile| profile.name.eq_ignore_ascii_case(selected_name))
    {
        profile.name.clone()
    } else {
        profiles
            .first()
            .map(|profile| profile.name.clone())
            .unwrap_or_else(default_blocklist_profile_name)
    }
}

fn make_unique_profile_name(base_name: &str, seen_names: &mut HashSet<String>) -> String {
    if seen_names.insert(base_name.to_ascii_lowercase()) {
        return base_name.to_string();
    }

    let mut suffix = 2usize;
    loop {
        let candidate = format!("{base_name} ({suffix})");
        if seen_names.insert(candidate.to_ascii_lowercase()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn normalize_blocklist_categories(
    categories: &[BlocklistCategoryConfig],
    legacy_sites: &[String],
    legacy_allowlist_sites: &[String],
) -> Vec<BlocklistCategoryConfig> {
    let mut normalized = Vec::new();
    let mut seen_names = HashSet::new();
    for category in categories {
        let base_name = normalize_nonempty_or_default_string(
            &category.name,
            &default_blocklist_category_name(),
        );
        let name = make_unique_profile_name(&base_name, &mut seen_names);
        normalized.push(BlocklistCategoryConfig {
            name,
            sites: category.sites.clone(),
            allowlist_sites: category.allowlist_sites.clone(),
        });
    }

    if normalized.is_empty() {
        return vec![BlocklistCategoryConfig {
            name: default_blocklist_category_name(),
            sites: legacy_sites.to_vec(),
            allowlist_sites: legacy_allowlist_sites.to_vec(),
        }];
    }

    if !legacy_sites.is_empty() || !legacy_allowlist_sites.is_empty() {
        let target_index = normalized
            .iter()
            .position(|category| {
                category
                    .name
                    .eq_ignore_ascii_case(&default_blocklist_category_name())
            })
            .unwrap_or(0);
        merge_unique_case_insensitive(&mut normalized[target_index].sites, legacy_sites);
        merge_unique_case_insensitive(
            &mut normalized[target_index].allowlist_sites,
            legacy_allowlist_sites,
        );
    }

    normalized
}

fn merge_unique_case_insensitive(target: &mut Vec<String>, source: &[String]) {
    let mut seen: HashSet<String> = target
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect();
    for value in source {
        let key = value.to_ascii_lowercase();
        if seen.insert(key) {
            target.push(value.clone());
        }
    }
}

fn normalize_selected_blocklist_category(
    selected_name: &str,
    categories: &[BlocklistCategoryConfig],
) -> String {
    let selected_name = selected_name.trim();
    if selected_name.is_empty() {
        return categories
            .first()
            .map(|category| category.name.clone())
            .unwrap_or_else(default_blocklist_category_name);
    }

    if let Some(category) = categories
        .iter()
        .find(|category| category.name.eq_ignore_ascii_case(selected_name))
    {
        category.name.clone()
    } else {
        categories
            .first()
            .map(|category| category.name.clone())
            .unwrap_or_else(default_blocklist_category_name)
    }
}

fn flatten_blocklist_categories(
    categories: &[BlocklistCategoryConfig],
) -> (Vec<String>, Vec<String>) {
    let mut sites = Vec::new();
    let mut allowlist_sites = Vec::new();
    let mut seen_sites = HashSet::new();
    let mut seen_allowlist_sites = HashSet::new();

    for category in categories {
        for site in &category.sites {
            let key = site.to_ascii_lowercase();
            if seen_sites.insert(key) {
                sites.push(site.clone());
            }
        }
        for site in &category.allowlist_sites {
            let key = site.to_ascii_lowercase();
            if seen_allowlist_sites.insert(key) {
                allowlist_sites.push(site.clone());
            }
        }
    }

    (sites, allowlist_sites)
}

#[cfg(test)]
mod tests;
