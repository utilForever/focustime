#[cfg(test)]
use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

const BLOCK_MARKER_START: &str = "# focustime-block-start";
const BLOCK_MARKER_END: &str = "# focustime-block-end";

mod hosts;
mod rules;

use hosts::{HOSTS_FILE, atomic_write_hosts_to_path, flush_dns_cache, hosts_file_diagnostics_for};
#[cfg(test)]
use hosts::{HostsWriteFailStep, set_test_hosts_write_fail_steps};

#[cfg(test)]
pub(crate) use rules::normalize_domain_host;
pub(crate) use rules::{SiteValidationError, domain_rule_matches_host, normalize_domain_rule};

#[cfg(test)]
thread_local! {
    static TEST_LAST_BLOCKING_ACTION: RefCell<Option<&'static str>> = const { RefCell::new(None) };
}

#[cfg(test)]
fn record_test_blocking_action(action: &'static str) {
    TEST_LAST_BLOCKING_ACTION.with(|slot| {
        *slot.borrow_mut() = Some(action);
    });
}

#[cfg(test)]
pub(crate) fn take_test_blocking_action() -> Option<&'static str> {
    TEST_LAST_BLOCKING_ACTION.with(|slot| slot.borrow_mut().take())
}

pub(crate) struct SiteBlocker {
    pub(crate) sites: Vec<String>,
    pub(crate) is_blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostsFileDiagnostics {
    pub(crate) path: String,
    pub(crate) read_error: Option<String>,
    pub(crate) write_error: Option<String>,
}

impl HostsFileDiagnostics {
    pub(crate) fn can_read(&self) -> bool {
        self.read_error.is_none()
    }

