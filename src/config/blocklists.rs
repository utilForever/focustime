use std::collections::HashSet;

use super::normalize_nonempty_or_default_string;
use crate::blocker::{domain_rule_matches_host, normalize_domain_rule};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "BlocklistProfileConfigDisk")]
pub(crate) struct BlocklistProfileConfig {
    #[serde(default = "default_blocklist_profile_name")]
    pub(crate) name: String,
    /// Profile-level blocklist rules.
    #[serde(default)]
    pub(crate) sites: Vec<String>,
    /// Profile-level allowlist rules. Effective focus blocking is computed as
    /// `sites - allowlist_sites`.
    #[serde(default)]
    pub(crate) allowlist_sites: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct BlocklistProfileConfigDisk {
    #[serde(default = "default_blocklist_profile_name")]
    name: String,
    #[serde(default)]
    sites: Vec<String>,
    #[serde(default)]
    allowlist_sites: Vec<String>,
    #[serde(default)]
    categories: Vec<BlocklistCategoryConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct BlocklistCategoryConfig {
    #[serde(default = "default_blocklist_category_name")]
    name: String,
    #[serde(default)]
    sites: Vec<String>,
    #[serde(default)]
    allowlist_sites: Vec<String>,
}

impl Default for BlocklistProfileConfig {
    fn default() -> Self {
        Self {
            name: default_blocklist_profile_name(),
            sites: Vec::new(),
            allowlist_sites: Vec::new(),
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

impl From<BlocklistProfileConfigDisk> for BlocklistProfileConfig {
    fn from(profile: BlocklistProfileConfigDisk) -> Self {
        let (sites, allowlist_sites) = flattened_profile_rules(
            &profile.categories,
            &profile.sites,
            &profile.allowlist_sites,
        );
        Self {
            name: profile.name,
            sites,
            allowlist_sites,
        }
    }
}

pub(crate) fn effective_blocked_sites_for_profile(profile: &BlocklistProfileConfig) -> Vec<String> {
    let allowlist_rules: Vec<String> = all_allowlist_rules_for_profile(profile)
        .iter()
        .filter_map(|rule| normalize_domain_rule(rule).ok())
        .collect();

    let mut seen = HashSet::new();
    all_blocklist_rules_for_profile(profile)
        .into_iter()
        .filter_map(|site| normalize_domain_rule(&site).ok())
        .filter(|site| !block_rule_excluded_by_allowlist(site, &allowlist_rules))
        .filter(|site| seen.insert(site.to_ascii_lowercase()))
        .collect()
}

fn block_rule_excluded_by_allowlist(block_rule: &str, allowlist_rules: &[String]) -> bool {
    if let Some(block_suffix) = block_rule.strip_prefix("*.") {
        let block_suffix = block_suffix.to_ascii_lowercase();
        return allowlist_rules.iter().any(|allow_rule| {
            if allow_rule.eq_ignore_ascii_case(block_rule) {
                return true;
            }
            let Some(allow_suffix) = allow_rule.strip_prefix("*.") else {
                return false;
            };
            let allow_suffix = allow_suffix.to_ascii_lowercase();
            block_suffix == allow_suffix || block_suffix.ends_with(&format!(".{allow_suffix}"))
        });
    }
    allowlist_rules
        .iter()
        .any(|allow_rule| domain_rule_matches_host(allow_rule, block_rule))
}

fn all_blocklist_rules_for_profile(profile: &BlocklistProfileConfig) -> Vec<String> {
    dedup_case_insensitive(profile.sites.iter().cloned())
}

fn all_allowlist_rules_for_profile(profile: &BlocklistProfileConfig) -> Vec<String> {
    dedup_case_insensitive(profile.allowlist_sites.iter().cloned())
}

fn dedup_case_insensitive<I>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}

pub(super) fn default_blocklist_profile_name() -> String {
    "Default".to_string()
}

fn default_blocklist_category_name() -> String {
    "General".to_string()
}

pub(super) fn normalize_blocklist_profiles(
    profiles: &[BlocklistProfileConfig],
    legacy_blocked_sites: &[String],
) -> Vec<BlocklistProfileConfig> {
    let mut sites = dedup_case_insensitive(legacy_blocked_sites.iter().cloned());
    let mut allowlist_sites = Vec::new();
    for profile in profiles {
        merge_unique_case_insensitive(&mut sites, &profile.sites);
        merge_unique_case_insensitive(&mut allowlist_sites, &profile.allowlist_sites);
    }

    vec![BlocklistProfileConfig {
        name: default_blocklist_profile_name(),
        sites,
        allowlist_sites,
    }]
}

pub(super) fn normalize_selected_blocklist_profile(
    _selected_name: &str,
    _profiles: &[BlocklistProfileConfig],
) -> String {
    default_blocklist_profile_name()
}

pub(super) fn make_unique_profile_name(
    base_name: &str,
    seen_names: &mut HashSet<String>,
) -> String {
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

fn flattened_profile_rules(
    categories: &[BlocklistCategoryConfig],
    legacy_sites: &[String],
    legacy_allowlist_sites: &[String],
) -> (Vec<String>, Vec<String>) {
    let categories =
        normalize_blocklist_categories(categories, legacy_sites, legacy_allowlist_sites);
    flatten_blocklist_categories(&categories)
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
        let target_index = if let Some(index) = normalized.iter().position(|category| {
            category
                .name
                .eq_ignore_ascii_case(&default_blocklist_category_name())
        }) {
            index
        } else {
            normalized.push(BlocklistCategoryConfig {
                name: default_blocklist_category_name(),
                sites: Vec::new(),
                allowlist_sites: Vec::new(),
            });
            normalized.len().saturating_sub(1)
        };
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
