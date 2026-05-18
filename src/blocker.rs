#[cfg(test)]
use std::cell::RefCell;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
const HOSTS_FILE: &str = r"C:\Windows\System32\drivers\etc\hosts";
#[cfg(not(target_os = "windows"))]
const HOSTS_FILE: &str = "/etc/hosts";
const BLOCK_MARKER_START: &str = "# focustime-block-start";
const BLOCK_MARKER_END: &str = "# focustime-block-end";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostsWriteFailStep {
    Snapshot,
    StageWrite,
    AfterReplace,
    RollbackRestore,
}

#[cfg(test)]
thread_local! {
    static TEST_LAST_BLOCKING_ACTION: RefCell<Option<&'static str>> = const { RefCell::new(None) };
    static TEST_HOSTS_WRITE_FAIL_STEPS: RefCell<Vec<HostsWriteFailStep>> = const { RefCell::new(Vec::new()) };
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

#[cfg(test)]
fn set_test_hosts_write_fail_steps(steps: &[HostsWriteFailStep]) {
    TEST_HOSTS_WRITE_FAIL_STEPS.with(|slot| {
        *slot.borrow_mut() = steps.to_vec();
    });
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteValidationError {
    EmptyHostname,
    MissingHostname,
    ContainsWhitespace,
    InvalidCharacter,
    InvalidLabel,
    MultipleHostnames,
}

impl SiteValidationError {
    pub fn message(self) -> &'static str {
        match self {
            SiteValidationError::EmptyHostname => "empty hostname",
            SiteValidationError::MissingHostname => "missing hostname",
            SiteValidationError::ContainsWhitespace => "contains whitespace",
            SiteValidationError::InvalidCharacter => "contains invalid characters",
            SiteValidationError::InvalidLabel => "invalid hostname format",
            SiteValidationError::MultipleHostnames => "multiple hostnames not allowed",
        }
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandBlockingBackend {
    pub block_command: String,
    pub unblock_command: String,
    pub diagnostics_command: String,
}

impl CommandBlockingBackend {
    pub fn normalized(&self) -> Self {
        Self {
            block_command: self.block_command.trim().to_string(),
            unblock_command: self.unblock_command.trim().to_string(),
            diagnostics_command: self.diagnostics_command.trim().to_string(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.block_command.trim().is_empty() && !self.unblock_command.trim().is_empty()
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
        if self.command_backend.diagnostics_command.trim().is_empty() {
            if self.command_backend.is_configured() {
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "command backend block/unblock commands are not fully configured",
            ));
        }
        run_shell_command(&self.command_backend.diagnostics_command)
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
        let mut hostname = input.trim().to_lowercase();

        if hostname.is_empty() {
            return Err(SiteValidationError::EmptyHostname);
        }

        // Strip URI scheme (e.g. "https://example.com" → "example.com").
        if let Some(sep) = hostname.find("://") {
            hostname = hostname[sep + 3..].to_string();
        }

        // Remove path, query, or fragment after the hostname.
        if let Some(pos) = hostname.find(['/', '?', '#']) {
            hostname.truncate(pos);
        }

        if let Some(at_pos) = hostname.rfind('@') {
            hostname = hostname[at_pos + 1..].to_string();
        }

        // Strip a port suffix from host:port forms when the suffix is numeric.
        if let Some(colon_pos) = hostname.rfind(':') {
            let port = &hostname[colon_pos + 1..];
            if hostname[..colon_pos].contains(':') || !port.chars().all(|c| c.is_ascii_digit()) {
                return Err(SiteValidationError::InvalidLabel);
            }
            hostname.truncate(colon_pos);
        }

        if hostname.is_empty() {
            return Err(SiteValidationError::MissingHostname);
        }

        // Reject anything with internal whitespace (would produce multi-hostname lines).
        if hostname.chars().any(char::is_whitespace) {
            return Err(SiteValidationError::ContainsWhitespace);
        }

        // Allow only ASCII letters, digits, dots, and hyphens.
        if !hostname
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
        {
            return Err(SiteValidationError::InvalidCharacter);
        }

        if hostname.starts_with('.')
            || hostname.ends_with('.')
            || hostname.contains("..")
            || hostname.len() > 253
        {
            return Err(SiteValidationError::InvalidLabel);
        }

        for label in hostname.split('.') {
            if label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
            {
                return Err(SiteValidationError::InvalidLabel);
            }
        }

        Ok(hostname)
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
        let template = match intent {
            BlockingIntent::Block => self.command_backend.block_command.as_str(),
            BlockingIntent::Unblock => self.command_backend.unblock_command.as_str(),
        };
        let template = template.trim();
        if template.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "command backend `{}` command is not configured",
                    blocking_intent_id(intent)
                ),
            ));
        }
        let rendered = render_command_template(template, &self.sites);
        run_shell_command(&rendered)
    }

    fn preview_from_command(&self, intent: BlockingIntent) -> io::Result<BlockingPreview> {
        let template = match intent {
            BlockingIntent::Block => self.command_backend.block_command.as_str(),
            BlockingIntent::Unblock => self.command_backend.unblock_command.as_str(),
        }
        .trim()
        .to_string();
        if template.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "command backend `{}` command is not configured",
                    blocking_intent_id(intent)
                ),
            ));
        }

        let rendered = render_command_template(&template, &self.sites);
        let action = match intent {
            BlockingIntent::Block => {
                if self.sites.is_empty() {
                    BlockingPreviewAction::NoChange
                } else {
                    BlockingPreviewAction::Block
                }
            }
            BlockingIntent::Unblock => BlockingPreviewAction::Unblock,
        };
        let effective_blocked_sites = if action == BlockingPreviewAction::Block {
            self.sites.clone()
        } else {
            Vec::new()
        };

        Ok(BlockingPreview {
            backend: BlockingBackendKind::Command,
            backend_target: rendered.clone(),
            attempted_backends: vec![BlockingBackendKind::Command],
            fallback_used: false,
            hosts_file_path: rendered,
            action,
            effective_blocked_sites,
            would_change: action != BlockingPreviewAction::NoChange,
            current_section: None,
            next_section: None,
        })
    }

    fn apply_hosts_block(&self) -> io::Result<()> {
        self.apply_hosts_block_to_path(Path::new(HOSTS_FILE))
    }

    fn apply_hosts_block_to_path(&self, hosts_path: &Path) -> io::Result<()> {
        let original = fs::read_to_string(hosts_path)?;
        let content = self.build_blocked_hosts_content(&original);
        atomic_write_hosts_to_path(hosts_path, &content)?;
        flush_dns_cache();
        Ok(())
    }

    fn remove_hosts_block(&self) -> io::Result<()> {
        self.remove_hosts_block_from_path(Path::new(HOSTS_FILE))
    }

    fn remove_hosts_block_from_path(&self, hosts_path: &Path) -> io::Result<()> {
        let content = fs::read_to_string(hosts_path)?;
        let cleaned = Self::strip_block_section(&content);
        // Only write back if something was actually removed.
        if cleaned != content {
            atomic_write_hosts_to_path(hosts_path, &cleaned)?;
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
        let next_section = match intent {
            BlockingIntent::Block if !self.sites.is_empty() => Some(self.render_block_section(nl)),
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
            self.sites.clone()
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

    fn build_blocked_hosts_content(&self, original: &str) -> String {
        let nl = line_ending_for(original);
        let section = self.render_block_section(nl);
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

    fn render_block_section(&self, nl: &str) -> String {
        let mut section = String::new();
        section.push_str(BLOCK_MARKER_START);
        section.push_str(nl);
        for site in &self.sites {
            append_site_entries(&mut section, site, nl);
        }
        section.push_str(BLOCK_MARKER_END);
        section.push_str(nl);
        section
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

fn render_command_template(template: &str, sites: &[String]) -> String {
    let sites_csv = sites.join(",");
    let sites_lines = sites.join("\n");
    template
        .replace("{sites_csv}", &sites_csv)
        .replace("{sites_lines}", &sites_lines)
        .replace("{site_count}", &sites.len().to_string())
}

fn run_shell_command(command: &str) -> io::Result<()> {
    let status = shell_command(command).status()?;
    if status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "command exited with status {}",
        status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    )))
}

#[cfg(target_os = "windows")]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", command]);
    cmd
}

#[cfg(not(target_os = "windows"))]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", command]);
    cmd
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

fn hosts_file_diagnostics_for(path: &Path) -> HostsFileDiagnostics {
    let read_error = fs::File::open(path).err().map(|error| error.to_string());
    let write_error = probe_hosts_write_path(path)
        .err()
        .map(|error| error.to_string());
    HostsFileDiagnostics {
        path: path.display().to_string(),
        read_error,
        write_error,
    }
}

#[cfg(target_os = "windows")]
fn probe_hosts_write_path(path: &Path) -> io::Result<()> {
    OpenOptions::new().append(true).open(path).map(|_| ())
}

#[cfg(not(target_os = "windows"))]
fn probe_hosts_write_path(path: &Path) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let probe = dir.join(format!(
        ".focustime_hosts_probe_{}_{}",
        std::process::id(),
        nanos
    ));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    fs::remove_file(&probe)
}

