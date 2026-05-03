use std::collections::{BTreeMap, HashSet};

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::ShortcutConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShortcutAction {
    Quit,
    TimerTogglePause,
    TimerStopReset,
    TimerNextPhase,
    OpenSiteManager,
    OpenProfileManager,
    OpenSessionPlanner,
    OpenStatsHistory,
    OpenSetupDiagnostics,
    TimerEditNote,
    BreakGlassOverride,
    DelayScheduleStart,
    BackSiteManager,
    ToggleSiteListMode,
    SiteAdd,
    SiteEdit,
    SiteDelete,
    SelectPreviousBlocklistProfile,
    SelectNextBlocklistProfile,
    CreateBlocklistProfile,
    RenameBlocklistProfile,
    DeleteBlocklistProfile,
    BackSessionPlanner,
    PlannerAdd,
    PlannerRename,
    PlannerFavorite,
    PlannerArchive,
    PlannerDelete,
    PlannerSelectRecent,
    BackProfileManager,
    ProfileEdit,
    SelectPreviousBreakTemplate,
    SelectNextBreakTemplate,
    BackStatsHistory,
    ExportStatsHistory,
    BackSetupDiagnostics,
    RefreshSetupDiagnostics,
}

const TIMER_SCOPE_ACTIONS: [ShortcutAction; 11] = [
    ShortcutAction::TimerTogglePause,
    ShortcutAction::TimerStopReset,
    ShortcutAction::TimerNextPhase,
    ShortcutAction::OpenSiteManager,
    ShortcutAction::OpenProfileManager,
    ShortcutAction::OpenSessionPlanner,
    ShortcutAction::OpenStatsHistory,
    ShortcutAction::OpenSetupDiagnostics,
    ShortcutAction::TimerEditNote,
    ShortcutAction::BreakGlassOverride,
    ShortcutAction::DelayScheduleStart,
];

const SITE_MANAGER_SCOPE_ACTIONS: [ShortcutAction; 10] = [
    ShortcutAction::BackSiteManager,
    ShortcutAction::ToggleSiteListMode,
    ShortcutAction::SiteAdd,
    ShortcutAction::SiteEdit,
    ShortcutAction::SiteDelete,
    ShortcutAction::SelectPreviousBlocklistProfile,
    ShortcutAction::SelectNextBlocklistProfile,
    ShortcutAction::CreateBlocklistProfile,
    ShortcutAction::RenameBlocklistProfile,
    ShortcutAction::DeleteBlocklistProfile,
];

const SESSION_PLANNER_SCOPE_ACTIONS: [ShortcutAction; 7] = [
    ShortcutAction::BackSessionPlanner,
    ShortcutAction::PlannerAdd,
    ShortcutAction::PlannerRename,
    ShortcutAction::PlannerFavorite,
    ShortcutAction::PlannerArchive,
    ShortcutAction::PlannerDelete,
    ShortcutAction::PlannerSelectRecent,
];

const PROFILE_MANAGER_SCOPE_ACTIONS: [ShortcutAction; 4] = [
    ShortcutAction::BackProfileManager,
    ShortcutAction::ProfileEdit,
    ShortcutAction::SelectPreviousBreakTemplate,
    ShortcutAction::SelectNextBreakTemplate,
];

const STATS_HISTORY_SCOPE_ACTIONS: [ShortcutAction; 2] = [
    ShortcutAction::BackStatsHistory,
    ShortcutAction::ExportStatsHistory,
];

const SETUP_SCOPE_ACTIONS: [ShortcutAction; 2] = [
    ShortcutAction::BackSetupDiagnostics,
    ShortcutAction::RefreshSetupDiagnostics,
];

#[derive(Debug, Clone)]
pub struct ShortcutBindings {
    keys: BTreeMap<ShortcutAction, char>,
}

