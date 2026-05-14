use crate::app::{
    App, BlockingIntent, BlockingPreview, BlockingPreviewSnapshot, ExportedStatsFiles,
    HistoryFeedback, HistoryFeedbackLevel, PhaseNotifier, PlannerFeedback, PlannerFeedbackLevel,
    SetupDiagnostics, SiteFeedback, SiteFeedbackLevel,
};

impl App {
    pub(super) fn export_stats_history(&mut self) {
        let current_dir = match std::env::current_dir() {
            Ok(path) => path,
            Err(e) => {
                self.set_history_feedback(
                    HistoryFeedbackLevel::Warning,
                    format!("Export failed: cannot determine current directory ({e})"),
                );
                return;
            }
        };
        self.export_stats_to_dir(&current_dir);
    }

    pub(super) fn export_stats_to_dir(&mut self, dir: &std::path::Path) {
        self.history_feedback = None;
        match self.stats.export_to_dir(dir) {
            Ok(paths) => self.set_history_feedback_for_export(paths),
            Err(e) => self
                .set_history_feedback(HistoryFeedbackLevel::Warning, format!("Export failed: {e}")),
        }
    }

    pub(super) fn sync_wakatime_tracking_for_state(&mut self) {
        let focus_running = self.focus_running_for_current_state();
        if focus_running && !self.wakatime.is_tracking() {
            self.wakatime.on_focus_start();
        } else if !focus_running && self.wakatime.is_tracking() {
            self.wakatime.on_focus_stop();
        }
    }

    pub(super) fn set_block_error_from_result(&mut self, result: std::io::Result<()>) {
        match result {
            Ok(()) => self.block_error = None,
            Err(e) => self.block_error = Some(e.to_string()),
        }
    }

    pub(super) fn set_site_feedback(
        &mut self,
        level: SiteFeedbackLevel,
        message: impl Into<String>,
    ) {
        self.site_feedback = Some(SiteFeedback {
            level,
            message: message.into(),
        });
    }

    pub(super) fn set_planner_feedback(
        &mut self,
        level: PlannerFeedbackLevel,
        message: impl Into<String>,
    ) {
        self.planner_feedback = Some(PlannerFeedback {
            level,
            message: message.into(),
        });
    }

    pub(super) fn set_history_feedback(
        &mut self,
        level: HistoryFeedbackLevel,
        message: impl Into<String>,
    ) {
        self.history_feedback = Some(HistoryFeedback {
            level,
            message: message.into(),
        });
    }

    fn set_history_feedback_for_export(&mut self, paths: ExportedStatsFiles) {
        let export_dir = paths
            .json_path
            .parent()
            .or_else(|| paths.csv_path.parent())
            .unwrap_or_else(|| std::path::Path::new("."));
        let json_name = paths
            .json_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("focustime-stats.json");
        let csv_name = paths
            .csv_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("focustime-stats.csv");
        self.set_history_feedback(
            HistoryFeedbackLevel::Success,
            format!(
                "Exported to {}: JSON {}, CSV {}",
                export_dir.display(),
                json_name,
                csv_name
            ),
        );
    }

    pub(super) fn refresh_setup_diagnostics(&mut self) {
        let deprecation_warnings =
            crate::app::setup_deprecation_warnings(&self.config_deprecation_warnings);
        self.setup_diagnostics = SetupDiagnostics::collect(&self.blocker, deprecation_warnings);
        self.refresh_blocking_preview();
    }

    pub(super) fn refresh_blocking_preview(&mut self) {
        self.blocking_preview = match self.compute_blocking_preview() {
            Ok(preview) => BlockingPreviewSnapshot {
                action: preview.action,
                would_change: preview.would_change,
                effective_blocked_sites_count: preview.effective_blocked_sites.len(),
                section: preview.section_for_display().map(ToString::to_string),
                error: None,
            },
            Err(error) => BlockingPreviewSnapshot {
                error: Some(error.to_string()),
                ..BlockingPreviewSnapshot::default()
            },
        };
    }

    pub(super) fn compute_blocking_preview(&self) -> std::io::Result<BlockingPreview> {
        let intent = if self.should_block_for_current_state() {
            BlockingIntent::Block
        } else {
            BlockingIntent::Unblock
        };
        self.blocker.preview_hosts_update(intent)
    }

    pub(super) fn rebuild_notifier(&mut self) {
        self.notifier = PhaseNotifier::new(self.notification_settings);
    }
}