enum HostsReplaceError {
    NoMutation(io::Error),
    NeedsRollback(io::Error),
}

/// Write `content` to the hosts file with staged replacement and rollback.
/// If an update fails after replacing the destination, the previous state is
/// restored from a snapshot to avoid leaving partial focustime sections behind.
fn atomic_write_hosts_to_path(hosts_path: &Path, content: &str) -> io::Result<()> {
    let staged_path = hosts_transaction_temp_path(hosts_path, "staged");
    let snapshot_path = hosts_transaction_temp_path(hosts_path, "snapshot");

    create_hosts_snapshot(hosts_path, &snapshot_path)?;
    if let Err(error) = write_staged_hosts_file(hosts_path, &staged_path, content) {
        let _ = remove_file_if_exists(&staged_path);
        let _ = remove_file_if_exists(&snapshot_path);
        return Err(error);
    }

    match replace_hosts_file(&staged_path, hosts_path) {
        Ok(()) => {}
        Err(HostsReplaceError::NoMutation(error)) => {
            let _ = remove_file_if_exists(&staged_path);
            let _ = remove_file_if_exists(&snapshot_path);
            return Err(error);
        }
        Err(HostsReplaceError::NeedsRollback(error)) => {
            return rollback_hosts_write(hosts_path, &snapshot_path, &staged_path, error);
        }
    }

    if let Err(error) = maybe_fail_hosts_write_step(HostsWriteFailStep::AfterReplace) {
        return rollback_hosts_write(hosts_path, &snapshot_path, &staged_path, error);
    }

    remove_file_if_exists(&staged_path)?;
    remove_file_if_exists(&snapshot_path)?;
    Ok(())
}

