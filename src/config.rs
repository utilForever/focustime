use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

mod blocklists;
mod diagnostics;
mod migration;
mod paths;
mod shortcuts;
mod wakatime;

pub(crate) use blocklists::{BlocklistProfileConfig, effective_blocked_sites_for_profile};
use blocklists::{
    default_blocklist_profile_name, normalize_blocklist_profiles,
    normalize_selected_blocklist_profile,
};
pub(crate) use diagnostics::{run_config_doctor, run_config_migration_assistant};
#[cfg(test)]
use diagnostics::{run_config_doctor_with_path, run_config_migration_assistant_with_path};
#[cfg(test)]
use migration::migrate_config_toml_legacy_to_v1;
use migration::{
    collect_legacy_profile_rename_advice, config_health_error, config_health_warning,
    detect_legacy_config_deprecation_warnings, migrate_config_toml_to_current,
    migrate_config_toml_to_current_detailed, sort_config_health_findings, summarize_config_health,
};
#[cfg(all(test, not(target_os = "windows")))]
use paths::env_path_from_value;
use paths::{app_dir_with_env, stats_app_dir_with_env};
#[cfg(test)]
use paths::{config_dir_from_env, stats_state_dir_from_env};
pub(crate) use shortcuts::ShortcutConfig;
pub(crate) use wakatime::{WakatimeMetadataConfig, WakatimeRuntimeConfig};

const CURRENT_CONFIG_SCHEMA_VERSION: u32 = 2;
const LEGACY_CONFIG_SCHEMA_VERSION: u32 = 0;
const SCHEDULE_TIME_STEP_MIN_MINUTES: u16 = 1;
const SCHEDULE_TIME_STEP_MAX_MINUTES: u16 = 60;

