use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::blocker::SiteBlocker;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TemporaryAllowlistEntry {
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub site: String,
    #[serde(default)]
    pub expires_at_epoch_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTemporaryAllowlistSpec {
    pub site: String,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActiveTemporaryAllowlistEntry {
    pub site: String,
    pub remaining_secs: u64,
    pub expires_at_epoch_secs: i64,
}

pub fn parse_temporary_allowlist_specs(
    input: &str,
) -> Result<Vec<ParsedTemporaryAllowlistSpec>, String> {
    let mut parsed: Vec<ParsedTemporaryAllowlistSpec> = Vec::new();
    let mut site_index_by_key: HashMap<String, usize> = HashMap::new();

    for raw_token in input.split([',', '\n', '\r']) {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let (site_token, duration_token) = token.rsplit_once('=').ok_or_else(|| {
            format!(
                "Invalid temporary allowlist entry `{}`. Use `HOST=30m` format.",
                display_input_value(token)
            )
        })?;
        let site = normalize_single_site(site_token)?;
        let duration_secs = parse_duration_secs(duration_token).map_err(|reason| {
            format!(
                "Invalid duration in `{}` ({reason}).",
                display_input_value(token)
            )
        })?;

        let key = site.to_ascii_lowercase();
        if let Some(index) = site_index_by_key.get(&key).copied() {
            parsed[index].duration_secs = duration_secs;
        } else {
            site_index_by_key.insert(key, parsed.len());
            parsed.push(ParsedTemporaryAllowlistSpec {
                site,
                duration_secs,
            });
        }
    }

    if parsed.is_empty() {
        return Err(
            "No temporary allowlist entries were provided. Use `HOST=30m` tokens.".to_string(),
        );
    }

    Ok(parsed)
}

pub fn prune_expired_temporary_allowlist_entries(
    entries: &mut Vec<TemporaryAllowlistEntry>,
    now_epoch_secs: i64,
) -> usize {
    let original_len = entries.len();
    entries.retain(|entry| {
        !entry.profile.trim().is_empty()
            && !entry.site.trim().is_empty()
            && entry.expires_at_epoch_secs > now_epoch_secs
    });
    original_len.saturating_sub(entries.len())
}

pub fn upsert_temporary_allowlist_entries(
    entries: &mut Vec<TemporaryAllowlistEntry>,
    profile: &str,
    specs: &[ParsedTemporaryAllowlistSpec],
    now_epoch_secs: i64,
) -> (usize, usize) {
    let profile = profile.trim();
    if profile.is_empty() || specs.is_empty() {
        return (0, 0);
    }

    let mut added = 0usize;
    let mut refreshed = 0usize;
    for spec in specs {
        let expires_at_epoch_secs = now_epoch_secs
            .saturating_add(i64::try_from(spec.duration_secs).unwrap_or(i64::MAX).max(1));

        if let Some(existing) = entries.iter_mut().find(|entry| {
            entry.profile.eq_ignore_ascii_case(profile)
                && entry.site.eq_ignore_ascii_case(&spec.site)
        }) {
            existing.profile = profile.to_string();
            existing.site = spec.site.clone();
            if existing.expires_at_epoch_secs != expires_at_epoch_secs {
                existing.expires_at_epoch_secs = expires_at_epoch_secs;
                refreshed += 1;
            }
            continue;
        }

        entries.push(TemporaryAllowlistEntry {
            profile: profile.to_string(),
            site: spec.site.clone(),
            expires_at_epoch_secs,
        });
        added += 1;
    }

    entries.sort_by(|left, right| {
        left.profile
            .to_ascii_lowercase()
            .cmp(&right.profile.to_ascii_lowercase())
            .then(
                left.site
                    .to_ascii_lowercase()
                    .cmp(&right.site.to_ascii_lowercase()),
            )
            .then(left.expires_at_epoch_secs.cmp(&right.expires_at_epoch_secs))
    });

    (added, refreshed)
}

pub fn active_temporary_allowlist_sites_for_profile(
    entries: &[TemporaryAllowlistEntry],
    profile: &str,
    now_epoch_secs: i64,
) -> Vec<String> {
    active_temporary_allowlist_status_entries_for_profile(entries, profile, now_epoch_secs)
        .into_iter()
        .map(|entry| entry.site)
        .collect()
}

pub fn active_temporary_allowlist_status_entries_for_profile(
    entries: &[TemporaryAllowlistEntry],
    profile: &str,
    now_epoch_secs: i64,
) -> Vec<ActiveTemporaryAllowlistEntry> {
    let mut latest_by_site: HashMap<String, ActiveTemporaryAllowlistEntry> = HashMap::new();

    for entry in entries {
        let profile_name = entry.profile.trim();
        let site = entry.site.trim();
        if profile_name.is_empty()
            || site.is_empty()
            || entry.expires_at_epoch_secs <= now_epoch_secs
            || !profile_name.eq_ignore_ascii_case(profile)
        {
            continue;
        }

        let remaining_secs = (entry.expires_at_epoch_secs - now_epoch_secs) as u64;
        let key = site.to_ascii_lowercase();
        match latest_by_site.get(&key) {
            Some(existing) if existing.expires_at_epoch_secs >= entry.expires_at_epoch_secs => {}
            _ => {
                latest_by_site.insert(
                    key,
                    ActiveTemporaryAllowlistEntry {
                        site: site.to_string(),
                        remaining_secs,
                        expires_at_epoch_secs: entry.expires_at_epoch_secs,
                    },
                );
            }
        }
    }

    let mut active = latest_by_site.into_values().collect::<Vec<_>>();
    active.sort_by(|left, right| {
        left.remaining_secs.cmp(&right.remaining_secs).then(
            left.site
                .to_ascii_lowercase()
                .cmp(&right.site.to_ascii_lowercase()),
        )
    });
    active
}

fn normalize_single_site(input: &str) -> Result<String, String> {
    let mut blocker = SiteBlocker::new();
    let result = blocker.add_sites_from_input(input);
    if let Some(invalid) = result.invalid.first() {
        return Err(format!(
            "Invalid hostname `{}` ({})",
            display_input_value(&invalid.input),
            invalid.reason.message()
        ));
    }
    if let Some(site) = result.added.first() {
        return Ok(site.clone());
    }
    Err(format!("Invalid hostname `{}`", display_input_value(input)))
}

fn parse_duration_secs(input: &str) -> Result<u64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("duration is empty".to_string());
    }

    let lower = trimmed.to_ascii_lowercase();
    let (number_part, multiplier) = match lower.chars().last() {
        Some('s') => (&lower[..lower.len() - 1], 1u64),
        Some('m') => (&lower[..lower.len() - 1], 60u64),
        Some('h') => (&lower[..lower.len() - 1], 60u64 * 60),
        Some('d') => (&lower[..lower.len() - 1], 60u64 * 60 * 24),
        Some(last) if last.is_ascii_digit() => (lower.as_str(), 1u64),
        _ => {
            return Err(
                "unsupported unit; use seconds (`s`), minutes (`m`), hours (`h`), or days (`d`)"
                    .to_string(),
            );
        }
    };

    if number_part.trim().is_empty() {
        return Err("duration value is missing".to_string());
    }

    let value = number_part
        .trim()
        .parse::<u64>()
        .map_err(|_| "duration must be a whole number".to_string())?;
    if value == 0 {
        return Err("duration must be greater than 0".to_string());
    }
    value
        .checked_mul(multiplier)
        .ok_or_else(|| "duration is too large".to_string())
}

