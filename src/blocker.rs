#[cfg(test)]
use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

const BLOCK_MARKER_START: &str = "# focustime-block-start";
const BLOCK_MARKER_END: &str = "# focustime-block-end";

mod command;
mod hosts;
mod rules;

pub use command::CommandBlockingBackend;
use command::{apply_command_backend, command_backend_diagnostics, preview_from_command};
use hosts::{HOSTS_FILE, atomic_write_hosts_to_path, flush_dns_cache, hosts_file_diagnostics_for};
#[cfg(test)]
use hosts::{HostsWriteFailStep, set_test_hosts_write_fail_steps};

#[cfg(test)]
pub use rules::normalize_domain_host;
pub use rules::{SiteValidationError, domain_rule_matches_host, normalize_domain_rule};

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

pub struct SiteBlocker {
    pub sites: Vec<String>,
    pub is_blocking: bool,
    backend_policy: BlockingBackendPolicy,
    command_backend: CommandBlockingBackend,
    active_backend: Option<BlockingBackendKind>,
    last_backend: Option<BlockingBackendKind>,
    last_fallback_used: bool,
    last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostsFileDiagnostics {
    pub path: String,
    pub read_error: Option<String>,
    pub write_error: Option<String>,
}

impl HostsFileDiagnostics {
    pub fn can_read(&self) -> bool {
        self.read_error.is_none()
    }

