use crate::wakatime::{
    WakatimeHeartbeatMetadata, WakatimeRuntimeOptions, WakatimeRuntimeState, WakatimeTracker,
};

const WAKATIME_PLUGIN_NAME: &str = "wakatime";

pub(crate) struct IntegrationRuntime {
    wakatime: Option<WakatimeTracker>,
}

impl IntegrationRuntime {
    pub(crate) fn load(
        enabled_plugins: &[String],
        wakatime_metadata: WakatimeHeartbeatMetadata,
        wakatime_runtime: WakatimeRuntimeOptions,
    ) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut wakatime = None;
        for plugin_name in normalized_enabled_plugins(enabled_plugins) {
            if plugin_name == WAKATIME_PLUGIN_NAME {
                wakatime = Some(WakatimeTracker::new_with_settings(
                    wakatime_metadata.clone(),
                    wakatime_runtime.clone(),
                ));
            } else {
                warnings.push(format!("unknown integration `{plugin_name}` ignored"));
            }
        }
        (Self { wakatime }, warnings)
    }

    pub(crate) fn poll_wakatime_events(&mut self) {
        if let Some(wakatime) = self.wakatime.as_mut() {
            wakatime.poll_events();
        }
    }

    pub(crate) fn set_wakatime_tracking(&mut self, focus_running: bool) {
        if let Some(wakatime) = self.wakatime.as_mut() {
            wakatime.set_focus_tracking(focus_running);
        }
    }

    pub(crate) fn advance_wakatime(&mut self, elapsed_secs: u64) {
        if let Some(wakatime) = self.wakatime.as_mut() {
            wakatime.tick_elapsed(elapsed_secs);
        }
    }

    pub(crate) fn set_wakatime_metadata(&mut self, metadata: WakatimeHeartbeatMetadata) {
        if let Some(wakatime) = self.wakatime.as_mut() {
            wakatime.set_heartbeat_metadata(metadata);
        }
    }

    pub(crate) fn wakatime_runtime_state(&self) -> WakatimeRuntimeState {
        self.wakatime
            .as_ref()
            .map(WakatimeTracker::runtime_state)
            .unwrap_or(WakatimeRuntimeState::NotConfigured)
    }

    pub(crate) fn wakatime_last_successful_heartbeat_epoch_secs(&self) -> Option<u64> {
        self.wakatime
            .as_ref()
            .and_then(WakatimeTracker::last_successful_heartbeat_epoch_secs)
    }

    #[cfg(test)]
    pub(crate) fn has_wakatime_enabled(&self) -> bool {
        self.wakatime.is_some()
    }

    #[cfg(test)]
    pub(crate) fn replace_wakatime_tracker_for_tests(&mut self, tracker: WakatimeTracker) {
        if let Some(wakatime) = self.wakatime.as_mut() {
            *wakatime = tracker;
        }
    }

    #[cfg(test)]
    pub(crate) fn wakatime_tracker_mut_for_tests(&mut self) -> Option<&mut WakatimeTracker> {
        self.wakatime.as_mut()
    }

    #[cfg(test)]
    pub(crate) fn wakatime_tracker_for_tests(&self) -> Option<&WakatimeTracker> {
        self.wakatime.as_ref()
    }
}

fn normalized_enabled_plugins(input: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::new();
    for plugin in input {
        let trimmed = plugin.trim().to_ascii_lowercase();
        if trimmed.is_empty() || normalized.contains(&trimmed) {
            continue;
        }
        normalized.push(trimmed);
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_plugin_name_is_reported_and_ignored() {
        let (runtime, warnings) = IntegrationRuntime::load(
            &["wakatime".to_string(), "custom".to_string()],
            WakatimeHeartbeatMetadata::default(),
            WakatimeRuntimeOptions::default(),
        );

        assert!(runtime.has_wakatime_enabled());
        assert_eq!(warnings, vec!["unknown integration `custom` ignored"]);
    }

    #[test]
    fn poll_wakatime_events_applies_async_updates() {
        let (mut runtime, _warnings) = IntegrationRuntime::load(
            &[WAKATIME_PLUGIN_NAME.to_string()],
            WakatimeHeartbeatMetadata::default(),
            WakatimeRuntimeOptions::default(),
        );
        runtime.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());
        runtime
            .wakatime_tracker_mut_for_tests()
            .expect("wakatime integration should be loaded")
            .push_failed_event_for_tests("HTTP 500");

        runtime.poll_wakatime_events();

        assert_eq!(
            runtime.wakatime_runtime_state(),
            WakatimeRuntimeState::Error("HTTP 500".to_string())
        );
    }

    #[test]
    fn disabled_wakatime_runtime_ignores_supported_hooks() {
        let (mut runtime, warnings) = IntegrationRuntime::load(
            &[],
            WakatimeHeartbeatMetadata::default(),
            WakatimeRuntimeOptions::default(),
        );

        assert!(warnings.is_empty());
        assert!(!runtime.has_wakatime_enabled());
        assert_eq!(
            runtime.wakatime_runtime_state(),
            WakatimeRuntimeState::NotConfigured
        );
        runtime.poll_wakatime_events();
        runtime.set_wakatime_tracking(true);
        runtime.advance_wakatime(60);
        assert_eq!(
            runtime.wakatime_runtime_state(),
            WakatimeRuntimeState::NotConfigured
        );
    }

    #[test]
    fn enabled_wakatime_runtime_delegates_supported_focus_hooks() {
        let (mut runtime, _warnings) = IntegrationRuntime::load(
            &[WAKATIME_PLUGIN_NAME.to_string()],
            WakatimeHeartbeatMetadata::default(),
            WakatimeRuntimeOptions::default(),
        );
        runtime.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());

        runtime.set_wakatime_tracking(true);

        let tracker = runtime
            .wakatime_tracker_for_tests()
            .expect("wakatime integration should be loaded");
        assert!(tracker.is_tracking());
        assert_eq!(tracker.runtime_state(), WakatimeRuntimeState::Sending);

        runtime
            .wakatime_tracker_mut_for_tests()
            .expect("wakatime integration should be loaded")
            .push_sent_event_for_tests();
        runtime.poll_wakatime_events();
        runtime.advance_wakatime(60);

        let tracker = runtime
            .wakatime_tracker_for_tests()
            .expect("wakatime integration should be loaded");
        assert!(tracker.is_tracking());
        assert_eq!(tracker.runtime_state(), WakatimeRuntimeState::Tracking);

        runtime.set_wakatime_tracking(false);

        let tracker = runtime
            .wakatime_tracker_for_tests()
            .expect("wakatime integration should be loaded");
        assert!(!tracker.is_tracking());
        assert_eq!(tracker.runtime_state(), WakatimeRuntimeState::Idle);
    }

    #[test]
    fn metadata_updates_are_forwarded_to_wakatime_plugin() {
        let (mut runtime, _warnings) = IntegrationRuntime::load(
            &[WAKATIME_PLUGIN_NAME.to_string()],
            WakatimeHeartbeatMetadata::default(),
            WakatimeRuntimeOptions::default(),
        );
        runtime.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());

        runtime.set_wakatime_metadata(WakatimeHeartbeatMetadata {
            project: "Docs".to_string(),
            language: "Markdown".to_string(),
        });

        assert_eq!(
            runtime
                .wakatime_tracker_mut_for_tests()
                .expect("wakatime integration should be loaded")
                .heartbeat_metadata_for_tests(),
            WakatimeHeartbeatMetadata {
                project: "Docs".to_string(),
                language: "Markdown".to_string(),
            }
        );
    }
}
