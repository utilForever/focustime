use crate::cli::{
    BlockingPreviewAction, BlockingPreviewOutput, DiagnosticsBlockingPreviewOutput,
    DiagnosticsCommandOutput, DiagnosticsSetupOutput, RecurringScheduleConfig,
    ScheduleInspectionOutput, SetupCheck, SetupCheckLevel, SetupCheckOutput, SetupDiagnostics,
    format_schedule_conflict, inspect_schedule_conflicts_from_config,
};
use crate::config::{
    ConfigDoctorReport, ConfigHealthFinding, ConfigHealthStatus, ConfigMigrationReport,
};

pub(in crate::cli) fn build_schedule_inspection_output(
    schedule: &RecurringScheduleConfig,
) -> ScheduleInspectionOutput {
    let conflicts = inspect_schedule_conflicts_from_config(schedule)
        .into_iter()
        .map(|conflict| format_schedule_conflict(&conflict))
        .collect::<Vec<_>>();
    ScheduleInspectionOutput {
        conflict_count: conflicts.len(),
        conflicts,
    }
}

pub(in crate::cli) fn print_config_doctor_output(payload: &ConfigDoctorReport) {
    println!("Diagnostics workflow: {}", payload.action);
    print_config_health_section(payload);
    print_canonical_diagnostics_hint();
}

fn print_config_health_section(payload: &ConfigDoctorReport) {
    println!("Config health: {}", config_health_status_id(payload.status));
    println!(
        "Config path: {}",
        payload
            .config_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string())
    );
    println!(
        "Detected schema version: {}",
        payload
            .detected_schema_version
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!("Current schema version: {}", payload.current_schema_version);
    print_migration_steps(&payload.migration_steps);
    print_config_health_findings(&payload.findings);
}

pub(in crate::cli) fn print_config_migration_output(payload: &ConfigMigrationReport) {
    println!("Diagnostics workflow: {}", payload.action);
    print_config_migration_section(payload);
    print_canonical_diagnostics_hint();
}

fn print_config_migration_section(payload: &ConfigMigrationReport) {
    println!("Config migration guidance: {}", payload.action);
    println!(
        "Config path: {}",
        payload
            .config_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string())
    );
    println!(
        "Detected schema version: {}",
        payload
            .detected_schema_version
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!("Target schema version: {}", payload.target_schema_version);
    println!(
        "Migration changes detected: {}",
        if payload.changed { "yes" } else { "no" }
    );
    println!(
        "Migration applied: {}",
        if payload.applied { "yes" } else { "no" }
    );
    println!(
        "Migration status: {}",
        config_health_status_id(payload.status)
    );
    println!(
        "Backup path: {}",
        payload
            .backup_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    print_migration_steps(&payload.steps);
    print_config_health_findings(&payload.findings);
}

fn print_migration_steps(steps: &[crate::config::ConfigMigrationStepReport]) {
    if steps.is_empty() {
        println!("Migration steps: none");
        return;
    }
    println!("Migration steps:");
    for step in steps {
        println!(
            "  - v{} -> v{}: {}",
            step.from_schema_version, step.to_schema_version, step.summary
        );
    }
}

fn print_config_health_findings(findings: &[ConfigHealthFinding]) {
    if findings.is_empty() {
        println!("Findings: none");
        return;
    }
    println!("Findings:");
    for finding in findings {
        println!(
            "  - [{}:{}] {}",
            finding.code,
            config_health_severity_id(finding.severity),
            finding.message
        );
        println!("    remediation: {}", finding.remediation);
    }
}

fn config_health_status_id(status: ConfigHealthStatus) -> &'static str {
    match status {
        ConfigHealthStatus::Ok => "ok",
        ConfigHealthStatus::Warning => "warning",
        ConfigHealthStatus::Error => "error",
    }
}

fn config_health_severity_id(severity: crate::config::ConfigHealthSeverity) -> &'static str {
    match severity {
        crate::config::ConfigHealthSeverity::Warning => "warning",
        crate::config::ConfigHealthSeverity::Error => "error",
    }
}

/// Prints setup diagnostics checks, including WakaTime config and runtime status.
pub(in crate::cli) fn print_diagnostics_command_output(payload: &DiagnosticsCommandOutput) {
    println!("Diagnostics workflow: {}", payload.action);
    print_setup_diagnostics_section(&payload.setup);
    print_diagnostics_blocking_preview_section(&payload.blocking_preview);
    print_config_health_section(&payload.config_doctor);
    print_config_migration_section(&payload.config_migration);
}

fn print_setup_diagnostics_section(payload: &DiagnosticsSetupOutput) {
    println!("Setup diagnostics:");
    println!("Hosts file: {}", payload.hosts_file_path);
    println!(
        "Backend policy: {} (order: {})",
        payload.backend_policy, payload.backend_order
    );
    print_diagnostics_check("Backend selection", &payload.backend_selection);
    print_diagnostics_check("Command backend", &payload.command_backend);
    print_diagnostics_check("Blocking permissions", &payload.blocking_permissions);
    print_diagnostics_check("Hosts write capability", &payload.hosts_write_capability);
    print_diagnostics_check("WakaTime config", &payload.wakatime_config);
    print_diagnostics_check("WakaTime runtime", &payload.wakatime_runtime);
    if payload.deprecation_warnings.is_empty() {
        println!("Deprecation warnings: none");
    } else {
        println!("Deprecation warnings:");
        for warning in &payload.deprecation_warnings {
            println!("  - {warning}");
        }
    }
}

