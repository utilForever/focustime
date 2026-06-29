use crate::blocker::{
    BlockingBackendKind, BlockingPreviewAction, HostsFileDiagnostics, SiteBlocker,
};

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
    pub(crate) blocking_permissions: SetupCheck,
    pub(crate) hosts_write_capability: SetupCheck,
    pub(crate) deprecation_warnings: Vec<String>,
}

impl SetupDiagnostics {
    /// Collects setup diagnostics for local blocking and config health surfaces.
    pub(super) fn collect(blocker: &SiteBlocker, deprecation_warnings: Vec<String>) -> Self {
        let hosts_diagnostics = blocker.hosts_file_diagnostics();
        let blocking_permissions = blocking_permissions_check(&hosts_diagnostics);
        let hosts_write_capability = hosts_write_capability_check(&hosts_diagnostics);
        let hosts_file_path = hosts_diagnostics.path.clone();
        Self {
            hosts_file_path,
            blocking_permissions,
            hosts_write_capability,
            deprecation_warnings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockingPreviewSnapshot {
    pub(crate) backend: Option<BlockingBackendKind>,
    pub(crate) backend_target: Option<String>,
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
