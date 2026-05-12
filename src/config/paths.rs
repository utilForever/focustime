use crate::config::{OsString, PathBuf};

pub(super) fn app_dir_with_env(get_var: impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    let config_dir = config_dir_from_env(get_var)?;
    Some(config_dir.join("focustime"))
}

pub(super) fn stats_app_dir_with_env(
    get_var: impl FnMut(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    let state_dir = stats_state_dir_from_env(get_var)?;
    Some(state_dir.join("focustime"))
}

pub(super) fn config_dir_from_env(
    mut get_var: impl FnMut(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let appdata = env_path_from_value(get_var("APPDATA")?)?;
        if appdata.is_absolute() {
            Some(appdata)
        } else {
            None
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Honour XDG_CONFIG_HOME if set, otherwise fall back to ~/.config.
        if let Some(xdg) = get_var("XDG_CONFIG_HOME").and_then(env_path_from_value) {
            if xdg.is_absolute() {
                return Some(xdg);
            }
        }
        let home = get_var("HOME").and_then(env_path_from_value)?;
        if !home.is_absolute() {
            return None;
        }
        Some(home.join(".config"))
    }
}

pub(super) fn stats_state_dir_from_env(
    mut get_var: impl FnMut(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = get_var("LOCALAPPDATA").and_then(env_path_from_value)
            && local_app_data.is_absolute()
        {
            return Some(local_app_data);
        }
        let appdata = env_path_from_value(get_var("APPDATA")?)?;
        if appdata.is_absolute() {
            Some(appdata)
        } else {
            None
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Honour XDG_STATE_HOME first, then XDG_DATA_HOME, then ~/.local/state.
        if let Some(xdg_state) = get_var("XDG_STATE_HOME").and_then(env_path_from_value) {
            if xdg_state.is_absolute() {
                return Some(xdg_state);
            }
        }
        if let Some(xdg_data) = get_var("XDG_DATA_HOME").and_then(env_path_from_value) {
            if xdg_data.is_absolute() {
                return Some(xdg_data);
            }
        }
        let home = get_var("HOME").and_then(env_path_from_value)?;
        if !home.is_absolute() {
            return None;
        }
        Some(home.join(".local").join("state"))
    }
}

pub(super) fn env_path_from_value(value: OsString) -> Option<PathBuf> {
    if value.is_empty() {
        return None;
    }
    if let Some(value_utf8) = value.to_str()
        && value_utf8.trim().is_empty()
    {
        return None;
    }
    Some(PathBuf::from(value))
}
