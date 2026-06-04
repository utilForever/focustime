use serde::{Deserialize, Serialize};

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
    #[serde(default = "default_shortcut_back_stats_history")]
    pub back_stats_history: String,
    #[serde(default = "default_shortcut_export_stats_history")]
    pub export_stats_history: String,
    #[serde(default = "default_shortcut_history_dashboard_select_previous")]
    pub history_dashboard_select_previous: String,
    #[serde(default = "default_shortcut_history_dashboard_select_next")]
    pub history_dashboard_select_next: String,
    #[serde(default = "default_shortcut_history_dashboard_toggle_pin")]
    pub history_dashboard_toggle_pin: String,
    #[serde(default = "default_shortcut_history_dashboard_move_left")]
    pub history_dashboard_move_left: String,
    #[serde(default = "default_shortcut_history_dashboard_move_right")]
    pub history_dashboard_move_right: String,
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
            back_stats_history: normalize_shortcut_token(
                &self.back_stats_history,
                &default_shortcut_back_stats_history(),
            ),
            export_stats_history: normalize_shortcut_token(
                &self.export_stats_history,
                &default_shortcut_export_stats_history(),
            ),
            history_dashboard_select_previous: normalize_shortcut_token(
                &self.history_dashboard_select_previous,
                &default_shortcut_history_dashboard_select_previous(),
            ),
            history_dashboard_select_next: normalize_shortcut_token(
                &self.history_dashboard_select_next,
                &default_shortcut_history_dashboard_select_next(),
            ),
            history_dashboard_toggle_pin: normalize_shortcut_token(
                &self.history_dashboard_toggle_pin,
                &default_shortcut_history_dashboard_toggle_pin(),
            ),
            history_dashboard_move_left: normalize_shortcut_token(
                &self.history_dashboard_move_left,
                &default_shortcut_history_dashboard_move_left(),
            ),
            history_dashboard_move_right: normalize_shortcut_token(
                &self.history_dashboard_move_right,
                &default_shortcut_history_dashboard_move_right(),
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
            back_stats_history: default_shortcut_back_stats_history(),
            export_stats_history: default_shortcut_export_stats_history(),
            history_dashboard_select_previous: default_shortcut_history_dashboard_select_previous(),
            history_dashboard_select_next: default_shortcut_history_dashboard_select_next(),
            history_dashboard_toggle_pin: default_shortcut_history_dashboard_toggle_pin(),
            history_dashboard_move_left: default_shortcut_history_dashboard_move_left(),
            history_dashboard_move_right: default_shortcut_history_dashboard_move_right(),
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

fn default_shortcut_back_stats_history() -> String {
    "h".to_string()
}

fn default_shortcut_export_stats_history() -> String {
    "e".to_string()
}

fn default_shortcut_history_dashboard_select_previous() -> String {
    "k".to_string()
}

fn default_shortcut_history_dashboard_select_next() -> String {
    "j".to_string()
}

fn default_shortcut_history_dashboard_toggle_pin() -> String {
    "p".to_string()
}

fn default_shortcut_history_dashboard_move_left() -> String {
    "<".to_string()
}

fn default_shortcut_history_dashboard_move_right() -> String {
    ">".to_string()
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