    pub(crate) fn can_write(&self) -> bool {
        self.write_error.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InvalidSiteInput {
    pub(crate) input: String,
    pub(crate) reason: SiteValidationError,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BulkAddResult {
    pub(crate) added: Vec<String>,
    pub(crate) duplicates: Vec<String>,
    pub(crate) invalid: Vec<InvalidSiteInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditSiteResult {
    Updated { old: String, new: String },
    Unchanged { hostname: String },
    Duplicate { hostname: String },
    Invalid(InvalidSiteInput),
    MissingSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockingIntent {
    Block,
    Unblock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockingPreviewAction {
    Block,
    Unblock,
    NoChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockingBackendKind {
    Hosts,
}

impl BlockingBackendKind {
    pub(crate) fn id(self) -> &'static str {
        match self {
            BlockingBackendKind::Hosts => "hosts",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockingPreview {
    pub(crate) backend: BlockingBackendKind,
    pub(crate) backend_target: String,
    pub(crate) hosts_file_path: String,
    pub(crate) action: BlockingPreviewAction,
    pub(crate) effective_blocked_sites: Vec<String>,
    pub(crate) would_change: bool,
    pub(crate) current_section: Option<String>,
    pub(crate) next_section: Option<String>,
}

impl BlockingPreview {
    pub(crate) fn section_for_display(&self) -> Option<&str> {
        self.next_section
            .as_deref()
            .or(self.current_section.as_deref())
    }
}

impl SiteBlocker {
    pub(crate) fn new() -> Self {
        Self {
            sites: Vec::new(),
            is_blocking: false,
        }
    }

    pub(crate) fn add_site(&mut self, site: String) {
        let _ = self.add_sites_from_input(&site);
    }

    pub(crate) fn add_sites_from_input(&mut self, input: &str) -> BulkAddResult {
        let candidates = split_hostname_candidates(input);
        let mut result = BulkAddResult::default();
        let mut known_sites: HashSet<String> = self.sites.iter().cloned().collect();

        if candidates.is_empty() {
            result.invalid.push(InvalidSiteInput {
                input: input.trim().to_string(),
                reason: SiteValidationError::EmptyHostname,
            });
            return result;
        }

        for candidate in candidates {
            match Self::sanitize_hostname_with_reason(&candidate) {
                Ok(hostname) => {
                    if !known_sites.insert(hostname.clone()) {
                        result.duplicates.push(hostname);
                    } else {
                        self.sites.push(hostname.clone());
                        result.added.push(hostname);
                    }
                }
                Err(reason) => result.invalid.push(InvalidSiteInput {
                    input: candidate,
                    reason,
                }),
            }
        }

        result
    }

    pub(crate) fn edit_site_from_input(&mut self, index: usize, input: &str) -> EditSiteResult {
        if index >= self.sites.len() {
            return EditSiteResult::MissingSelection;
        }

        let candidates = split_hostname_candidates(input);
        if candidates.is_empty() {
            return EditSiteResult::Invalid(InvalidSiteInput {
                input: input.trim().to_string(),
                reason: SiteValidationError::EmptyHostname,
            });
        }
        if candidates.len() > 1 {
            return EditSiteResult::Invalid(InvalidSiteInput {
                input: input.trim().to_string(),
                reason: SiteValidationError::MultipleHostnames,
            });
        }

        let candidate = candidates[0].clone();
        let normalized = match Self::sanitize_hostname_with_reason(&candidate) {
            Ok(hostname) => hostname,
            Err(reason) => {
                return EditSiteResult::Invalid(InvalidSiteInput {
                    input: candidate,
                    reason,
                });
            }
        };

        let current = self.sites[index].clone();
        if normalized == current {
            return EditSiteResult::Unchanged {
                hostname: normalized,
            };
        }

        if self
            .sites
            .iter()
            .enumerate()
            .any(|(i, site)| i != index && site == &normalized)
        {
            return EditSiteResult::Duplicate {
                hostname: normalized,
            };
        }

        self.sites[index] = normalized.clone();
        EditSiteResult::Updated {
            old: current,
            new: normalized,
        }
    }

    /// Validate and normalise a user-supplied hostname.
    fn sanitize_hostname_with_reason(input: &str) -> Result<String, SiteValidationError> {
        normalize_domain_rule(input)
    }

    pub(crate) fn remove_site(&mut self, index: usize) -> Option<String> {
        if index < self.sites.len() {
            return Some(self.sites.remove(index));
        }
        None
    }

    pub(crate) fn hosts_file_diagnostics(&self) -> HostsFileDiagnostics {
        hosts_file_diagnostics_for(Path::new(HOSTS_FILE))
    }

    pub(crate) fn preview_hosts_update(
        &self,
        intent: BlockingIntent,
    ) -> io::Result<BlockingPreview> {
        self.preview_with_backend(intent, BlockingBackendKind::Hosts)
    }

    /// Activate blocking by writing entries into the hosts file.
    /// Returns an error if the file is not writable (e.g. needs sudo).
    pub(crate) fn block(&mut self) -> io::Result<()> {
        #[cfg(test)]
        record_test_blocking_action("block");

        if self.sites.is_empty() {
            // Best-effort: strip any stale block section left by a prior run.
            let _ = self.remove_hosts_block();
            self.is_blocking = false;
            return Ok(());
        }
        self.apply_with_backend(BlockingIntent::Block, BlockingBackendKind::Hosts)?;
        self.is_blocking = true;
        Ok(())
    }

    /// Remove the focustime block section from the hosts file.
    /// Always attempts to strip any existing block section, even after a crash
    /// left entries behind with is_blocking == false.
    pub(crate) fn unblock(&mut self) -> io::Result<()> {
        #[cfg(test)]
        record_test_blocking_action("unblock");

        self.apply_with_backend(BlockingIntent::Unblock, BlockingBackendKind::Hosts)?;
        self.is_blocking = false;
        Ok(())
    }

    /// Remove block entries on app exit (best-effort).
    pub(crate) fn cleanup(&mut self) {
        let _ = self.unblock();
    }

    fn apply_with_backend(
        &mut self,
        intent: BlockingIntent,
        backend: BlockingBackendKind,
    ) -> io::Result<()> {
        match backend {
            BlockingBackendKind::Hosts => match intent {
                BlockingIntent::Block => self.apply_hosts_block(),
                BlockingIntent::Unblock => self.remove_hosts_block(),
            },
        }
    }

    fn preview_with_backend(
        &self,
        intent: BlockingIntent,
        backend: BlockingBackendKind,
    ) -> io::Result<BlockingPreview> {
        match backend {
            BlockingBackendKind::Hosts => {
                let original = fs::read_to_string(HOSTS_FILE)?;
                Ok(self.preview_from_hosts_content(HOSTS_FILE, &original, intent))
            }
        }
    }

    fn apply_hosts_block(&self) -> io::Result<()> {
        self.apply_hosts_block_to_path(Path::new(HOSTS_FILE))
    }

    fn apply_hosts_block_to_path(&self, hosts_path: &Path) -> io::Result<()> {
        let hosts_sites = self.hosts_renderable_sites();
        let changed = atomic_write_hosts_to_path(hosts_path, |snapshot_content| {
            if hosts_sites.is_empty() {
                let cleaned = Self::strip_block_section(snapshot_content);
                if cleaned == snapshot_content {
                    None
                } else {
                    Some(cleaned)
                }
            } else {
                Some(self.build_blocked_hosts_content(snapshot_content, &hosts_sites))
            }
        })?;
        if changed {
            flush_dns_cache();
        }
        Ok(())
    }

    fn remove_hosts_block(&self) -> io::Result<()> {
        self.remove_hosts_block_from_path(Path::new(HOSTS_FILE))
    }

    fn remove_hosts_block_from_path(&self, hosts_path: &Path) -> io::Result<()> {
        // Fast path: when there is no managed section to remove, avoid creating
        // rollback artifacts in the hosts directory (which may require write
        // permission even for a no-op unblock).
        let current = fs::read_to_string(hosts_path)?;
        if Self::strip_block_section(&current) == current {
            return Ok(());
        }

        let changed = atomic_write_hosts_to_path(hosts_path, |snapshot_content| {
            let cleaned = Self::strip_block_section(snapshot_content);
            if cleaned == snapshot_content {
                None
            } else {
                Some(cleaned)
            }
        })?;
        // Only flush DNS when we actually changed the hosts file.
        if changed {
            flush_dns_cache();
        }
        Ok(())
    }

    pub(crate) fn strip_block_section(content: &str) -> String {
        // Verify that both markers exist as *complete* lines in the correct order
        // before attempting to strip. This prevents a marker that appears only as
        // a substring of a longer line (e.g. "# focustime-block-end  extra") from
        // being found by a substring search but then silently missed by the line
        // loop, which would drop the remainder of the file.
        let has_block = {
            let mut found_start = false;
            content.lines().any(|line| {
                if !found_start {
                    found_start = line.trim() == BLOCK_MARKER_START;
                    false
                } else {
                    line.trim() == BLOCK_MARKER_END
                }
            })
        };
        if !has_block {
            return content.to_string();
        }

        // Preserve the original line ending style (LF vs CRLF).
        let nl = line_ending_for(content);
        let mut result = String::with_capacity(content.len());
        let mut in_block = false;

        for line in content.lines() {
            if line.trim() == BLOCK_MARKER_START {
                in_block = true;
                continue;
            }
            if line.trim() == BLOCK_MARKER_END {
                in_block = false;
                continue;
            }
            if !in_block {
                result.push_str(line);
                result.push_str(nl);
            }
        }

        result
    }

    fn preview_from_hosts_content(
        &self,
        hosts_file_path: &str,
        original: &str,
        intent: BlockingIntent,
    ) -> BlockingPreview {
        let nl = line_ending_for(original);
        let current_section = Self::extract_block_sections(original);
        let hosts_sites = self.hosts_renderable_sites();
        let next_section = match intent {
            BlockingIntent::Block if !hosts_sites.is_empty() => {
                Some(self.render_block_section(nl, &hosts_sites))
            }
            BlockingIntent::Block | BlockingIntent::Unblock => None,
        };
        let next_content = match next_section.as_deref() {
            Some(section) => Self::build_hosts_content_with_section(original, section, nl),
            None => Self::strip_block_section(original),
        };
        let would_change = next_content != original;
        let action = if !would_change {
            BlockingPreviewAction::NoChange
        } else if next_section.is_some() {
            BlockingPreviewAction::Block
        } else {
            BlockingPreviewAction::Unblock
        };
        let effective_blocked_sites = if next_section.is_some() {
            hosts_sites
        } else {
            Vec::new()
        };

        BlockingPreview {
            backend: BlockingBackendKind::Hosts,
            backend_target: hosts_file_path.to_string(),
            hosts_file_path: hosts_file_path.to_string(),
            action,
            effective_blocked_sites,
            would_change,
            current_section,
            next_section,
        }
    }

    fn build_blocked_hosts_content(&self, original: &str, hosts_sites: &[String]) -> String {
        let nl = line_ending_for(original);
        let section = self.render_block_section(nl, hosts_sites);
        Self::build_hosts_content_with_section(original, &section, nl)
    }

    fn build_hosts_content_with_section(original: &str, section: &str, nl: &str) -> String {
        let mut content = Self::strip_block_section(original);

        // Only insert a separator newline when the content doesn't already end
        // with one, so repeated focus/break cycles don't accumulate blank lines.
        if !content.ends_with(nl) && !content.is_empty() {
            content.push_str(nl);
        }
        content.push_str(section);
        content
    }

    fn render_block_section(&self, nl: &str, hosts_sites: &[String]) -> String {
        let mut section = String::new();
        section.push_str(BLOCK_MARKER_START);
        section.push_str(nl);
        for site in hosts_sites {
            append_site_entries(&mut section, site, nl);
        }
        section.push_str(BLOCK_MARKER_END);
        section.push_str(nl);
        section
    }

    fn hosts_renderable_sites(&self) -> Vec<String> {
        self.sites
            .iter()
            .filter(|site| !site.starts_with("*."))
            .cloned()
            .collect()
    }

    fn extract_block_sections(content: &str) -> Option<String> {
        let nl = line_ending_for(content);
        let mut in_block = false;
        let mut section = String::new();
        let mut sections = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if !in_block {
                if trimmed == BLOCK_MARKER_START {
                    in_block = true;
                    section.clear();
                    section.push_str(BLOCK_MARKER_START);
                    section.push_str(nl);
                }
                continue;
            }

            if trimmed == BLOCK_MARKER_END {
                section.push_str(BLOCK_MARKER_END);
                section.push_str(nl);
                sections.push(std::mem::take(&mut section));
                in_block = false;
                continue;
            }

            section.push_str(line);
            section.push_str(nl);
        }

        if sections.is_empty() {
            None
        } else {
            Some(sections.concat())
        }
    }
}

impl Default for SiteBlocker {
    fn default() -> Self {
        Self::new()
    }
}

fn line_ending_for(content: &str) -> &'static str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn append_site_entries(content: &mut String, site: &str, nl: &str) {
    append_hosts_mapping(content, "127.0.0.1", site, nl);
    append_hosts_mapping(content, "::1", site, nl);
    if !site.starts_with("www.") {
        let www_site = format!("www.{site}");
        append_hosts_mapping(content, "127.0.0.1", &www_site, nl);
        append_hosts_mapping(content, "::1", &www_site, nl);
    }
}

fn append_hosts_mapping(content: &mut String, host: &str, site: &str, nl: &str) {
    content.push_str(host);
    content.push(' ');
    content.push_str(site);
    content.push_str(nl);
}

fn split_hostname_candidates(input: &str) -> Vec<String> {
    input
        .split([',', '\n', '\r'])
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests;