/// Persistent application configuration stored as TOML.
///
/// File locations:
/// - Unix / macOS: `$XDG_CONFIG_HOME/focustime/config.toml` if set,
///   otherwise `~/.config/focustime/config.toml`
/// - Windows:      `%APPDATA%\focustime\config.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AppConfig {
    /// Duration of a focus session in seconds (legacy load-time compatibility field).
    #[serde(default = "default_focus_secs", skip_serializing)]
    pub(crate) focus_secs: u64,
    /// Duration of a short-break session in seconds (legacy load-time compatibility field).
    #[serde(default = "default_short_break_secs", skip_serializing)]
    pub(crate) short_break_secs: u64,
    /// Duration of a long-break session in seconds (legacy load-time compatibility field).
    #[serde(default = "default_long_break_secs", skip_serializing)]
    pub(crate) long_break_secs: u64,
    /// Number of completed focus sessions before a long break.
    #[serde(default = "default_long_break_interval", skip_serializing)]
    pub(crate) long_break_interval: u32,
    /// Deprecated blocked-sites mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub(crate) blocked_sites: Vec<String>,
    /// Named blocklist profiles.
    ///
    /// Each profile stores a separate blocked-sites list. This field supports
    /// issue #110 and supersedes `blocked_sites` as the primary representation.
    #[serde(default)]
    pub(crate) blocklist_profiles: Vec<BlocklistProfileConfig>,
    /// Name of the active blocklist profile.
    #[serde(default = "default_blocklist_profile_name")]
    pub(crate) selected_blocklist_profile: String,
    /// Blocking backend selection and fallback behavior.
    #[serde(default)]
    pub(crate) blocking_backend: BlockingBackendConfig,
    /// Selected profile identifier.
    #[serde(default)]
    pub(crate) selected_profile: ProfileId,
    /// Editable custom profile persisted by the app.
    ///
    /// When this is absent, the app derives it from the legacy duration fields.
    /// This is the canonical persisted timer-duration surface.
    #[serde(default)]
    pub(crate) custom_profile: Option<CustomProfileConfig>,
    /// Selected UI theme preset.
    #[serde(default)]
    pub(crate) selected_theme_preset: ThemePreset,
    /// Deprecated top-level automation mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub(crate) notifications: NotificationConfig,
    /// Deprecated top-level automation mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub(crate) auto_start: AutoStartConfig,
    /// Deprecated top-level automation mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub(crate) recurring_schedule: RecurringScheduleConfig,
    /// Runtime tuning knobs for schedule editing behavior.
    #[serde(default)]
    pub(crate) schedule_runtime: ScheduleRuntimeConfig,
    /// Profile-scoped automation settings.
    ///
    /// When absent, legacy global automation fields are used as shared defaults
    /// for all profiles during normalization.
    #[serde(default)]
    pub(crate) profile_automation: Option<ProfileAutomationSettingsConfig>,
    /// Deprecated top-level automation mirror (legacy load-time compatibility field).
    #[serde(default, skip_serializing)]
    pub(crate) strict_mode: bool,
    /// Duration of a break-glass unblock override in seconds.
    ///
    /// This value is clamped to a non-zero default during normalization.
    #[serde(default = "default_break_glass_duration_secs")]
    pub(crate) break_glass_duration_secs: u64,
    /// Daily goal settings for focus minutes and completed pomodoros.
    ///
    /// A value of `0` disables the corresponding goal.
    #[serde(default)]
    pub(crate) daily_goal: DailyGoalConfig,
    /// Weekly goal settings for focus minutes and completed pomodoros.
    ///
    /// A value of `0` disables the corresponding goal.
    #[serde(default)]
    pub(crate) weekly_goal: WeeklyGoalConfig,
    /// Monthly goal settings for focus minutes and completed pomodoros.
    ///
    /// A value of `0` disables the corresponding goal.
    #[serde(default)]
    pub(crate) monthly_goal: MonthlyGoalConfig,
    /// Carry-over behavior for unmet daily/weekly/monthly targets.
    #[serde(default)]
    pub(crate) goal_carry_over: GoalCarryOverConfig,
    /// Retention policy for persisted stats history.
    #[serde(default)]
    pub(crate) stats_retention: StatsRetentionConfig,
    /// Deprecated history dashboard KPI card customization surface.
    ///
    /// Older config files may still contain this section, but the app now
    /// always uses the stable default KPI layout and does not persist it.
    #[serde(default, skip_serializing)]
    pub(crate) history_dashboard: HistoryDashboardConfig,
    /// WakaTime heartbeat metadata labels.
    #[serde(default)]
    pub(crate) wakatime: WakatimeMetadataConfig,
    /// Runtime tuning knobs for WakaTime retry/queue behavior.
    #[serde(default)]
    pub(crate) wakatime_runtime: WakatimeRuntimeConfig,
    /// Feature flags used to safely gate compatibility-sensitive behavior.
    #[serde(default)]
    pub(crate) feature_flags: FeatureFlagsConfig,
    /// User-configurable keyboard shortcuts for core TUI command actions.
    #[serde(default)]
    pub(crate) shortcuts: ShortcutConfig,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConfigHealthStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConfigHealthSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ConfigHealthFinding {
    pub(crate) code: String,
    pub(crate) severity: ConfigHealthSeverity,
    pub(crate) message: String,
    pub(crate) remediation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ConfigMigrationStepReport {
    pub(crate) from_schema_version: u32,
    pub(crate) to_schema_version: u32,
    pub(crate) summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ConfigDoctorReport {
    pub(crate) action: &'static str,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) detected_schema_version: Option<u32>,
    pub(crate) current_schema_version: u32,
    pub(crate) status: ConfigHealthStatus,
    pub(crate) migration_steps: Vec<ConfigMigrationStepReport>,
    pub(crate) findings: Vec<ConfigHealthFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ConfigMigrationReport {
    pub(crate) action: &'static str,
    pub(crate) applied: bool,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) backup_path: Option<PathBuf>,
    pub(crate) detected_schema_version: Option<u32>,
    pub(crate) target_schema_version: u32,
    pub(crate) changed: bool,
    pub(crate) status: ConfigHealthStatus,
    pub(crate) steps: Vec<ConfigMigrationStepReport>,
    pub(crate) findings: Vec<ConfigHealthFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct FeatureFlagsConfig {
    #[serde(default)]
    pub(crate) integrations: IntegrationFeatureFlagsConfig,
}

impl FeatureFlagsConfig {
    pub(crate) fn normalized(&self) -> Self {
        Self {
            integrations: self.integrations.normalized(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct IntegrationFeatureFlagsConfig {
    #[serde(default = "default_enabled_integrations")]
    pub(crate) enabled: Vec<String>,
}

impl IntegrationFeatureFlagsConfig {
    pub(crate) fn normalized(&self) -> Self {
        let mut enabled = Vec::new();
        for integration in &self.enabled {
            let trimmed = integration.trim().to_ascii_lowercase();
            if trimmed.is_empty() || enabled.contains(&trimmed) {
                continue;
            }
            enabled.push(trimmed);
        }
        Self { enabled }
    }

    pub(crate) fn is_enabled(&self, integration: &str) -> bool {
        let normalized = integration.trim().to_ascii_lowercase();
        self.enabled
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&normalized))
    }
}

impl Default for IntegrationFeatureFlagsConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled_integrations(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct BlockingBackendConfig {
    #[serde(default)]
    pub(crate) policy: BlockingBackendPolicyConfig,
    #[serde(default)]
    pub(crate) command: CommandBlockingBackendConfig,
}

impl BlockingBackendConfig {
    pub(crate) fn normalized(&self) -> Self {
        Self {
            policy: self.policy,
            command: self.command.normalized(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BlockingBackendPolicyConfig {
    HostsOnly,
    #[default]
    HostsThenCommand,
    CommandThenHosts,
    CommandOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct CommandBlockingBackendConfig {
    #[serde(default)]
    pub(crate) block_command: String,
    #[serde(default)]
    pub(crate) unblock_command: String,
    #[serde(default)]
    pub(crate) diagnostics_command: String,
}

impl CommandBlockingBackendConfig {
    pub(crate) fn normalized(&self) -> Self {
        Self {
            block_command: self.block_command.trim().to_string(),
            unblock_command: self.unblock_command.trim().to_string(),
            diagnostics_command: self.diagnostics_command.trim().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct NotificationConfig {
    #[serde(default = "default_notification_enabled")]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) sound: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct AutoStartConfig {
    #[serde(default)]
    pub(crate) focus_to_break: bool,
    #[serde(default)]
    pub(crate) break_to_focus: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecurringScheduleConfig {
    #[serde(default)]
    pub(crate) windows: Vec<RecurringFocusWindowConfig>,
}

impl RecurringScheduleConfig {
    pub(crate) fn normalized(&self) -> Self {
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
        Self { windows }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ScheduleRuntimeConfig {
    #[serde(default = "default_schedule_time_step_minutes")]
    pub(crate) time_step_minutes: u16,
}

impl ScheduleRuntimeConfig {
    pub(crate) fn normalized(&self) -> Self {
        Self {
            time_step_minutes: self.time_step_minutes.clamp(
                SCHEDULE_TIME_STEP_MIN_MINUTES,
                SCHEDULE_TIME_STEP_MAX_MINUTES,
            ),
        }
    }
}

impl Default for ScheduleRuntimeConfig {
    fn default() -> Self {
        Self {
            time_step_minutes: default_schedule_time_step_minutes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct ProfileAutomationConfig {
    #[serde(default)]
    pub(crate) notifications: NotificationConfig,
    #[serde(default)]
    pub(crate) auto_start: AutoStartConfig,
    #[serde(default)]
    pub(crate) strict_mode: bool,
    #[serde(default)]
    pub(crate) recurring_schedule: RecurringScheduleConfig,
}

impl ProfileAutomationConfig {
    pub(crate) fn normalized(&self) -> Self {
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
pub(crate) struct ProfileAutomationSettingsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "classic")]
    pub(crate) basic: Option<ProfileAutomationConfig>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "deep_work",
        alias = "deep-work"
    )]
    pub(crate) standard: Option<ProfileAutomationConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "custom")]
    pub(crate) advanced: Option<ProfileAutomationConfig>,
}

impl ProfileAutomationSettingsConfig {
    fn with_shared_defaults(shared: ProfileAutomationConfig) -> Self {
        let shared = Some(shared.normalized());
        Self {
            basic: shared.clone(),
            standard: shared.clone(),
            advanced: shared,
        }
    }

    fn normalized_with_fallback(&self, fallback: &ProfileAutomationConfig) -> Self {
        Self {
            basic: Some(self.for_profile(ProfileId::Classic, fallback).normalized()),
            standard: Some(self.for_profile(ProfileId::DeepWork, fallback).normalized()),
            advanced: Some(self.for_profile(ProfileId::Custom, fallback).normalized()),
        }
    }

    pub(crate) fn for_profile(
        &self,
        profile: ProfileId,
        fallback: &ProfileAutomationConfig,
    ) -> ProfileAutomationConfig {
        let configured = match profile {
            ProfileId::Classic => self.basic.clone(),
            ProfileId::DeepWork => self.standard.clone(),
            ProfileId::Custom => self.advanced.clone(),
        };
        configured.unwrap_or_else(|| fallback.clone()).normalized()
    }

    pub(crate) fn set_for_profile(&mut self, profile: ProfileId, config: ProfileAutomationConfig) {
        let value = Some(config.normalized());
        match profile {
            ProfileId::Classic => self.basic = value,
            ProfileId::DeepWork => self.standard = value,
            ProfileId::Custom => self.advanced = value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RecurringFocusWindowConfig {
    #[serde(default = "default_schedule_window_days")]
    pub(crate) days: Vec<String>,
    #[serde(default = "default_schedule_window_start")]
    pub(crate) start: String,
    #[serde(default = "default_schedule_window_end")]
    pub(crate) end: String,
}

impl RecurringFocusWindowConfig {
    pub(crate) fn normalized(&self) -> Self {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct DailyGoalConfig {
    /// Target focused minutes for the current day.
    #[serde(default)]
    pub(crate) minutes: u64,
    /// Target completed pomodoros for the current day.
    #[serde(default)]
    pub(crate) pomodoros: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct WeeklyGoalConfig {
    /// Target focused minutes for the current week.
    #[serde(default)]
    pub(crate) minutes: u64,
    /// Target completed pomodoros for the current week.
    #[serde(default)]
    pub(crate) pomodoros: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct MonthlyGoalConfig {
    /// Target focused minutes for the current month.
    #[serde(default)]
    pub(crate) minutes: u64,
    /// Target completed pomodoros for the current month.
    #[serde(default)]
    pub(crate) pomodoros: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct GoalCarryOverConfig {
    /// When enabled, unmet daily targets are added to the next day's target.
    #[serde(default)]
    pub(crate) daily: bool,
    /// When enabled, unmet weekly targets are added to the next week's target.
    #[serde(default)]
    pub(crate) weekly: bool,
    /// When enabled, unmet monthly targets are added to the next month's target.
    #[serde(default)]
    pub(crate) monthly: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StatsRetentionPreset {
    KeepAll,
    Aggressive,
    #[default]
    Balanced,
}

impl StatsRetentionPreset {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::KeepAll => "keep_all",
            Self::Balanced => "balanced",
            Self::Aggressive => "aggressive",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct StatsRetentionConfig {
    #[serde(default)]
    pub(crate) preset: StatsRetentionPreset,
}

impl StatsRetentionConfig {
    pub(crate) fn windows(self) -> StatsRetentionWindows {
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
pub(crate) struct StatsRetentionWindows {
    pub(crate) keep_daily_days: Option<u16>,
    pub(crate) keep_focus_sessions_days: Option<u16>,
    pub(crate) keep_session_interruptions_days: Option<u16>,
    pub(crate) keep_break_glass_overrides_days: Option<u16>,
    pub(crate) keep_weekly_goal_snapshots_days: Option<u16>,
    pub(crate) keep_monthly_goal_snapshots_days: Option<u16>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HistoryKpiCardId {
    SessionSummary,
    FocusScore,
    GoalStreak,
    FocusRisk,
    WeeklyAllocation,
    LastInterruption,
    StatsGrowth,
    Retention,
    ComparisonFilters,
    Unknown,
}

impl HistoryKpiCardId {
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::SessionSummary => "session_summary",
            Self::FocusScore => "focus_score",
            Self::GoalStreak => "goal_streak",
            Self::FocusRisk => "focus_risk",
            Self::WeeklyAllocation => "weekly_allocation",
            Self::LastInterruption => "last_interruption",
            Self::StatsGrowth => "stats_growth",
            Self::Retention => "retention",
            Self::ComparisonFilters => "comparison_filters",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::SessionSummary => "Session Summary",
            Self::FocusScore => "Focus Score",
            Self::GoalStreak => "Goal Streak",
            Self::FocusRisk => "Focus Risk",
            Self::WeeklyAllocation => "Weekly Allocation",
            Self::LastInterruption => "Last Interruption",
            Self::StatsGrowth => "Stats Growth",
            Self::Retention => "Retention",
            Self::ComparisonFilters => "Comparison Filters",
            Self::Unknown => "Unknown",
        }
    }

    pub(crate) const fn all() -> [Self; 9] {
        [
            Self::SessionSummary,
            Self::FocusScore,
            Self::GoalStreak,
            Self::FocusRisk,
            Self::WeeklyAllocation,
            Self::LastInterruption,
            Self::StatsGrowth,
            Self::Retention,
            Self::ComparisonFilters,
        ]
    }

    fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "session_summary" | "session-summary" | "sessionsummary" => Self::SessionSummary,
            "focus_score" | "focus-score" | "focusscore" => Self::FocusScore,
            "goal_streak" | "goal-streak" | "goalstreak" => Self::GoalStreak,
            "focus_risk" | "focus-risk" | "focusrisk" => Self::FocusRisk,
            "weekly_allocation" | "weekly-allocation" | "weeklyallocation" => {
                Self::WeeklyAllocation
            }
            "last_interruption" | "last-interruption" | "lastinterruption" => {
                Self::LastInterruption
            }
            "stats_growth" | "stats-growth" | "statsgrowth" => Self::StatsGrowth,
            "retention" => Self::Retention,
            "comparison_filters" | "comparison-filters" | "comparisonfilters" => {
                Self::ComparisonFilters
            }
            _ => Self::Unknown,
        }
    }
}

impl<'de> Deserialize<'de> for HistoryKpiCardId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_config_value(&value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct HistoryDashboardConfig {
    #[serde(default = "default_history_dashboard_card_order")]
    pub(crate) card_order: Vec<HistoryKpiCardId>,
    #[serde(default = "default_history_dashboard_pinned_cards")]
    pub(crate) pinned_cards: Vec<HistoryKpiCardId>,
}

impl HistoryDashboardConfig {
    pub(crate) fn normalized(&self) -> Self {
        Self::default()
    }
}

impl Default for HistoryDashboardConfig {
    fn default() -> Self {
        Self {
            card_order: default_history_dashboard_card_order(),
            pinned_cards: default_history_dashboard_pinned_cards(),
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

fn default_schedule_time_step_minutes() -> u16 {
    15
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

/// Provides the default start time for a recurring schedule window.
fn default_schedule_window_start() -> String {
    "09:00".to_string()
}

fn default_schedule_window_end() -> String {
    "10:00".to_string()
}

fn default_break_glass_duration_secs() -> u64 {
    5 * 60
}

fn default_history_dashboard_pinned_cards() -> Vec<HistoryKpiCardId> {
    HistoryKpiCardId::all().to_vec()
}

fn default_history_dashboard_card_order() -> Vec<HistoryKpiCardId> {
    HistoryKpiCardId::all().to_vec()
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub(crate) enum ProfileId {
    #[serde(rename = "basic")]
    Classic,
    #[serde(rename = "standard")]
    DeepWork,
    #[default]
    #[serde(rename = "advanced")]
    Custom,
}

impl ProfileId {
    fn from_config_value(value: &str) -> Self {
        canonical_profile_id_token(value)
            .map(profile_id_for_token)
            .unwrap_or(Self::Custom)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ProfileId::Classic => "Basic",
            ProfileId::DeepWork => "Standard",
            ProfileId::Custom => "Advanced",
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

fn canonical_profile_id_token(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "basic" | "classic" => Some("basic"),
        "standard" | "deep-work" | "deep_work" | "deepwork" => Some("standard"),
        "advanced" | "custom" => Some("advanced"),
        _ => None,
    }
}

fn profile_id_for_token(token: &str) -> ProfileId {
    match token {
        "basic" => ProfileId::Classic,
        "standard" => ProfileId::DeepWork,
        "advanced" => ProfileId::Custom,
        _ => ProfileId::Custom,
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ThemePreset {
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

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::HighContrast => "high-contrast",
            Self::DeuteranopiaFriendly => "deuteranopia-friendly",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::HighContrast => "High Contrast",
            Self::DeuteranopiaFriendly => "Deuteranopia Friendly",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Classic => Self::HighContrast,
            Self::HighContrast => Self::DeuteranopiaFriendly,
            Self::DeuteranopiaFriendly => Self::Classic,
        }
    }

    pub(crate) fn previous(self) -> Self {
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
pub(crate) struct CustomProfileConfig {
    #[serde(default = "default_focus_secs")]
    pub(crate) focus_secs: u64,
    #[serde(default = "default_short_break_secs")]
    pub(crate) short_break_secs: u64,
    #[serde(default = "default_long_break_secs")]
    pub(crate) long_break_secs: u64,
    #[serde(default = "default_long_break_interval")]
    pub(crate) long_break_interval: u32,
}

impl CustomProfileConfig {
    pub(crate) fn normalized(&self) -> Self {
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

fn default_enabled_integrations() -> Vec<String> {
    vec!["wakatime".to_string()]
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
            history_dashboard: HistoryDashboardConfig::default(),
            wakatime: WakatimeMetadataConfig::default(),
            wakatime_runtime: WakatimeRuntimeConfig::default(),
            feature_flags: FeatureFlagsConfig::default(),
            shortcuts: ShortcutConfig::default(),
        }
    }
}

impl AppConfig {
    pub(crate) fn normalized(self) -> Self {
        self.normalize()
    }

    pub(crate) fn load_with_deprecation_warnings() -> (Self, Vec<String>) {
        Self::try_load_with_deprecation_warnings().unwrap_or_else(|| (Self::default(), Vec::new()))
    }

    /// Load the config from disk, falling back to [`AppConfig::default`] on any
    /// error (missing file, parse error, corrupt data, etc.).
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn load() -> Self {
        Self::load_with_deprecation_warnings().0
    }

    /// Returns the effective custom profile configuration.
    ///
    /// If `custom_profile` is not present in the config file, this derives
    /// values from legacy duration fields to preserve user settings.
    pub(crate) fn effective_custom_profile(&self) -> CustomProfileConfig {
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

    pub(crate) fn profile_automation_for(&self, profile: ProfileId) -> ProfileAutomationConfig {
        let fallback = ProfileAutomationConfig::default();
        self.profile_automation
            .as_ref()
            .map(|settings| settings.for_profile(profile, &fallback))
            .unwrap_or(fallback)
    }

    pub(crate) fn set_profile_automation_for(
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
    pub(crate) fn save(&self) -> io::Result<()> {
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
        self.blocking_backend = self.blocking_backend.normalized();
        self.schedule_runtime = self.schedule_runtime.normalized();
        self.history_dashboard = self.history_dashboard.normalized();
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

#[cfg(test)]
mod tests;
