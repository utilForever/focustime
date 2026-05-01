use super::*;

pub(super) fn app_dir_with_env(get_var: impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    let config_dir = config_dir_from_env(get_var)?;
    Some(config_dir.join("focustime"))
}

pub(super) fn config_dir_from_env(
    mut get_var: impl FnMut(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env_path_from_value(get_var("APPDATA")?)
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Honour XDG_CONFIG_HOME if set, otherwise fall back to ~/.config.
        if let Some(xdg) = get_var("XDG_CONFIG_HOME").and_then(env_path_from_value) {
            return Some(xdg);
        }
        let home = get_var("HOME").and_then(env_path_from_value)?;
        Some(home.join(".config"))
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
