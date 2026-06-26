use std::collections::{BTreeMap, HashSet};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::ShortcutConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ShortcutAction {
    Quit,
    TimerTogglePause,
    TimerStopReset,
    TimerNextPhase,
    OpenSiteManager,
    OpenProfileManager,
    OpenTaskSetup,
    OpenStatsHistory,
    OpenSetupDiagnostics,
    BreakGlassOverride,
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
    BackTaskSetup,
    TaskSetupAdd,
    TaskSetupRename,
    TaskSetupFavorite,
    TaskSetupArchive,
    TaskSetupDelete,
    TaskSetupSelectRecent,
    BackProfileManager,
    ProfileEdit,
    BackStatsHistory,
    ExportStatsHistory,
    HistoryDashboardSelectPrevious,
    HistoryDashboardSelectNext,
    HistoryDashboardTogglePin,
    HistoryDashboardMoveLeft,
    HistoryDashboardMoveRight,
    BackSetupDiagnostics,
    RefreshSetupDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum NavigationAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Confirm,
    Cancel,
    Delete,
    Backspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ShortcutKey {
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

const TIMER_SCOPE_ACTIONS: [ShortcutAction; 9] = [
    ShortcutAction::TimerTogglePause,
    ShortcutAction::TimerStopReset,
    ShortcutAction::TimerNextPhase,
    ShortcutAction::OpenSiteManager,
    ShortcutAction::OpenProfileManager,
    ShortcutAction::OpenTaskSetup,
    ShortcutAction::OpenStatsHistory,
    ShortcutAction::OpenSetupDiagnostics,
    ShortcutAction::BreakGlassOverride,
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
    ShortcutAction::BackTaskSetup,
    ShortcutAction::TaskSetupAdd,
    ShortcutAction::TaskSetupRename,
    ShortcutAction::TaskSetupFavorite,
    ShortcutAction::TaskSetupArchive,
    ShortcutAction::TaskSetupDelete,
    ShortcutAction::TaskSetupSelectRecent,
];

const PROFILE_MANAGER_SCOPE_ACTIONS: [ShortcutAction; 2] = [
    ShortcutAction::BackProfileManager,
    ShortcutAction::ProfileEdit,
];

const STATS_HISTORY_SCOPE_ACTIONS: [ShortcutAction; 7] = [
    ShortcutAction::BackStatsHistory,
    ShortcutAction::ExportStatsHistory,
    ShortcutAction::HistoryDashboardSelectPrevious,
    ShortcutAction::HistoryDashboardSelectNext,
    ShortcutAction::HistoryDashboardTogglePin,
    ShortcutAction::HistoryDashboardMoveLeft,
    ShortcutAction::HistoryDashboardMoveRight,
];

const SETUP_SCOPE_ACTIONS: [ShortcutAction; 2] = [
    ShortcutAction::BackSetupDiagnostics,
    ShortcutAction::RefreshSetupDiagnostics,
];

const SITE_MANAGER_NAV_ACTIONS: [NavigationAction; 4] = [
    NavigationAction::MoveDown,
    NavigationAction::MoveUp,
    NavigationAction::Cancel,
    NavigationAction::Delete,
];

const SITE_INPUT_NAV_ACTIONS: [NavigationAction; 3] = [
    NavigationAction::Confirm,
    NavigationAction::Cancel,
    NavigationAction::Backspace,
];

const SESSION_PLANNER_NAV_ACTIONS: [NavigationAction; 7] = [
    NavigationAction::MoveDown,
    NavigationAction::MoveUp,
    NavigationAction::MoveLeft,
    NavigationAction::MoveRight,
    NavigationAction::Confirm,
    NavigationAction::Cancel,
    NavigationAction::Delete,
];

const PROFILE_SELECTION_NAV_ACTIONS: [NavigationAction; 4] = [
    NavigationAction::MoveDown,
    NavigationAction::MoveUp,
    NavigationAction::Confirm,
    NavigationAction::Cancel,
];

const PROFILE_EDIT_NAV_ACTIONS: [NavigationAction; 7] = [
    NavigationAction::MoveDown,
    NavigationAction::MoveUp,
    NavigationAction::MoveLeft,
    NavigationAction::MoveRight,
    NavigationAction::Confirm,
    NavigationAction::Cancel,
    NavigationAction::Backspace,
];

const FALLBACK_SHORTCUT_KEYS: [char; 36] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'i', 'j', 'k', 'l', 'o', 'q', 'v', 'w', 'y', '1', '2', '3',
    '4', '5', '6', '7', '8', '9', '0', '[', ']', ';', ',', '.', '/', '-', '=', '`', '\\',
];

const ALL_NAVIGATION_ACTIONS: [NavigationAction; 8] = [
    NavigationAction::MoveUp,
    NavigationAction::MoveDown,
    NavigationAction::MoveLeft,
    NavigationAction::MoveRight,
    NavigationAction::Confirm,
    NavigationAction::Cancel,
    NavigationAction::Delete,
    NavigationAction::Backspace,
];

#[derive(Debug, Clone)]
pub(crate) struct ShortcutBindings {
    command_keys: BTreeMap<ShortcutAction, char>,
    navigation_keys: BTreeMap<NavigationAction, ShortcutKey>,
}

impl ShortcutBindings {
    #[cfg(test)]
    pub(crate) fn from_config(config: &ShortcutConfig) -> Self {
        Self::from_config_with_diagnostics(config).0
    }

    pub(crate) fn from_config_with_diagnostics(config: &ShortcutConfig) -> (Self, Vec<String>) {
        let mut diagnostics = Vec::new();
        let quit_key = requested_shortcut_char(config, ShortcutAction::Quit);
        let mut command_keys = BTreeMap::new();
        command_keys.insert(ShortcutAction::Quit, quit_key);

        resolve_scope(&mut command_keys, config, quit_key, &TIMER_SCOPE_ACTIONS);
        resolve_scope(
            &mut command_keys,
            config,
            quit_key,
            &SITE_MANAGER_SCOPE_ACTIONS,
        );
        resolve_scope(
            &mut command_keys,
            config,
            quit_key,
            &SESSION_PLANNER_SCOPE_ACTIONS,
        );
        resolve_scope(
            &mut command_keys,
            config,
            quit_key,
            &PROFILE_MANAGER_SCOPE_ACTIONS,
        );
        resolve_scope(
            &mut command_keys,
            config,
            quit_key,
            &STATS_HISTORY_SCOPE_ACTIONS,
        );
        resolve_scope(&mut command_keys, config, quit_key, &SETUP_SCOPE_ACTIONS);

        let mut navigation_keys = BTreeMap::new();
        for action in ALL_NAVIGATION_ACTIONS {
            navigation_keys.insert(action, requested_navigation_key(config, action));
        }

        resolve_navigation_scope(
            &mut navigation_keys,
            &command_keys,
            &SITE_MANAGER_SCOPE_ACTIONS,
            &SITE_MANAGER_NAV_ACTIONS,
            "site manager",
            &mut diagnostics,
        );
        resolve_navigation_scope(
            &mut navigation_keys,
            &command_keys,
            &SITE_MANAGER_SCOPE_ACTIONS,
            &SITE_INPUT_NAV_ACTIONS,
            "site input",
            &mut diagnostics,
        );
        resolve_navigation_scope(
            &mut navigation_keys,
            &command_keys,
            &SESSION_PLANNER_SCOPE_ACTIONS,
            &SESSION_PLANNER_NAV_ACTIONS,
            "task setup",
            &mut diagnostics,
        );
        resolve_navigation_scope(
            &mut navigation_keys,
            &command_keys,
            &SESSION_PLANNER_SCOPE_ACTIONS,
            &SITE_INPUT_NAV_ACTIONS,
            "task setup input",
            &mut diagnostics,
        );
        resolve_navigation_scope(
            &mut navigation_keys,
            &command_keys,
            &PROFILE_MANAGER_SCOPE_ACTIONS,
            &PROFILE_SELECTION_NAV_ACTIONS,
            "profile manager",
            &mut diagnostics,
        );
        resolve_navigation_scope(
            &mut navigation_keys,
            &command_keys,
            &PROFILE_MANAGER_SCOPE_ACTIONS,
            &PROFILE_EDIT_NAV_ACTIONS,
            "profile editor",
            &mut diagnostics,
        );
        resolve_navigation_scope(
            &mut navigation_keys,
            &command_keys,
            &STATS_HISTORY_SCOPE_ACTIONS,
            &[NavigationAction::Cancel],
            "history view",
            &mut diagnostics,
        );
        resolve_navigation_scope(
            &mut navigation_keys,
            &command_keys,
            &SETUP_SCOPE_ACTIONS,
            &[NavigationAction::Cancel],
            "setup diagnostics",
            &mut diagnostics,
        );

        (
            Self {
                command_keys,
                navigation_keys,
            },
            diagnostics,
        )
    }

    pub(crate) fn to_config(&self) -> ShortcutConfig {
        ShortcutConfig {
            quit: key_token(self.key(ShortcutAction::Quit)),
            timer_toggle_pause: key_token(self.key(ShortcutAction::TimerTogglePause)),
            timer_stop_reset: key_token(self.key(ShortcutAction::TimerStopReset)),
            timer_next_phase: key_token(self.key(ShortcutAction::TimerNextPhase)),
            open_site_manager: key_token(self.key(ShortcutAction::OpenSiteManager)),
            open_profile_manager: key_token(self.key(ShortcutAction::OpenProfileManager)),
            open_session_planner: key_token(self.key(ShortcutAction::OpenTaskSetup)),
            open_stats_history: key_token(self.key(ShortcutAction::OpenStatsHistory)),
            open_setup_diagnostics: key_token(self.key(ShortcutAction::OpenSetupDiagnostics)),
            break_glass_override: key_token(self.key(ShortcutAction::BreakGlassOverride)),
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
            back_session_planner: key_token(self.key(ShortcutAction::BackTaskSetup)),
            planner_add: key_token(self.key(ShortcutAction::TaskSetupAdd)),
            planner_rename: key_token(self.key(ShortcutAction::TaskSetupRename)),
            planner_favorite: key_token(self.key(ShortcutAction::TaskSetupFavorite)),
            planner_archive: key_token(self.key(ShortcutAction::TaskSetupArchive)),
            planner_delete: key_token(self.key(ShortcutAction::TaskSetupDelete)),
            planner_select_recent: key_token(self.key(ShortcutAction::TaskSetupSelectRecent)),
            back_profile_manager: key_token(self.key(ShortcutAction::BackProfileManager)),
            profile_edit: key_token(self.key(ShortcutAction::ProfileEdit)),
            back_stats_history: key_token(self.key(ShortcutAction::BackStatsHistory)),
            export_stats_history: key_token(self.key(ShortcutAction::ExportStatsHistory)),
            history_dashboard_select_previous: key_token(
                self.key(ShortcutAction::HistoryDashboardSelectPrevious),
            ),
            history_dashboard_select_next: key_token(
                self.key(ShortcutAction::HistoryDashboardSelectNext),
            ),
            history_dashboard_toggle_pin: key_token(
                self.key(ShortcutAction::HistoryDashboardTogglePin),
            ),
            history_dashboard_move_left: key_token(
                self.key(ShortcutAction::HistoryDashboardMoveLeft),
            ),
            history_dashboard_move_right: key_token(
                self.key(ShortcutAction::HistoryDashboardMoveRight),
            ),
            back_setup_diagnostics: key_token(self.key(ShortcutAction::BackSetupDiagnostics)),
            refresh_setup_diagnostics: key_token(self.key(ShortcutAction::RefreshSetupDiagnostics)),
            navigate_up: navigation_key_token(self.navigation_key(NavigationAction::MoveUp)),
            navigate_down: navigation_key_token(self.navigation_key(NavigationAction::MoveDown)),
            navigate_left: navigation_key_token(self.navigation_key(NavigationAction::MoveLeft)),
            navigate_right: navigation_key_token(self.navigation_key(NavigationAction::MoveRight)),
            confirm: navigation_key_token(self.navigation_key(NavigationAction::Confirm)),
            cancel: navigation_key_token(self.navigation_key(NavigationAction::Cancel)),
            delete: navigation_key_token(self.navigation_key(NavigationAction::Delete)),
            backspace: navigation_key_token(self.navigation_key(NavigationAction::Backspace)),
        }
    }

    pub(crate) fn matches(&self, action: ShortcutAction, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char(code) if code == self.key(action))
    }

    pub(crate) fn navigation_matches(&self, action: NavigationAction, key: &KeyEvent) -> bool {
        matches_navigation_key(self.navigation_key(action), key)
    }

    pub(crate) fn hint(&self, action: ShortcutAction) -> String {
        format!("[{}]", self.label(action))
    }

    pub(crate) fn navigation_hint(&self, action: NavigationAction) -> String {
        format!("[{}]", self.navigation_label(action))
    }

    pub(crate) fn label(&self, action: ShortcutAction) -> String {
        let key = self.key(action);
        if key == ' ' {
            "Space".to_string()
        } else {
            key.to_string()
        }
    }

    pub(crate) fn navigation_label(&self, action: NavigationAction) -> String {
        navigation_key_label(self.navigation_key(action))
    }

    fn key(&self, action: ShortcutAction) -> char {
        self.command_keys
            .get(&action)
            .copied()
            .unwrap_or_else(|| default_shortcut_char(action))
    }

    fn navigation_key(&self, action: NavigationAction) -> ShortcutKey {
        self.navigation_keys
            .get(&action)
            .copied()
            .unwrap_or_else(|| default_navigation_key(action))
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
    let scope_defaults: Vec<char> = actions
        .iter()
        .map(|action| default_shortcut_char(*action))
        .collect();

    for action in actions {
        let requested = requested_shortcut_char(config, *action);
        let default = default_shortcut_char(*action);
        let resolved = if !used.contains(&requested) {
            requested
        } else if !used.contains(&default) {
            default
        } else if let Some(scope_fallback) = scope_defaults
            .iter()
            .copied()
            .find(|key| !used.contains(key))
        {
            scope_fallback
        } else {
            fallback_shortcut_key(&used).unwrap_or(default)
        };
        keys.insert(*action, resolved);
        used.insert(resolved);
    }
}

fn resolve_navigation_scope(
    navigation_keys: &mut BTreeMap<NavigationAction, ShortcutKey>,
    command_keys: &BTreeMap<ShortcutAction, char>,
    command_actions: &[ShortcutAction],
    navigation_actions: &[NavigationAction],
    scope_name: &str,
    diagnostics: &mut Vec<String>,
) {
    let mut used = HashSet::new();
    if let Some(quit_key) = command_keys.get(&ShortcutAction::Quit).copied() {
        used.insert(ShortcutKey::Char(quit_key));
    }
    for action in command_actions {
        if let Some(command_key) = command_keys.get(action).copied() {
            used.insert(ShortcutKey::Char(command_key));
        }
    }

    for action in navigation_actions {
        let requested = navigation_keys
            .get(action)
            .copied()
            .unwrap_or_else(|| default_navigation_key(*action));
        if used.insert(requested) {
            continue;
        }

        let fallback = default_navigation_key(*action);
        if used.insert(fallback) {
            navigation_keys.insert(*action, fallback);
            diagnostics.push(format!(
                "Shortcut `{}` conflicts in {scope_name}; using default `{}`.",
                navigation_field_name(*action),
                navigation_key_token(fallback)
            ));
        }
    }
}

fn fallback_shortcut_key(used: &HashSet<char>) -> Option<char> {
    FALLBACK_SHORTCUT_KEYS
        .iter()
        .copied()
        .find(|key| !used.contains(key))
}

fn requested_shortcut_char(config: &ShortcutConfig, action: ShortcutAction) -> char {
    let requested = match action {
        ShortcutAction::Quit => &config.quit,
        ShortcutAction::TimerTogglePause => &config.timer_toggle_pause,
        ShortcutAction::TimerStopReset => &config.timer_stop_reset,
        ShortcutAction::TimerNextPhase => &config.timer_next_phase,
        ShortcutAction::OpenSiteManager => &config.open_site_manager,
        ShortcutAction::OpenProfileManager => &config.open_profile_manager,
        ShortcutAction::OpenTaskSetup => &config.open_session_planner,
        ShortcutAction::OpenStatsHistory => &config.open_stats_history,
        ShortcutAction::OpenSetupDiagnostics => &config.open_setup_diagnostics,
        ShortcutAction::BreakGlassOverride => &config.break_glass_override,
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
        ShortcutAction::BackTaskSetup => &config.back_session_planner,
        ShortcutAction::TaskSetupAdd => &config.planner_add,
        ShortcutAction::TaskSetupRename => &config.planner_rename,
        ShortcutAction::TaskSetupFavorite => &config.planner_favorite,
        ShortcutAction::TaskSetupArchive => &config.planner_archive,
        ShortcutAction::TaskSetupDelete => &config.planner_delete,
        ShortcutAction::TaskSetupSelectRecent => &config.planner_select_recent,
        ShortcutAction::BackProfileManager => &config.back_profile_manager,
        ShortcutAction::ProfileEdit => &config.profile_edit,
        ShortcutAction::BackStatsHistory => &config.back_stats_history,
        ShortcutAction::ExportStatsHistory => &config.export_stats_history,
        ShortcutAction::HistoryDashboardSelectPrevious => &config.history_dashboard_select_previous,
        ShortcutAction::HistoryDashboardSelectNext => &config.history_dashboard_select_next,
        ShortcutAction::HistoryDashboardTogglePin => &config.history_dashboard_toggle_pin,
        ShortcutAction::HistoryDashboardMoveLeft => &config.history_dashboard_move_left,
        ShortcutAction::HistoryDashboardMoveRight => &config.history_dashboard_move_right,
        ShortcutAction::BackSetupDiagnostics => &config.back_setup_diagnostics,
        ShortcutAction::RefreshSetupDiagnostics => &config.refresh_setup_diagnostics,
    };
    parse_shortcut_char(requested).unwrap_or_else(|| default_shortcut_char(action))
}

fn requested_navigation_key(config: &ShortcutConfig, action: NavigationAction) -> ShortcutKey {
    let requested = match action {
        NavigationAction::MoveUp => &config.navigate_up,
        NavigationAction::MoveDown => &config.navigate_down,
        NavigationAction::MoveLeft => &config.navigate_left,
        NavigationAction::MoveRight => &config.navigate_right,
        NavigationAction::Confirm => &config.confirm,
        NavigationAction::Cancel => &config.cancel,
        NavigationAction::Delete => &config.delete,
        NavigationAction::Backspace => &config.backspace,
    };
    parse_shortcut_key(requested).unwrap_or_else(|| default_navigation_key(action))
}

fn default_shortcut_char(action: ShortcutAction) -> char {
    match action {
        ShortcutAction::Quit => 'q',
        ShortcutAction::TimerTogglePause => ' ',
        ShortcutAction::TimerStopReset => 's',
        ShortcutAction::TimerNextPhase => 'n',
        ShortcutAction::OpenSiteManager => 'b',
        ShortcutAction::OpenProfileManager => 'p',
        ShortcutAction::OpenTaskSetup => 't',
        ShortcutAction::OpenStatsHistory => 'h',
        ShortcutAction::OpenSetupDiagnostics => 'd',
        ShortcutAction::BreakGlassOverride => 'u',
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
        ShortcutAction::BackTaskSetup => 't',
        ShortcutAction::TaskSetupAdd => 'a',
        ShortcutAction::TaskSetupRename => 'e',
        ShortcutAction::TaskSetupFavorite => 'f',
        ShortcutAction::TaskSetupArchive => 'x',
        ShortcutAction::TaskSetupDelete => 'd',
        ShortcutAction::TaskSetupSelectRecent => 'r',
        ShortcutAction::BackProfileManager => 'p',
        ShortcutAction::ProfileEdit => 'e',
        ShortcutAction::BackStatsHistory => 'h',
        ShortcutAction::ExportStatsHistory => 'e',
        ShortcutAction::HistoryDashboardSelectPrevious => 'k',
        ShortcutAction::HistoryDashboardSelectNext => 'j',
        ShortcutAction::HistoryDashboardTogglePin => 'p',
        ShortcutAction::HistoryDashboardMoveLeft => '<',
        ShortcutAction::HistoryDashboardMoveRight => '>',
        ShortcutAction::BackSetupDiagnostics => 'd',
        ShortcutAction::RefreshSetupDiagnostics => 'r',
    }
}

fn default_navigation_key(action: NavigationAction) -> ShortcutKey {
    match action {
        NavigationAction::MoveUp => ShortcutKey::Up,
        NavigationAction::MoveDown => ShortcutKey::Down,
        NavigationAction::MoveLeft => ShortcutKey::Left,
        NavigationAction::MoveRight => ShortcutKey::Right,
        NavigationAction::Confirm => ShortcutKey::Enter,
        NavigationAction::Cancel => ShortcutKey::Esc,
        NavigationAction::Delete => ShortcutKey::Delete,
        NavigationAction::Backspace => ShortcutKey::Backspace,
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

fn parse_shortcut_key(value: &str) -> Option<ShortcutKey> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return None;
    }

    match normalized.to_ascii_lowercase().as_str() {
        "enter" => Some(ShortcutKey::Enter),
        "esc" | "escape" => Some(ShortcutKey::Esc),
        "up" => Some(ShortcutKey::Up),
        "down" => Some(ShortcutKey::Down),
        "left" => Some(ShortcutKey::Left),
        "right" => Some(ShortcutKey::Right),
        "backspace" => Some(ShortcutKey::Backspace),
        "delete" | "del" => Some(ShortcutKey::Delete),
        _ => parse_shortcut_char(normalized).map(ShortcutKey::Char),
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

fn navigation_key_token(value: ShortcutKey) -> String {
    match value {
        ShortcutKey::Char(c) => key_token(c),
        ShortcutKey::Enter => "enter".to_string(),
        ShortcutKey::Esc => "esc".to_string(),
        ShortcutKey::Up => "up".to_string(),
        ShortcutKey::Down => "down".to_string(),
        ShortcutKey::Left => "left".to_string(),
        ShortcutKey::Right => "right".to_string(),
        ShortcutKey::Backspace => "backspace".to_string(),
        ShortcutKey::Delete => "delete".to_string(),
    }
}

fn navigation_key_label(value: ShortcutKey) -> String {
    match value {
        ShortcutKey::Char(' ') => "Space".to_string(),
        ShortcutKey::Char(c) => c.to_string(),
        ShortcutKey::Enter => "Enter".to_string(),
        ShortcutKey::Esc => "Esc".to_string(),
        ShortcutKey::Up => "↑".to_string(),
        ShortcutKey::Down => "↓".to_string(),
        ShortcutKey::Left => "←".to_string(),
        ShortcutKey::Right => "→".to_string(),
        ShortcutKey::Backspace => "Backspace".to_string(),
        ShortcutKey::Delete => "Del".to_string(),
    }
}

fn navigation_field_name(action: NavigationAction) -> &'static str {
    match action {
        NavigationAction::MoveUp => "navigate_up",
        NavigationAction::MoveDown => "navigate_down",
        NavigationAction::MoveLeft => "navigate_left",
        NavigationAction::MoveRight => "navigate_right",
        NavigationAction::Confirm => "confirm",
        NavigationAction::Cancel => "cancel",
        NavigationAction::Delete => "delete",
        NavigationAction::Backspace => "backspace",
    }
}

fn matches_navigation_key(binding: ShortcutKey, key: &KeyEvent) -> bool {
    match binding {
        ShortcutKey::Char(expected) => match key.code {
            KeyCode::Char(actual)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let actual = if actual.is_ascii_alphabetic() {
                    actual.to_ascii_lowercase()
                } else {
                    actual
                };
                actual == expected
            }
            _ => false,
        },
        ShortcutKey::Enter => key.code == KeyCode::Enter,
        ShortcutKey::Esc => key.code == KeyCode::Esc,
        ShortcutKey::Up => key.code == KeyCode::Up,
        ShortcutKey::Down => key.code == KeyCode::Down,
        ShortcutKey::Left => key.code == KeyCode::Left,
        ShortcutKey::Right => key.code == KeyCode::Right,
        ShortcutKey::Backspace => key.code == KeyCode::Backspace,
        ShortcutKey::Delete => key.code == KeyCode::Delete,
    }
}

#[cfg(test)]
mod tests {
    use super::{NavigationAction, ShortcutAction, ShortcutBindings};
    use crate::config::ShortcutConfig;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn resolve_scope_keeps_quit_unique_when_quit_is_c() {
        let bindings = ShortcutBindings::from_config(&ShortcutConfig {
            quit: "c".to_string(),
            timer_toggle_pause: "c".to_string(),
            timer_stop_reset: "c".to_string(),
            ..ShortcutConfig::default()
        });

        assert!(bindings.matches(
            ShortcutAction::Quit,
            &KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)
        ));
        assert!(!bindings.matches(
            ShortcutAction::TimerTogglePause,
            &KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)
        ));
        assert!(!bindings.matches(
            ShortcutAction::TimerStopReset,
            &KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)
        ));
    }

    #[test]
    fn resolve_scope_avoids_duplicates_when_requested_and_default_collide() {
        let bindings = ShortcutBindings::from_config(&ShortcutConfig {
            quit: "a".to_string(),
            timer_stop_reset: "a".to_string(),
            timer_next_phase: "s".to_string(),
            ..ShortcutConfig::default()
        });

        assert!(!bindings.matches(
            ShortcutAction::TimerStopReset,
            &KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)
        ));
        assert!(!bindings.matches(
            ShortcutAction::TimerNextPhase,
            &KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)
        ));
    }

    #[test]
    fn navigation_named_tokens_match_expected_special_keys() {
        let bindings = ShortcutBindings::from_config(&ShortcutConfig {
            navigate_down: "down".to_string(),
            confirm: "enter".to_string(),
            cancel: "esc".to_string(),
            delete: "delete".to_string(),
            backspace: "backspace".to_string(),
            ..ShortcutConfig::default()
        });

        assert!(bindings.navigation_matches(
            NavigationAction::MoveDown,
            &KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
        ));
        assert!(bindings.navigation_matches(
            NavigationAction::Confirm,
            &KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ));
        assert!(bindings.navigation_matches(
            NavigationAction::Cancel,
            &KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
        ));
        assert!(bindings.navigation_matches(
            NavigationAction::Delete,
            &KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)
        ));
        assert!(bindings.navigation_matches(
            NavigationAction::Backspace,
            &KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)
        ));
    }

    #[test]
    fn conflicting_navigation_binding_falls_back_with_diagnostic() {
        let (bindings, diagnostics) =
            ShortcutBindings::from_config_with_diagnostics(&ShortcutConfig {
                planner_add: "j".to_string(),
                navigate_down: "j".to_string(),
                ..ShortcutConfig::default()
            });

        assert!(
            diagnostics
                .iter()
                .any(|message| message.contains("navigate_down"))
        );
        assert!(bindings.navigation_matches(
            NavigationAction::MoveDown,
            &KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
        ));
        assert!(!bindings.navigation_matches(
            NavigationAction::MoveDown,
            &KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)
        ));
    }
}
