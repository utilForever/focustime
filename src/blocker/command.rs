use std::io;
use std::process::Command;

use super::{
    BlockingBackendKind, BlockingIntent, BlockingPreview, BlockingPreviewAction, blocking_intent_id,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CommandBlockingBackend {
    pub(crate) block_command: String,
    pub(crate) unblock_command: String,
    pub(crate) diagnostics_command: String,
}

impl CommandBlockingBackend {
    pub(crate) fn normalized(&self) -> Self {
        Self {
            block_command: self.block_command.trim().to_string(),
            unblock_command: self.unblock_command.trim().to_string(),
            diagnostics_command: self.diagnostics_command.trim().to_string(),
        }
    }

    pub(crate) fn is_configured(&self) -> bool {
        !self.block_command.trim().is_empty() && !self.unblock_command.trim().is_empty()
    }
}

pub(super) fn command_backend_diagnostics(backend: &CommandBlockingBackend) -> io::Result<()> {
    if backend.diagnostics_command.trim().is_empty() {
        if backend.is_configured() {
            return Ok(());
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "command backend block/unblock commands are not fully configured",
        ));
    }
    run_shell_command(&backend.diagnostics_command)
}

pub(super) fn apply_command_backend(
    backend: &CommandBlockingBackend,
    intent: BlockingIntent,
    sites: &[String],
) -> io::Result<()> {
    let template = command_template_for_intent(backend, intent).trim();
    if template.is_empty() {
        return Err(missing_command_error(intent));
    }
    let rendered = render_command_template(template, sites);
    run_shell_command(&rendered)
}

pub(super) fn preview_from_command(
    backend: &CommandBlockingBackend,
    intent: BlockingIntent,
    sites: &[String],
) -> io::Result<BlockingPreview> {
    let template = command_template_for_intent(backend, intent)
        .trim()
        .to_string();
    if template.is_empty() {
        return Err(missing_command_error(intent));
    }

    let rendered = render_command_template(&template, sites);
    let action = match intent {
        BlockingIntent::Block => {
            if sites.is_empty() {
                BlockingPreviewAction::NoChange
            } else {
                BlockingPreviewAction::Block
            }
        }
        BlockingIntent::Unblock => BlockingPreviewAction::Unblock,
    };
    let effective_blocked_sites = if action == BlockingPreviewAction::Block {
        sites.to_vec()
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

fn command_template_for_intent(backend: &CommandBlockingBackend, intent: BlockingIntent) -> &str {
    match intent {
        BlockingIntent::Block => backend.block_command.as_str(),
        BlockingIntent::Unblock => backend.unblock_command.as_str(),
    }
}

fn missing_command_error(intent: BlockingIntent) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "command backend `{}` command is not configured",
            blocking_intent_id(intent)
        ),
    )
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