impl ShortcutBindings {
    pub fn from_config(config: &ShortcutConfig) -> Self {
        let quit_key = requested_shortcut_char(config, ShortcutAction::Quit);
        let mut keys = BTreeMap::new();
        keys.insert(ShortcutAction::Quit, quit_key);

        resolve_scope(&mut keys, config, quit_key, &TIMER_SCOPE_ACTIONS);
        resolve_scope(&mut keys, config, quit_key, &SITE_MANAGER_SCOPE_ACTIONS);
        resolve_scope(&mut keys, config, quit_key, &SESSION_PLANNER_SCOPE_ACTIONS);
        resolve_scope(&mut keys, config, quit_key, &PROFILE_MANAGER_SCOPE_ACTIONS);
        resolve_scope(&mut keys, config, quit_key, &STATS_HISTORY_SCOPE_ACTIONS);
        resolve_scope(&mut keys, config, quit_key, &SETUP_SCOPE_ACTIONS);

        Self { keys }
    }

    pub fn to_config(&self) -> ShortcutConfig {
        ShortcutConfig {
            quit: key_token(self.key(ShortcutAction::Quit)),
            timer_toggle_pause: key_token(self.key(ShortcutAction::TimerTogglePause)),
            timer_stop_reset: key_token(self.key(ShortcutAction::TimerStopReset)),
            timer_next_phase: key_token(self.key(ShortcutAction::TimerNextPhase)),
            open_site_manager: key_token(self.key(ShortcutAction::OpenSiteManager)),
            open_profile_manager: key_token(self.key(ShortcutAction::OpenProfileManager)),
            open_session_planner: key_token(self.key(ShortcutAction::OpenSessionPlanner)),
            open_stats_history: key_token(self.key(ShortcutAction::OpenStatsHistory)),
            open_setup_diagnostics: key_token(self.key(ShortcutAction::OpenSetupDiagnostics)),
            timer_edit_note: key_token(self.key(ShortcutAction::TimerEditNote)),
            break_glass_override: key_token(self.key(ShortcutAction::BreakGlassOverride)),
            delay_schedule_start: key_token(self.key(ShortcutAction::DelayScheduleStart)),
            back_site_manager: key_token(self.key(ShortcutAction::BackSiteManager)),
            toggle_site_list_mode: key_token(self.key(ShortcutAction::ToggleSiteListMode)),
            site_add: key_token(self.key(ShortcutAction::SiteAdd)),
            site_edit: key_token(self.key(ShortcutAction::SiteEdit)),
            site_delete: key_token(self.key(ShortcutAction::SiteDelete)),
            select_previous_blocklist_profile: key_token(
                self.key(ShortcutAction::SelectPreviousBlocklistProfile),
            ),
            select_next_blocklist_profile: key_token(
                self.key(ShortcutAction::SelectNextBlocklistProfile),
            ),
            create_blocklist_profile: key_token(self.key(ShortcutAction::CreateBlocklistProfile)),
            rename_blocklist_profile: key_token(self.key(ShortcutAction::RenameBlocklistProfile)),
            delete_blocklist_profile: key_token(self.key(ShortcutAction::DeleteBlocklistProfile)),
            back_session_planner: key_token(self.key(ShortcutAction::BackSessionPlanner)),
            planner_add: key_token(self.key(ShortcutAction::PlannerAdd)),
            planner_rename: key_token(self.key(ShortcutAction::PlannerRename)),
            planner_favorite: key_token(self.key(ShortcutAction::PlannerFavorite)),
            planner_archive: key_token(self.key(ShortcutAction::PlannerArchive)),
            planner_delete: key_token(self.key(ShortcutAction::PlannerDelete)),
            planner_select_recent: key_token(self.key(ShortcutAction::PlannerSelectRecent)),
            back_profile_manager: key_token(self.key(ShortcutAction::BackProfileManager)),
            profile_edit: key_token(self.key(ShortcutAction::ProfileEdit)),
            select_previous_break_template: key_token(
                self.key(ShortcutAction::SelectPreviousBreakTemplate),
            ),
            select_next_break_template: key_token(
                self.key(ShortcutAction::SelectNextBreakTemplate),
            ),
            back_stats_history: key_token(self.key(ShortcutAction::BackStatsHistory)),
            export_stats_history: key_token(self.key(ShortcutAction::ExportStatsHistory)),
            back_setup_diagnostics: key_token(self.key(ShortcutAction::BackSetupDiagnostics)),
            refresh_setup_diagnostics: key_token(self.key(ShortcutAction::RefreshSetupDiagnostics)),
        }
    }