    pub fn can_write(&self) -> bool {
        self.write_error.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidSiteInput {
    pub input: String,
    pub reason: SiteValidationError,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkAddResult {
    pub added: Vec<String>,
    pub duplicates: Vec<String>,
    pub invalid: Vec<InvalidSiteInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditSiteResult {
    Updated { old: String, new: String },
    Unchanged { hostname: String },
    Duplicate { hostname: String },
    Invalid(InvalidSiteInput),
    MissingSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingIntent {
    Block,
    Unblock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingPreviewAction {
    Block,
    Unblock,
    NoChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingBackendKind {
    Hosts,
    Command,
}

impl BlockingBackendKind {
    pub fn id(self) -> &'static str {
        match self {
            BlockingBackendKind::Hosts => "hosts",
            BlockingBackendKind::Command => "command",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockingBackendPolicy {
    HostsOnly,
    #[default]
    HostsThenCommand,
    CommandThenHosts,
    CommandOnly,
}

impl BlockingBackendPolicy {
    pub fn id(self) -> &'static str {
        match self {
            BlockingBackendPolicy::HostsOnly => "hosts_only",
            BlockingBackendPolicy::HostsThenCommand => "hosts_then_command",
            BlockingBackendPolicy::CommandThenHosts => "command_then_hosts",
            BlockingBackendPolicy::CommandOnly => "command_only",
        }
    }

    fn backend_order(self) -> Vec<BlockingBackendKind> {
        match self {
            BlockingBackendPolicy::HostsOnly => vec![BlockingBackendKind::Hosts],
            BlockingBackendPolicy::HostsThenCommand => {
                vec![BlockingBackendKind::Hosts, BlockingBackendKind::Command]
            }
            BlockingBackendPolicy::CommandThenHosts => {
                vec![BlockingBackendKind::Command, BlockingBackendKind::Hosts]
            }
            BlockingBackendPolicy::CommandOnly => vec![BlockingBackendKind::Command],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockingBackendStatus {
    pub policy: BlockingBackendPolicy,
    pub order: Vec<BlockingBackendKind>,
    pub active_backend: Option<BlockingBackendKind>,
    pub last_backend: Option<BlockingBackendKind>,
    pub fallback_used: bool,
    pub last_error: Option<String>,
    pub command_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockingPreview {
    pub backend: BlockingBackendKind,
    pub backend_target: String,
    pub attempted_backends: Vec<BlockingBackendKind>,
    pub fallback_used: bool,
    pub hosts_file_path: String,
    pub action: BlockingPreviewAction,
    pub effective_blocked_sites: Vec<String>,
    pub would_change: bool,
    pub current_section: Option<String>,
    pub next_section: Option<String>,
}

impl BlockingPreview {
    pub fn section_for_display(&self) -> Option<&str> {
        self.next_section
            .as_deref()
            .or(self.current_section.as_deref())
    }
}

impl SiteBlocker {
    pub fn new() -> Self {
        Self::with_backend_config(
            BlockingBackendPolicy::default(),
            CommandBlockingBackend::default(),
        )
    }

    pub fn with_backend_config(
        backend_policy: BlockingBackendPolicy,
        command_backend: CommandBlockingBackend,
    ) -> Self {
        Self {
            sites: Vec::new(),
            is_blocking: false,
            backend_policy,
            command_backend: command_backend.normalized(),
            active_backend: None,
            last_backend: None,
            last_fallback_used: false,
            last_error: None,
        }
    }

    pub fn backend_status(&self) -> BlockingBackendStatus {
        BlockingBackendStatus {
            policy: self.backend_policy,
            order: self.backend_policy.backend_order(),
            active_backend: self.active_backend,
            last_backend: self.last_backend,
            fallback_used: self.last_fallback_used,
            last_error: self.last_error.clone(),
            command_configured: self.command_backend.is_configured(),
        }
    }

    pub fn backend_config(&self) -> (BlockingBackendPolicy, CommandBlockingBackend) {
        (self.backend_policy, self.command_backend.clone())
    }

    pub fn command_backend_diagnostics(&self) -> io::Result<()> {
        command_backend_diagnostics(&self.command_backend)
    }

    pub fn add_site(&mut self, site: String) {
        let _ = self.add_sites_from_input(&site);
    }

    pub fn add_sites_from_input(&mut self, input: &str) -> BulkAddResult {
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

    pub fn edit_site_from_input(&mut self, index: usize, input: &str) -> EditSiteResult {
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

    pub fn remove_site(&mut self, index: usize) -> Option<String> {
        if index < self.sites.len() {
            return Some(self.sites.remove(index));
        }
        None
    }

    pub fn hosts_file_diagnostics(&self) -> HostsFileDiagnostics {
        hosts_file_diagnostics_for(Path::new(HOSTS_FILE))
    }

    pub fn preview_hosts_update(&self, intent: BlockingIntent) -> io::Result<BlockingPreview> {
        let order = self.backend_order_for_intent(intent);
        let mut errors = Vec::new();
        for (index, backend) in order.iter().copied().enumerate() {
            match self.preview_with_backend(intent, backend) {
                Ok(mut preview) => {
                    preview.attempted_backends = order[..=index].to_vec();
                    preview.fallback_used = index > 0;
                    return Ok(preview);
                }
                Err(error) => errors.push(format!("{}: {error}", backend.id())),
            }
        }
        Err(io::Error::other(format!(
            "Failed to generate blocking preview ({})",
            errors.join(" | ")
        )))
    }

    /// Activate blocking by writing entries into the hosts file.
    /// Returns an error if the file is not writable (e.g. needs sudo).
    pub fn block(&mut self) -> io::Result<()> {
        #[cfg(test)]
        record_test_blocking_action("block");

        if self.sites.is_empty() {
            if let Some(active_backend) = self.active_backend {
                let _ = self.apply_with_backend(BlockingIntent::Unblock, active_backend);
            }
            // Best-effort: strip any stale block section left by a prior run.
            let _ = self.remove_hosts_block();
            self.is_blocking = false;
            self.active_backend = None;
            self.last_backend = None;
            self.last_fallback_used = false;
            self.last_error = None;
            return Ok(());
        }
        self.apply_with_fallback(BlockingIntent::Block)?;
        self.is_blocking = true;
        Ok(())
    }

    /// Remove the focustime block section from the hosts file.
    /// Always attempts to strip any existing block section, even after a crash
    /// left entries behind with is_blocking == false.
    pub fn unblock(&mut self) -> io::Result<()> {
        #[cfg(test)]
        record_test_blocking_action("unblock");

        self.apply_with_fallback(BlockingIntent::Unblock)?;
        self.is_blocking = false;
        self.active_backend = None;
        Ok(())
    }

    /// Remove block entries on app exit (best-effort).
    pub fn cleanup(&mut self) {
        let _ = self.unblock();
    }

    fn apply_with_fallback(&mut self, intent: BlockingIntent) -> io::Result<()> {
        let order = self.backend_order_for_intent(intent);
        let mut errors = Vec::new();
        let mut first_success: Option<(usize, BlockingBackendKind)> = None;
        let attempt_all_backends = intent == BlockingIntent::Unblock;
        for (index, backend) in order.iter().copied().enumerate() {
            match self.apply_with_backend(intent, backend) {
                Ok(()) => {
                    if first_success.is_none() {
                        first_success = Some((index, backend));
                    }
                    if !attempt_all_backends {
                        break;
                    }
                }
                Err(error) => errors.push(format!("{}: {error}", backend.id())),
            }
        }

        if let Some((index, backend)) = first_success {
            self.last_backend = Some(backend);
            self.last_fallback_used = index > 0;
            self.last_error = None;
            if intent == BlockingIntent::Block {
                self.active_backend = Some(backend);
            }
            return Ok(());
        }

        let message = format!(
            "all configured blocking backends failed for {} ({})",
            blocking_intent_id(intent),
            errors.join(" | ")
        );
        self.last_backend = None;
        self.last_fallback_used = false;
        self.last_error = Some(message.clone());
        Err(io::Error::new(io::ErrorKind::PermissionDenied, message))
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
            BlockingBackendKind::Command => self.apply_command_backend(intent),
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
            BlockingBackendKind::Command => self.preview_from_command(intent),
        }
    }

    fn backend_order_for_intent(&self, intent: BlockingIntent) -> Vec<BlockingBackendKind> {
        if intent == BlockingIntent::Block {
            return self.backend_policy.backend_order();
        }

        let mut order = Vec::new();
        if let Some(active_backend) = self.active_backend {
            order.push(active_backend);
        }
        for backend in self.backend_policy.backend_order() {
            if !order.contains(&backend) {
                order.push(backend);
            }
        }
        order
    }

    fn apply_command_backend(&self, intent: BlockingIntent) -> io::Result<()> {
        apply_command_backend(&self.command_backend, intent, &self.sites)
    }

    fn preview_from_command(&self, intent: BlockingIntent) -> io::Result<BlockingPreview> {
        preview_from_command(&self.command_backend, intent, &self.sites)
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
            attempted_backends: vec![BlockingBackendKind::Hosts],
            fallback_used: false,
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

fn blocking_intent_id(intent: BlockingIntent) -> &'static str {
    match intent {
        BlockingIntent::Block => "block",
        BlockingIntent::Unblock => "unblock",
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
