use crate::cli::{DEFAULT_WATCH_INTERVAL_SECS, ParsedToken};

use super::invalid_usage;

pub(in crate::cli) fn parse_watch_interval_option(
    tokens: &[ParsedToken],
) -> Result<Option<u64>, String> {
    let mut interval: Option<u64> = None;
    for token in tokens {
        if let ParsedToken::Watch(value) = token {
            if interval.is_some() {
                return Err(invalid_usage("`--watch` can only be specified once."));
            }
            interval = Some(value.unwrap_or(DEFAULT_WATCH_INTERVAL_SECS));
        }
    }
    Ok(interval)
}

pub(in crate::cli) fn parse_daemon_port_option(
    tokens: &[ParsedToken],
) -> Result<Option<u16>, String> {
    let mut port: Option<u16> = None;
    for token in tokens {
        if let ParsedToken::DaemonPort(value) = token {
            if port.is_some() {
                return Err(invalid_usage("`--daemon-port` can only be specified once."));
            }
            port = Some(*value);
        }
    }
    Ok(port)
}

pub(in crate::cli) fn parse_watch_interval_secs(value: &str) -> Result<u64, String> {
    let trimmed = value.trim();
    let secs = trimmed
        .parse::<u64>()
        .map_err(|_| invalid_usage("`--watch` requires a positive whole number of seconds."))?;
    if secs == 0 {
        return Err(invalid_usage(
            "`--watch` requires a positive whole number of seconds.",
        ));
    }
    Ok(secs)
}

pub(in crate::cli) fn parse_daemon_port(value: &str) -> Result<u16, String> {
    let trimmed = value.trim();
    let port = trimmed
        .parse::<u16>()
        .map_err(|_| invalid_usage("`--daemon-port` requires a port between 1 and 65535."))?;
    if port == 0 {
        return Err(invalid_usage(
            "`--daemon-port` requires a port between 1 and 65535.",
        ));
    }
    Ok(port)
}

pub(in crate::cli) fn require_nonempty_key_value<'a>(
    value: &'a str,
    message: &str,
) -> Result<&'a str, String> {
    if value.trim().is_empty() {
        return Err(invalid_usage(message));
    }
    Ok(value)
}