    pub fn matches(&self, action: ShortcutAction, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char(code) if code == self.key(action))
    }

    pub fn hint(&self, action: ShortcutAction) -> String {
        format!("[{}]", self.label(action))
    }

    pub fn label(&self, action: ShortcutAction) -> String {
        let key = self.key(action);
        if key == ' ' {
            "Space".to_string()
        } else {
            key.to_string()
        }
    }

    fn key(&self, action: ShortcutAction) -> char {
        self.keys
            .get(&action)
            .copied()
            .unwrap_or_else(|| default_shortcut_char(action))
    }
}

fn resolve_scope(
    keys: &mut BTreeMap<ShortcutAction, char>,
    config: &ShortcutConfig,
    quit_key: char,
    actions: &[ShortcutAction],
) {
    let mut used = HashSet::new();
    used.insert(quit_key);

    for action in actions {
        let requested = requested_shortcut_char(config, *action);
        let default = default_shortcut_char(*action);
        let resolved = if !used.contains(&requested) {
            requested
        } else if !used.contains(&default) {
            default
        } else {
            requested
        };
        keys.insert(*action, resolved);
        used.insert(resolved);
    }
}

fn requested_shortcut_char(config: &ShortcutConfig, action: ShortcutAction) -> char {
    let requested = match action {
        ShortcutAction::Quit => &config.quit,
        ShortcutAction::TimerTogglePause => &config.timer_toggle_pause,
        ShortcutAction::TimerStopReset => &config.timer_stop_reset,
        ShortcutAction::TimerNextPhase => &config.timer_next_phase,
        ShortcutAction::OpenSiteManager => &config.open_site_manager,
        ShortcutAction::OpenProfileManager => &config.open_profile_manager,
        ShortcutAction::OpenSessionPlanner => &config.open_session_planner,
        ShortcutAction::OpenStatsHistory => &config.open_stats_history,
        ShortcutAction::OpenSetupDiagnostics => &config.open_setup_diagnostics,
        ShortcutAction::TimerEditNote => &config.timer_edit_note,
        ShortcutAction::BreakGlassOverride => &config.break_glass_override,
        ShortcutAction::DelayScheduleStart => &config.delay_schedule_start,
        ShortcutAction::BackSiteManager => &config.back_site_manager,
        ShortcutAction::ToggleSiteListMode => &config.toggle_site_list_mode,
        ShortcutAction::SiteAdd => &config.site_add,
        ShortcutAction::SiteEdit => &config.site_edit,
        ShortcutAction::SiteDelete => &config.site_delete,
        ShortcutAction::SelectPreviousBlocklistProfile => &config.select_previous_blocklist_profile,
        ShortcutAction::SelectNextBlocklistProfile => &config.select_next_blocklist_profile,
        ShortcutAction::CreateBlocklistProfile => &config.create_blocklist_profile,
        ShortcutAction::RenameBlocklistProfile => &config.rename_blocklist_profile,
        ShortcutAction::DeleteBlocklistProfile => &config.delete_blocklist_profile,
        ShortcutAction::BackSessionPlanner => &config.back_session_planner,
        ShortcutAction::PlannerAdd => &config.planner_add,
        ShortcutAction::PlannerRename => &config.planner_rename,
        ShortcutAction::PlannerFavorite => &config.planner_favorite,
        ShortcutAction::PlannerArchive => &config.planner_archive,
        ShortcutAction::PlannerDelete => &config.planner_delete,
        ShortcutAction::PlannerSelectRecent => &config.planner_select_recent,
        ShortcutAction::BackProfileManager => &config.back_profile_manager,
        ShortcutAction::ProfileEdit => &config.profile_edit,
        ShortcutAction::SelectPreviousBreakTemplate => &config.select_previous_break_template,
        ShortcutAction::SelectNextBreakTemplate => &config.select_next_break_template,
        ShortcutAction::BackStatsHistory => &config.back_stats_history,
        ShortcutAction::ExportStatsHistory => &config.export_stats_history,
        ShortcutAction::BackSetupDiagnostics => &config.back_setup_diagnostics,
        ShortcutAction::RefreshSetupDiagnostics => &config.refresh_setup_diagnostics,
    };
    parse_shortcut_char(requested).unwrap_or_else(|| default_shortcut_char(action))
}

