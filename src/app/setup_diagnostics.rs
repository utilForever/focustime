use crate::blocker::{
    BlockingBackendKind, BlockingPreviewAction, HostsFileDiagnostics, SiteBlocker,
};
use crate::wakatime::{WakatimeConfigStatus, WakatimeRuntimeState, WakatimeTracker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupCheckLevel {
    Ok,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SetupCheck {
    pub(crate) level: SetupCheckLevel,
    pub(crate) message: String,
}

impl SetupCheck {
    fn ok(message: impl Into<String>) -> Self {
        Self {
            level: SetupCheckLevel::Ok,
            message: message.into(),
        }
    }

    fn warning(message: impl Into<String>) -> Self {
        Self {
            level: SetupCheckLevel::Warning,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SetupDiagnostics {
    pub(crate) hosts_file_path: String,
    pub(crate) backend_policy: String,
    pub(crate) backend_order: String,
    pub(crate) backend_selection: SetupCheck,
    pub(crate) blocking_permissions: SetupCheck,
    pub(crate) hosts_write_capability: SetupCheck,
    pub(crate) command_backend: SetupCheck,
    pub(crate) wakatime_config: SetupCheck,
    pub(crate) wakatime_runtime: SetupCheck,
    pub(crate) deprecation_warnings: Vec<String>,
}

impl SetupDiagnostics {
    /// Collects setup diagnostics, including WakaTime queue/retry runtime status.
    pub(super) fn collect(
        blocker: &SiteBlocker,
        deprecation_warnings: Vec<String>,
        wakatime_integration_enabled: bool,
        wakatime_runtime_state: WakatimeRuntimeState,
    ) -> Self {
        let hosts_diagnostics = blocker.hosts_file_diagnostics();
        let backend_status = blocker.backend_status();
        let blocking_permissions = blocking_permissions_check(&hosts_diagnostics);
        let hosts_write_capability = hosts_write_capability_check(&hosts_diagnostics);
        let hosts_file_path = hosts_diagnostics.path.clone();
        let backend_policy = backend_status.policy.id().to_string();
        let backend_order = backend_status
            .order
            .iter()
            .map(|backend| backend.id())
            .collect::<Vec<_>>()
            .join(" -> ");
        let backend_selection = backend_selection_check(
            backend_status.last_backend,
            backend_status.fallback_used,
            backend_status.last_error.as_deref(),
        );
        let command_backend = command_backend_check(blocker);
        let wakatime_config = if wakatime_integration_enabled {
            let wakatime_diagnostics = WakatimeTracker::config_diagnostics();
            match wakatime_diagnostics.status {
                WakatimeConfigStatus::Configured => SetupCheck::ok(wakatime_diagnostics.detail),
                WakatimeConfigStatus::MissingConfigFile
                | WakatimeConfigStatus::MissingApiKey
                | WakatimeConfigStatus::UnreadableConfig
                | WakatimeConfigStatus::HomeDirectoryUnavailable => {
                    SetupCheck::warning(wakatime_diagnostics.detail)
                }
            }
        } else {
            SetupCheck::ok("Disabled by WakaTime integration configuration.".to_string())
        };
        let wakatime_runtime =
            wakatime_runtime_check(wakatime_integration_enabled, &wakatime_runtime_state);
        Self {
            hosts_file_path,
            backend_policy,
            backend_order,
            backend_selection,
            blocking_permissions,
            hosts_write_capability,
            command_backend,
            wakatime_config,
            wakatime_runtime,
            deprecation_warnings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockingPreviewSnapshot {
    pub(crate) backend: Option<BlockingBackendKind>,
    pub(crate) backend_target: Option<String>,
    pub(crate) attempted_backends: Vec<BlockingBackendKind>,
    pub(crate) fallback_used: bool,
    pub(crate) action: BlockingPreviewAction,
    pub(crate) would_change: bool,
    pub(crate) effective_blocked_sites_count: usize,
    pub(crate) section: Option<String>,
    pub(crate) error: Option<String>,
}

impl Default for BlockingPreviewSnapshot {
    fn default() -> Self {
        Self {
            backend: None,
            backend_target: None,
            attempted_backends: Vec::new(),
            fallback_used: false,
            action: BlockingPreviewAction::NoChange,
            would_change: false,
            effective_blocked_sites_count: 0,
            section: None,
            error: None,
        }
    }
}

fn permission_remediation_guidance() -> &'static str {
    if cfg!(target_os = "windows") {
        "Run focustime from an Administrator terminal, then open [d] Setup and press [r] Refresh."
    } else {
        "Run focustime with elevated privileges (e.g. sudo), verify hosts-file permissions, then press [r] Refresh."
    }
}

fn blocking_permissions_check(hosts_diagnostics: &HostsFileDiagnostics) -> SetupCheck {
    if hosts_diagnostics.can_write() {
        SetupCheck::ok("Ready: hosts file can be opened for write access")
    } else {
        let reason = hosts_diagnostics
            .write_error
            .as_deref()
            .unwrap_or("unknown write error");
        SetupCheck::warning(format!(
            "Blocked: write permission unavailable ({reason}). {}",
            permission_remediation_guidance()
        ))
    }
}

fn backend_selection_check(
    last_backend: Option<BlockingBackendKind>,
    fallback_used: bool,
    last_error: Option<&str>,
) -> SetupCheck {
    if let Some(error) = last_error {
        return SetupCheck::warning(format!("Blocked: backend selection failed ({error})"));
    }
    if let Some(backend) = last_backend {
        if fallback_used {
            return SetupCheck::warning(format!(
                "Fallback active: using `{}` backend after primary backend failure",
                backend.id()
            ));
        }
        return SetupCheck::ok(format!("Ready: using `{}` backend", backend.id()));
    }
    SetupCheck::warning(
        "Awaiting first block/unblock operation to confirm selected backend".to_string(),
    )
}

fn command_backend_check(blocker: &SiteBlocker) -> SetupCheck {
    match blocker.command_backend_diagnostics() {
        Ok(()) => SetupCheck::ok("Ready: command backend diagnostics passed"),
        Err(error) => SetupCheck::warning(format!(
            "Blocked: command backend unavailable ({error}). Configure commands or use hosts backend."
        )),
    }
}

/// Converts the current WakaTime runtime state into a user-facing setup check.
fn wakatime_runtime_check(
    wakatime_integration_enabled: bool,
    state: &WakatimeRuntimeState,
) -> SetupCheck {
    if !wakatime_integration_enabled {
        return SetupCheck::ok("Disabled by WakaTime integration configuration.");
    }
    match state {
        WakatimeRuntimeState::NotConfigured => SetupCheck::warning(
            "Not configured: WakaTime heartbeats and offline queue replay are inactive",
        ),
        WakatimeRuntimeState::Idle => SetupCheck::ok("Idle: no queued WakaTime heartbeats"),
        WakatimeRuntimeState::Tracking => SetupCheck::ok("Tracking: no queued WakaTime heartbeats"),
        WakatimeRuntimeState::Sending => SetupCheck::ok("Sending: heartbeat request in flight"),
        WakatimeRuntimeState::Queued { pending } => SetupCheck::warning(format!(
            "Queued: {pending} WakaTime heartbeat{} pending replay",
            plural_suffix(*pending)
        )),
        WakatimeRuntimeState::Replaying { pending } => SetupCheck::warning(format!(
            "Replaying: {pending} WakaTime heartbeat{} pending, oldest first",
            plural_suffix(*pending)
        )),
        WakatimeRuntimeState::Retrying {
            attempt,
            max_attempts,
            next_backoff_secs,
            error,
        } => SetupCheck::warning(format!(
            "Retrying: attempt {attempt}/{max_attempts}, next retry in {next_backoff_secs}s ({error})"
        )),
        WakatimeRuntimeState::Error(error) => SetupCheck::warning(format!("Error: {error}")),
    }
}

/// Returns the suffix needed to pluralize heartbeat status messages.
fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn hosts_write_capability_check(hosts_diagnostics: &HostsFileDiagnostics) -> SetupCheck {
    let can_read = hosts_diagnostics.can_read();
    let can_write = hosts_diagnostics.can_write();
    match (
        can_read,
        can_write,
        hosts_diagnostics.read_error.as_deref(),
        hosts_diagnostics.write_error.as_deref(),
    ) {
        (true, true, _, _) => SetupCheck::ok("Ready: hosts file is readable and writable"),
        (false, true, Some(read_error), _) => SetupCheck::warning(format!(
            "Blocked: cannot read hosts file ({read_error}). {}",
            permission_remediation_guidance()
        )),
        (true, false, _, Some(write_error)) => SetupCheck::warning(format!(
            "Blocked: cannot write hosts file ({write_error}). {}",
            permission_remediation_guidance()
        )),
        (false, false, Some(read_error), Some(write_error)) => SetupCheck::warning(format!(
            "Blocked: read error ({read_error}); write error ({write_error}). {}",
            permission_remediation_guidance()
        )),
        _ => SetupCheck::warning(format!(
            "Blocked: hosts access diagnostics unavailable. {}",
            permission_remediation_guidance()
        )),
    }
}
