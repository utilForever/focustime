use crate::app::{
    App, BlockingIntent, BlockingPreview, BlockingPreviewSnapshot, ExportedStatsFiles,
    HistoryFeedback, HistoryFeedbackLevel, PhaseNotifier, PlannerFeedback, PlannerFeedbackLevel,
    SetupDiagnostics, SiteFeedback, SiteFeedbackLevel,
};
use crate::integration::IntegrationLifecycleEvent;

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
        let context = self.history_kpi_export_context();
        match self.stats.export_to_dir_with_context(dir, &context) {
            Ok(paths) => self.set_history_feedback_for_export(paths),
            Err(e) => self
                .set_history_feedback(HistoryFeedbackLevel::Warning, format!("Export failed: {e}")),
        }
    }

    fn history_kpi_export_context(&self) -> crate::stats::HistoryKpiExportContext {
        let reference_day = chrono::Local::now().date_naive();
        crate::stats::HistoryKpiExportContext {
            reference_day,
            daily_goal: crate::stats::DailyGoalSnapshot {
                minutes: self.daily_goal.minutes,
                pomodoros: self.daily_goal.pomodoros,
            },
            weekly_goal: crate::stats::DailyGoalSnapshot {
                minutes: self.weekly_goal.minutes,
                pomodoros: self.weekly_goal.pomodoros,
            },
            monthly_goal: crate::stats::DailyGoalSnapshot {
                minutes: self.monthly_goal.minutes,
                pomodoros: self.monthly_goal.pomodoros,
            },
            carry_over_daily: self.goal_carry_over.daily,
            carry_over_weekly: self.goal_carry_over.weekly,
            carry_over_monthly: self.goal_carry_over.monthly,
            recurring_schedule: self.recurring_schedule.clone(),
            stats_retention: self.stats_retention,
            comparison_dimension: self.history_comparison_dimension,
            comparison_task_filter: self.history_task_filter.clone(),
            comparison_profile_filter: self.history_profile_filter,
            comparison_time_of_day_filter: self.history_time_of_day_filter,
        }
    }

    pub(super) fn sync_wakatime_tracking_for_state(&mut self) {
        let focus_running = self.focus_running_for_current_state();
        if let Err(error) = self.integrations.dispatch_lifecycle_event(
            IntegrationLifecycleEvent::FocusStateChanged { focus_running },
        ) {
            self.config_error = Some(error);
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
        self.setup_diagnostics = SetupDiagnostics::collect(
            &self.blocker,
            deprecation_warnings,
            self.feature_flags.integrations.is_enabled("wakatime"),
        );
        self.refresh_blocking_preview();
    }

    pub(super) fn refresh_blocking_preview(&mut self) {
        self.blocking_preview = match self.compute_blocking_preview() {
            Ok(preview) => BlockingPreviewSnapshot {
                backend: Some(preview.backend),
                backend_target: Some(preview.backend_target.clone()),
                attempted_backends: preview.attempted_backends.clone(),
                fallback_used: preview.fallback_used,
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