fn rollback_hosts_write(
    hosts_path: &Path,
    snapshot_path: &Path,
    staged_path: &Path,
    update_error: io::Error,
) -> io::Result<()> {
    let rollback_result = restore_hosts_snapshot(hosts_path, snapshot_path);
    let _ = remove_file_if_exists(staged_path);
    let _ = remove_file_if_exists(snapshot_path);

    match rollback_result {
        Ok(()) => Err(io::Error::new(
            update_error.kind(),
            format!("failed to update hosts file: {update_error}; previous state restored"),
        )),
        Err(rollback_error) => Err(io::Error::other(format!(
            "failed to update hosts file: {update_error}; rollback failed: {rollback_error}"
        ))),
    }
}

fn create_hosts_snapshot(hosts_path: &Path, snapshot_path: &Path) -> io::Result<()> {
    maybe_fail_hosts_write_step(HostsWriteFailStep::Snapshot)?;
    fs::copy(hosts_path, snapshot_path).map(|_| ())
}

fn write_staged_hosts_file(hosts_path: &Path, staged_path: &Path, content: &str) -> io::Result<()> {
    maybe_fail_hosts_write_step(HostsWriteFailStep::StageWrite)?;
    fs::write(staged_path, content)?;
    // Copy the original file's permissions onto the staged file so replacement
    // does not silently change the hosts file mode bits.
    if let Ok(meta) = fs::metadata(hosts_path) {
        let _ = fs::set_permissions(staged_path, meta.permissions());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn replace_hosts_file(staged_path: &Path, hosts_path: &Path) -> Result<(), HostsReplaceError> {
    match fs::rename(staged_path, hosts_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(hosts_path).map_err(HostsReplaceError::NoMutation)?;
            fs::rename(staged_path, hosts_path).map_err(HostsReplaceError::NeedsRollback)
        }
        Err(error) => Err(HostsReplaceError::NoMutation(error)),
    }
}

#[cfg(not(target_os = "windows"))]
fn replace_hosts_file(staged_path: &Path, hosts_path: &Path) -> Result<(), HostsReplaceError> {
    fs::rename(staged_path, hosts_path).map_err(HostsReplaceError::NoMutation)
}

fn restore_hosts_snapshot(hosts_path: &Path, snapshot_path: &Path) -> io::Result<()> {
    maybe_fail_hosts_write_step(HostsWriteFailStep::RollbackRestore)?;
    restore_snapshot_file(snapshot_path, hosts_path)
}

#[cfg(target_os = "windows")]
fn restore_snapshot_file(snapshot_path: &Path, hosts_path: &Path) -> io::Result<()> {
    match fs::rename(snapshot_path, hosts_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(hosts_path)?;
            fs::rename(snapshot_path, hosts_path)
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(target_os = "windows"))]
fn restore_snapshot_file(snapshot_path: &Path, hosts_path: &Path) -> io::Result<()> {
    fs::rename(snapshot_path, hosts_path)
}

fn hosts_transaction_temp_path(hosts_path: &Path, marker: &str) -> PathBuf {
    let dir = hosts_path.parent().unwrap_or(Path::new("."));
    let target_name = hosts_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("hosts");
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    dir.join(format!(
        ".{target_name}.focustime.{pid}.{nanos}.{marker}.tmp"
    ))
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn maybe_fail_hosts_write_step(step: HostsWriteFailStep) -> io::Result<()> {
    #[cfg(test)]
    {
        let should_fail = TEST_HOSTS_WRITE_FAIL_STEPS.with(|slot| {
            let mut configured = slot.borrow_mut();
            if configured.first().copied() == Some(step) {
                configured.remove(0);
                true
            } else {
                false
            }
        });
        if should_fail {
            return Err(io::Error::other(format!(
                "simulated hosts write failure at {step:?}"
            )));
        }
    }
    #[cfg(not(test))]
    {
        let _ = step;
    }

    Ok(())
}

/// Flush the OS DNS cache so /etc/hosts changes take effect immediately.
/// Best-effort: failures are silently ignored.
fn flush_dns_cache() {
    #[cfg(target_os = "macos")]
    {
        // Flush mDNSResponder cache (macOS 10.10.4+)
        let _ = Command::new("dscacheutil").arg("-flushcache").status();
        let _ = Command::new("killall")
            .args(["-HUP", "mDNSResponder"])
            .status();
    }
    #[cfg(target_os = "linux")]
    {
        // systemd-resolved
        let _ = Command::new("systemd-resolve")
            .arg("--flush-caches")
            .status();
        // nscd (older systems)
        let _ = Command::new("nscd").args(["-i", "hosts"]).status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("ipconfig").arg("/flushdns").status();
    }
}

#[cfg(test)]
mod tests {
    use crate::blocker::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_hosts_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("focustime-hosts-rollback-{label}-{unique}.tmp"))
    }

    #[test]
    fn strip_unterminated_start_marker_leaves_content_unchanged() {
        // A lone start marker without an end marker must not drop any content.
        let input = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n";
        assert_eq!(SiteBlocker::strip_block_section(input), input);
    }

    #[test]
    fn strip_empty_string() {
        assert_eq!(SiteBlocker::strip_block_section(""), "");
    }

    #[test]
    fn strip_no_block_section_is_unchanged() {
        let input = "127.0.0.1 localhost\n::1 localhost\n";
        assert_eq!(SiteBlocker::strip_block_section(input), input);
    }

    #[test]
    fn strip_removes_block_section() {
        let input = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n# focustime-block-end\n::1 localhost\n";
        let expected = "127.0.0.1 localhost\n::1 localhost\n";
        assert_eq!(SiteBlocker::strip_block_section(input), expected);
    }

    #[test]
    fn strip_removes_block_section_at_end_of_file() {
        let input = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n# focustime-block-end\n";
        let expected = "127.0.0.1 localhost\n";
        assert_eq!(SiteBlocker::strip_block_section(input), expected);
    }

    #[test]
    fn strip_multiple_sites_in_section() {
        let input = "before\n# focustime-block-start\n127.0.0.1 a.com\n127.0.0.1 b.com\n# focustime-block-end\nafter\n";
        let expected = "before\nafter\n";
        assert_eq!(SiteBlocker::strip_block_section(input), expected);
    }

    #[test]
    fn add_site_normalizes_and_deduplicates() {
        let mut b = SiteBlocker::new();
        b.add_site("  Example.COM  ".to_string());
        b.add_site("example.com".to_string());
        assert_eq!(b.sites, vec!["example.com"]);
    }

    #[test]
    fn add_site_ignores_empty() {
        let mut b = SiteBlocker::new();
        b.add_site("   ".to_string());
        assert!(b.sites.is_empty());
    }

    #[test]
    fn add_site_strips_scheme_and_path() {
        let mut b = SiteBlocker::new();
        b.add_site("https://example.com/some/path?q=1".to_string());
        assert_eq!(b.sites, vec!["example.com"]);
    }

    #[test]
    fn add_site_strips_numeric_port() {
        let mut b = SiteBlocker::new();
        b.add_site("https://example.com:443/some/path".to_string());
        assert_eq!(b.sites, vec!["example.com"]);
    }

    #[test]
    fn add_site_rejects_multiple_hostnames() {
        let mut b = SiteBlocker::new();
        b.add_site("example.com other.com".to_string());
        assert!(b.sites.is_empty());
    }

    #[test]
    fn add_site_rejects_invalid_characters() {
        let mut b = SiteBlocker::new();
        b.add_site("exam_ple.com".to_string());
        assert!(b.sites.is_empty());
    }

    #[test]
    fn bulk_add_accepts_comma_and_newline_separators() {
        let mut b = SiteBlocker::new();
        let result = b.add_sites_from_input("example.com, github.com\nhttps://rust-lang.org/docs");
        assert_eq!(
            result.added,
            vec!["example.com", "github.com", "rust-lang.org"]
        );
        assert!(result.duplicates.is_empty());
        assert!(result.invalid.is_empty());
        assert_eq!(b.sites, vec!["example.com", "github.com", "rust-lang.org"]);
    }

    #[test]
    fn bulk_add_reports_duplicates_and_invalid_entries() {
        let mut b = SiteBlocker::new();
        let result = b.add_sites_from_input("github.com, bad host, exam_ple.com, github.com");
        assert_eq!(result.added, vec!["github.com"]);
        assert_eq!(result.duplicates, vec!["github.com"]);
        assert_eq!(
            result.invalid,
            vec![
                InvalidSiteInput {
                    input: "bad host".to_string(),
                    reason: SiteValidationError::ContainsWhitespace,
                },
                InvalidSiteInput {
                    input: "exam_ple.com".to_string(),
                    reason: SiteValidationError::InvalidCharacter,
                }
            ]
        );
    }

    #[test]
    fn edit_site_updates_selected_entry() {
        let mut b = SiteBlocker::new();
        b.add_site("a.com".to_string());
        let result = b.edit_site_from_input(0, "https://news.ycombinator.com:443/newest");
        assert_eq!(
            result,
            EditSiteResult::Updated {
                old: "a.com".to_string(),
                new: "news.ycombinator.com".to_string()
            }
        );
        assert_eq!(b.sites, vec!["news.ycombinator.com"]);
    }

    #[test]
    fn edit_site_rejects_duplicate_hostname() {
        let mut b = SiteBlocker::new();
        b.add_site("a.com".to_string());
        b.add_site("b.com".to_string());
        let result = b.edit_site_from_input(0, "b.com");
        assert_eq!(
            result,
            EditSiteResult::Duplicate {
                hostname: "b.com".to_string()
            }
        );
        assert_eq!(b.sites, vec!["a.com", "b.com"]);
    }

    #[test]
    fn edit_site_rejects_multiple_hostnames() {
        let mut b = SiteBlocker::new();
        b.add_site("a.com".to_string());
        let result = b.edit_site_from_input(0, "a.com, b.com");
        assert_eq!(
            result,
            EditSiteResult::Invalid(InvalidSiteInput {
                input: "a.com, b.com".to_string(),
                reason: SiteValidationError::MultipleHostnames,
            })
        );
    }

    #[test]
    fn strip_out_of_order_markers_leaves_content_unchanged() {
        // End marker before start marker: treat as corrupt, return unchanged.
        let input = "127.0.0.1 localhost\n# focustime-block-end\n# focustime-block-start\nafter\n";
        assert_eq!(SiteBlocker::strip_block_section(input), input);
    }

    #[test]
    fn strip_marker_with_trailing_content_leaves_unchanged() {
        // Markers that appear as substrings of longer lines must not be treated
        // as valid markers; the whole file should be returned untouched.
        let input = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n# focustime-block-end extra\n::1 localhost\n";
        assert_eq!(SiteBlocker::strip_block_section(input), input);
    }

    #[test]
    fn strip_preserves_crlf_line_endings() {
        let input = "127.0.0.1 localhost\r\n# focustime-block-start\r\n127.0.0.1 example.com\r\n# focustime-block-end\r\n::1 localhost\r\n";
        let expected = "127.0.0.1 localhost\r\n::1 localhost\r\n";
        assert_eq!(SiteBlocker::strip_block_section(input), expected);
    }

    #[test]
    fn remove_site_by_index() {
        let mut b = SiteBlocker::new();
        b.add_site("a.com".to_string());
        b.add_site("b.com".to_string());
        let removed = b.remove_site(0);
        assert_eq!(removed.as_deref(), Some("a.com"));
        assert_eq!(b.sites, vec!["b.com"]);
    }

    #[test]
    fn remove_site_out_of_bounds_is_safe() {
        let mut b = SiteBlocker::new();
        b.add_site("a.com".to_string());
        assert!(b.remove_site(5).is_none()); // should not panic
        assert_eq!(b.sites.len(), 1);
    }

    #[test]
    fn hosts_file_diagnostics_reports_read_and_write_success_for_temp_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("focustime-hosts-diagnostics-{unique}.tmp"));
        fs::write(&path, "127.0.0.1 localhost\n").expect("temp hosts file should be writable");

        let diagnostics = hosts_file_diagnostics_for(&path);

        assert!(diagnostics.can_read());
        assert!(diagnostics.can_write());
        assert!(diagnostics.read_error.is_none());
        assert!(diagnostics.write_error.is_none());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn hosts_file_diagnostics_reports_missing_file_errors() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("focustime-hosts-diagnostics-missing-{unique}.tmp"));
        let _ = fs::remove_file(&path);

        let diagnostics = hosts_file_diagnostics_for(&path);

        assert!(!diagnostics.can_read());
        assert!(diagnostics.read_error.is_some());
        #[cfg(target_os = "windows")]
        {
            assert!(!diagnostics.can_write());
            assert!(diagnostics.write_error.is_some());
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert!(diagnostics.can_write());
            assert!(diagnostics.write_error.is_none());
        }
    }

    #[test]
    fn preview_block_reports_next_section_and_change() {
        let mut blocker = SiteBlocker::new();
        blocker.add_site("example.com".to_string());
        let original = "127.0.0.1 localhost\n";

        let preview = blocker.preview_from_hosts_content("hosts", original, BlockingIntent::Block);

        assert_eq!(preview.action, BlockingPreviewAction::Block);
        assert!(preview.would_change);
        assert_eq!(preview.current_section, None);
        assert_eq!(preview.effective_blocked_sites, vec!["example.com"]);
        let section = preview
            .next_section
            .as_deref()
            .expect("block preview should include next section");
        assert!(section.contains("# focustime-block-start"));
        assert!(section.contains("127.0.0.1 example.com"));
        assert!(section.contains("::1 www.example.com"));
        assert_eq!(preview.section_for_display(), Some(section));
    }

    #[test]
    fn preview_unblock_reports_current_section_and_change() {
        let blocker = SiteBlocker::new();
        let original = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n# focustime-block-end\n";

        let preview =
            blocker.preview_from_hosts_content("hosts", original, BlockingIntent::Unblock);

        assert_eq!(preview.action, BlockingPreviewAction::Unblock);
        assert!(preview.would_change);
        assert!(preview.next_section.is_none());
        let section = preview
            .current_section
            .as_deref()
            .expect("unblock preview should include current section");
        assert!(section.contains("# focustime-block-start"));
        assert!(section.contains("# focustime-block-end"));
        assert_eq!(preview.section_for_display(), Some(section));
    }

    #[test]
    fn preview_unblock_reports_all_existing_sections() {
        let blocker = SiteBlocker::new();
        let original = concat!(
            "127.0.0.1 localhost\n",
            "# focustime-block-start\n",
            "127.0.0.1 example.com\n",
            "# focustime-block-end\n",
            "# focustime-block-start\n",
            "127.0.0.1 github.com\n",
            "# focustime-block-end\n",
        );

        let preview =
            blocker.preview_from_hosts_content("hosts", original, BlockingIntent::Unblock);
        let section = preview
            .current_section
            .as_deref()
            .expect("unblock preview should include all current sections");

        assert_eq!(section.matches("# focustime-block-start").count(), 2);
        assert!(section.contains("127.0.0.1 example.com"));
        assert!(section.contains("127.0.0.1 github.com"));
    }

    #[test]
    fn preview_block_no_change_when_hosts_already_match() {
        let mut blocker = SiteBlocker::new();
        blocker.add_site("example.com".to_string());
        let original = blocker.build_blocked_hosts_content("127.0.0.1 localhost\n");

        let preview = blocker.preview_from_hosts_content("hosts", &original, BlockingIntent::Block);

        assert_eq!(preview.action, BlockingPreviewAction::NoChange);
        assert!(!preview.would_change);
        assert!(preview.next_section.is_some());
        assert!(preview.current_section.is_some());
    }

    #[test]
    fn block_rolls_back_to_original_after_post_replace_failure() {
        let path = temp_hosts_path("block-post-replace");
        let original = "127.0.0.1 localhost\n::1 localhost\n";
        fs::write(&path, original).expect("temp hosts file should be writable");

        let mut blocker = SiteBlocker::new();
        blocker.add_site("example.com".to_string());
        set_test_hosts_write_fail_steps(&[HostsWriteFailStep::AfterReplace]);

        let result = blocker.apply_hosts_block_to_path(&path);
        set_test_hosts_write_fail_steps(&[]);

        assert!(result.is_err());
        let restored = fs::read_to_string(&path).expect("hosts file should remain readable");
        assert_eq!(restored, original);
        assert!(!restored.contains(BLOCK_MARKER_START));
        assert!(!restored.contains(BLOCK_MARKER_END));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn block_failure_before_replace_keeps_original_content() {
        let path = temp_hosts_path("block-stage-write");
        let original = "127.0.0.1 localhost\n::1 localhost\n";
        fs::write(&path, original).expect("temp hosts file should be writable");

        let mut blocker = SiteBlocker::new();
        blocker.add_site("example.com".to_string());
        set_test_hosts_write_fail_steps(&[HostsWriteFailStep::StageWrite]);

        let result = blocker.apply_hosts_block_to_path(&path);
        set_test_hosts_write_fail_steps(&[]);

        assert!(result.is_err());
        let content = fs::read_to_string(&path).expect("hosts file should remain readable");
        assert_eq!(content, original);
        assert!(!content.contains(BLOCK_MARKER_START));
        assert!(!content.contains(BLOCK_MARKER_END));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn unblock_rolls_back_to_complete_section_after_post_replace_failure() {
        let path = temp_hosts_path("unblock-post-replace");
        let original = concat!(
            "127.0.0.1 localhost\n",
            "# focustime-block-start\n",
            "127.0.0.1 example.com\n",
            "::1 example.com\n",
            "# focustime-block-end\n",
            "::1 localhost\n",
        );
        fs::write(&path, original).expect("temp hosts file should be writable");

        let blocker = SiteBlocker::new();
        set_test_hosts_write_fail_steps(&[HostsWriteFailStep::AfterReplace]);

        let result = blocker.remove_hosts_block_from_path(&path);
        set_test_hosts_write_fail_steps(&[]);

        assert!(result.is_err());
        let restored = fs::read_to_string(&path).expect("hosts file should remain readable");
        assert_eq!(restored, original);
        assert_eq!(restored.matches(BLOCK_MARKER_START).count(), 1);
        assert_eq!(restored.matches(BLOCK_MARKER_END).count(), 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn rollback_failure_is_reported_when_restore_step_fails() {
        let path = temp_hosts_path("rollback-restore-error");
        let original = "127.0.0.1 localhost\n::1 localhost\n";
        fs::write(&path, original).expect("temp hosts file should be writable");

        let mut blocker = SiteBlocker::new();
        blocker.add_site("example.com".to_string());
        set_test_hosts_write_fail_steps(&[
            HostsWriteFailStep::AfterReplace,
            HostsWriteFailStep::RollbackRestore,
        ]);

        let result = blocker.apply_hosts_block_to_path(&path);
        set_test_hosts_write_fail_steps(&[]);

        let error = result.expect_err("rollback should fail when restore step is injected to fail");
        assert!(error.to_string().contains("rollback failed"));
        let content = fs::read_to_string(&path).expect("hosts file should remain readable");
        assert!(content.contains(BLOCK_MARKER_START));
        assert!(content.contains(BLOCK_MARKER_END));

        let _ = fs::remove_file(path);
    }
}