fn default_shortcut_char(action: ShortcutAction) -> char {
    match action {
        ShortcutAction::Quit => 'q',
        ShortcutAction::TimerTogglePause => ' ',
        ShortcutAction::TimerStopReset => 's',
        ShortcutAction::TimerNextPhase => 'n',
        ShortcutAction::OpenSiteManager => 'b',
        ShortcutAction::OpenProfileManager => 'p',
        ShortcutAction::OpenSessionPlanner => 't',
        ShortcutAction::OpenStatsHistory => 'h',
        ShortcutAction::OpenSetupDiagnostics => 'd',
        ShortcutAction::TimerEditNote => 'm',
        ShortcutAction::BreakGlassOverride => 'u',
        ShortcutAction::DelayScheduleStart => 'z',
        ShortcutAction::BackSiteManager => 'b',
        ShortcutAction::ToggleSiteListMode => 'm',
        ShortcutAction::SiteAdd => 'a',
        ShortcutAction::SiteEdit => 'e',
        ShortcutAction::SiteDelete => 'd',
        ShortcutAction::SelectPreviousBlocklistProfile => '[',
        ShortcutAction::SelectNextBlocklistProfile => ']',
        ShortcutAction::CreateBlocklistProfile => 'n',
        ShortcutAction::RenameBlocklistProfile => 'r',
        ShortcutAction::DeleteBlocklistProfile => 'x',
        ShortcutAction::BackSessionPlanner => 't',
        ShortcutAction::PlannerAdd => 'a',
        ShortcutAction::PlannerRename => 'e',
        ShortcutAction::PlannerFavorite => 'f',
        ShortcutAction::PlannerArchive => 'x',
        ShortcutAction::PlannerDelete => 'd',
        ShortcutAction::PlannerSelectRecent => 'r',
        ShortcutAction::BackProfileManager => 'p',
        ShortcutAction::ProfileEdit => 'e',
        ShortcutAction::SelectPreviousBreakTemplate => '[',
        ShortcutAction::SelectNextBreakTemplate => ']',
        ShortcutAction::BackStatsHistory => 'h',
        ShortcutAction::ExportStatsHistory => 'e',
        ShortcutAction::BackSetupDiagnostics => 'd',
        ShortcutAction::RefreshSetupDiagnostics => 'r',
    }
}

fn parse_shortcut_char(value: &str) -> Option<char> {
    let normalized = value.trim();
    if normalized.eq_ignore_ascii_case("space") {
        return Some(' ');
    }

    let mut chars = normalized.chars();
    let key = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    if key.is_ascii_alphabetic() {
        Some(key.to_ascii_lowercase())
    } else {
        Some(key)
    }
}

fn key_token(value: char) -> String {
    if value == ' ' {
        "space".to_string()
    } else if value.is_ascii_alphabetic() {
        value.to_ascii_lowercase().to_string()
    } else {
        value.to_string()
    }
}