fn display_input_value(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        "<empty>".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_temporary_allowlist_specs_accepts_inline_durations() {
        let parsed = parse_temporary_allowlist_specs("reddit.com=30m, youtube.com=45s").unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].site, "reddit.com");
        assert_eq!(parsed[0].duration_secs, 1800);
        assert_eq!(parsed[1].site, "youtube.com");
        assert_eq!(parsed[1].duration_secs, 45);
    }

    #[test]
    fn parse_temporary_allowlist_specs_rejects_missing_duration() {
        let error = parse_temporary_allowlist_specs("reddit.com").unwrap_err();
        assert!(error.contains("HOST=30m"));
    }

    #[test]
    fn active_temporary_allowlist_status_entries_filter_by_profile_and_expiry() {
        let entries = vec![
            TemporaryAllowlistEntry {
                profile: "Work".to_string(),
                site: "reddit.com".to_string(),
                expires_at_epoch_secs: 200,
            },
            TemporaryAllowlistEntry {
                profile: "work".to_string(),
                site: "news.ycombinator.com".to_string(),
                expires_at_epoch_secs: 150,
            },
            TemporaryAllowlistEntry {
                profile: "Study".to_string(),
                site: "x.com".to_string(),
                expires_at_epoch_secs: 200,
            },
            TemporaryAllowlistEntry {
                profile: "Work".to_string(),
                site: "expired.com".to_string(),
                expires_at_epoch_secs: 50,
            },
        ];

        let active = active_temporary_allowlist_status_entries_for_profile(&entries, "WORK", 100);
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].site, "news.ycombinator.com");
        assert_eq!(active[0].remaining_secs, 50);
        assert_eq!(active[1].site, "reddit.com");
        assert_eq!(active[1].remaining_secs, 100);
    }

    #[test]
    fn upsert_temporary_allowlist_entries_adds_and_refreshes_entries() {
        let mut entries = vec![TemporaryAllowlistEntry {
            profile: "Work".to_string(),
            site: "reddit.com".to_string(),
            expires_at_epoch_secs: 100,
        }];
        let specs = vec![
            ParsedTemporaryAllowlistSpec {
                site: "reddit.com".to_string(),
                duration_secs: 120,
            },
            ParsedTemporaryAllowlistSpec {
                site: "youtube.com".to_string(),
                duration_secs: 60,
            },
        ];

        let (added, refreshed) =
            upsert_temporary_allowlist_entries(&mut entries, "Work", &specs, 1000);
        assert_eq!(added, 1);
        assert_eq!(refreshed, 1);
        assert_eq!(entries.len(), 2);
    }
}
