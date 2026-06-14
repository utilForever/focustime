use crate::wakatime::{
    WakatimeHeartbeatMetadata, WakatimeRuntimeOptions, WakatimeRuntimeState, WakatimeTracker,
};

const WAKATIME_PLUGIN_NAME: &str = "wakatime";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum IntegrationId {
    Wakatime,
}

impl IntegrationId {
    fn from_config_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            WAKATIME_PLUGIN_NAME => Some(Self::Wakatime),
            _ => None,
        }
    }
}

pub(crate) struct IntegrationRuntime {
    wakatime: Option<WakatimeIntegration>,
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
            let Some(plugin_id) = IntegrationId::from_config_name(&plugin_name) else {
                warnings.push(format!("unknown integration `{plugin_name}` ignored"));
                continue;
            };
            match plugin_id {
                IntegrationId::Wakatime => {
                    wakatime = Some(WakatimeIntegration::new(
                        wakatime_metadata.clone(),
                        wakatime_runtime.clone(),
                    ));
                }
            }
        }
        (Self { wakatime }, warnings)
    }

    pub(crate) fn poll_wakatime_events(&mut self) {
        let Some(wakatime) = self.wakatime.as_mut() else {
            return;
        };
        wakatime.poll_events();
    }

    pub(crate) fn set_wakatime_tracking(&mut self, focus_running: bool) {
        let Some(wakatime) = self.wakatime.as_mut() else {
            return;
        };
        wakatime.set_tracking(focus_running);
    }

    pub(crate) fn advance_wakatime(&mut self, elapsed_secs: u64) {
        let Some(wakatime) = self.wakatime.as_mut() else {
            return;
        };
        wakatime.advance(elapsed_secs);
    }

    pub(crate) fn set_wakatime_metadata(&mut self, metadata: WakatimeHeartbeatMetadata) {
        let Some(wakatime) = self.wakatime.as_mut() else {
            return;
        };
        wakatime.set_metadata(metadata);
    }

    pub(crate) fn wakatime_runtime_state(&self) -> WakatimeRuntimeState {
        self.wakatime
            .as_ref()
            .map(|wakatime| wakatime.tracker().runtime_state())
            .unwrap_or(WakatimeRuntimeState::NotConfigured)
    }

    pub(crate) fn wakatime_last_successful_heartbeat_epoch_secs(&self) -> Option<u64> {
        self.wakatime
            .as_ref()
            .and_then(|wakatime| wakatime.tracker().last_successful_heartbeat_epoch_secs())
    }

    #[cfg(test)]
    pub(crate) fn has_wakatime_enabled(&self) -> bool {
        self.wakatime.is_some()
    }

    #[cfg(test)]
    pub(crate) fn replace_wakatime_tracker_for_tests(&mut self, tracker: WakatimeTracker) {
        let Some(wakatime) = self.wakatime.as_mut() else {
            return;
        };
        wakatime.tracker = tracker;
    }

    #[cfg(test)]
    pub(crate) fn wakatime_tracker_mut_for_tests(&mut self) -> Option<&mut WakatimeTracker> {
        self.wakatime.as_mut().map(|wakatime| &mut wakatime.tracker)
    }

    #[cfg(test)]
    pub(crate) fn wakatime_tracker_for_tests(&self) -> Option<&WakatimeTracker> {
        self.wakatime.as_ref().map(WakatimeIntegration::tracker)
    }
}

struct WakatimeIntegration {
    tracker: WakatimeTracker,
}

impl WakatimeIntegration {
    fn new(metadata: WakatimeHeartbeatMetadata, runtime: WakatimeRuntimeOptions) -> Self {
        Self {
            tracker: WakatimeTracker::new_with_settings(metadata, runtime),
        }
    }

    fn tracker(&self) -> &WakatimeTracker {
        &self.tracker
    }

    fn poll_events(&mut self) {
        self.tracker.poll_events();
    }

    fn set_tracking(&mut self, focus_running: bool) {
        if focus_running && !self.tracker.is_tracking() {
            self.tracker.on_focus_start();
        } else if !focus_running && self.tracker.is_tracking() {
            self.tracker.on_focus_stop();
        }
    }

    fn advance(&mut self, elapsed_secs: u64) {
        self.tracker.tick_elapsed(elapsed_secs);
    }

    fn set_metadata(&mut self, metadata: WakatimeHeartbeatMetadata) {
        self.tracker.set_heartbeat_metadata(metadata);
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