fn print_diagnostics_blocking_preview_section(payload: &DiagnosticsBlockingPreviewOutput) {
    println!("Blocking preview: {}", payload.status);
    if let Some(error) = payload.error.as_deref() {
        println!("Preview error: {error}");
        return;
    }
    if let Some(preview) = payload.preview.as_ref() {
        print_blocking_preview_fields(preview);
    }
}

fn print_canonical_diagnostics_hint() {
    println!(
        "Canonical diagnostics: run `focustime --diagnostics` for setup, blocking preview, config health, and migration guidance."
    );
}

fn print_diagnostics_check(label: &str, check: &SetupCheckOutput) {
    println!("{label}: {} ({})", check.message, check.level);
}

/// Builds the serializable diagnostics payload used by text and JSON output.
pub(in crate::cli) fn build_diagnostics_command_output(
    diagnostics: &SetupDiagnostics,
    config_doctor: ConfigDoctorReport,
    config_migration: ConfigMigrationReport,
    blocking_preview: DiagnosticsBlockingPreviewOutput,
) -> DiagnosticsCommandOutput {
    DiagnosticsCommandOutput {
        action: "diagnostics",
        setup: DiagnosticsSetupOutput {
            hosts_file_path: diagnostics.hosts_file_path.clone(),
            backend_policy: diagnostics.backend_policy.clone(),
            backend_order: diagnostics.backend_order.clone(),
            backend_selection: setup_check_output(&diagnostics.backend_selection),
            command_backend: setup_check_output(&diagnostics.command_backend),
            blocking_permissions: setup_check_output(&diagnostics.blocking_permissions),
            hosts_write_capability: setup_check_output(&diagnostics.hosts_write_capability),
            wakatime_config: setup_check_output(&diagnostics.wakatime_config),
            wakatime_runtime: setup_check_output(&diagnostics.wakatime_runtime),
            deprecation_warnings: diagnostics.deprecation_warnings.clone(),
        },
        blocking_preview,
        config_doctor,
        config_migration,
    }
}

pub(in crate::cli) fn build_diagnostics_blocking_preview_output(
    preview: &crate::blocker::BlockingPreview,
) -> DiagnosticsBlockingPreviewOutput {
    DiagnosticsBlockingPreviewOutput {
        status: "ok",
        error: None,
        preview: Some(build_blocking_preview_output(preview)),
    }
}

pub(in crate::cli) fn build_diagnostics_blocking_preview_error(
    error: impl Into<String>,
) -> DiagnosticsBlockingPreviewOutput {
    DiagnosticsBlockingPreviewOutput {
        status: "error",
        error: Some(error.into()),
        preview: None,
    }
}

fn print_blocking_preview_fields(payload: &BlockingPreviewOutput) {
    println!(
        "Backend: {} (target: {})",
        payload.backend, payload.backend_target
    );
    if !payload.attempted_backends.is_empty() {
        println!(
            "Attempted backends: {}",
            payload.attempted_backends.join(" -> ")
        );
    }
    println!(
        "Fallback used: {}",
        if payload.fallback_used { "yes" } else { "no" }
    );
    println!("Hosts file: {}", payload.hosts_file_path);
    println!(
        "Preview action: {} (changes: {})",
        payload.action,
        if payload.would_change { "yes" } else { "no" }
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
    if !payload.effective_blocked_sites.is_empty() {
        println!("Sites: {}", payload.effective_blocked_sites.join(", "));
    }
    if let Some(section) = payload.section.as_deref() {
        println!("Section preview:");
        print!("{section}");
    } else {
        println!("Section preview: none");
    }
}

fn build_blocking_preview_output(
    preview: &crate::blocker::BlockingPreview,
) -> BlockingPreviewOutput {
    let action = match preview.action {
        BlockingPreviewAction::Block => "block",
        BlockingPreviewAction::Unblock => "unblock",
        BlockingPreviewAction::NoChange => "no_change",
    };
    BlockingPreviewOutput {
        backend: preview.backend.id(),
        backend_target: preview.backend_target.clone(),
        attempted_backends: preview
            .attempted_backends
            .iter()
            .map(|backend| backend.id())
            .collect(),
        fallback_used: preview.fallback_used,
        hosts_file_path: preview.hosts_file_path.clone(),
        action,
        would_change: preview.would_change,
        effective_blocked_sites_count: preview.effective_blocked_sites.len(),
        effective_blocked_sites: preview.effective_blocked_sites.clone(),
        section: preview.section_for_display().map(ToString::to_string),
    }
}

fn setup_check_output(check: &SetupCheck) -> SetupCheckOutput {
    SetupCheckOutput {
        level: setup_check_level_id(check.level),
        message: check.message.clone(),
    }
}

fn setup_check_level_id(level: SetupCheckLevel) -> &'static str {
    match level {
        SetupCheckLevel::Ok => "ok",
        SetupCheckLevel::Warning => "warning",
    }
}
