use std::any::Any;

use crate::wakatime::{
    WakatimeHeartbeatMetadata, WakatimeRuntimeOptions, WakatimeRuntimeState, WakatimeTracker,
};

const WAKATIME_PLUGIN_NAME: &str = "wakatime";
const WAKATIME_CAPABILITIES: [IntegrationCapability; 5] = [
    IntegrationCapability::FocusLifecycleHooks,
    IntegrationCapability::FocusElapsedHooks,
    IntegrationCapability::TaskMetadataHooks,
    IntegrationCapability::AsyncPollingHooks,
    IntegrationCapability::NetworkAccess,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegrationId {
    Wakatime,
}

impl IntegrationId {
    pub fn config_name(self) -> &'static str {
        match self {
            Self::Wakatime => WAKATIME_PLUGIN_NAME,
        }
    }

    fn from_config_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            WAKATIME_PLUGIN_NAME => Some(Self::Wakatime),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegrationCapability {
    FocusLifecycleHooks,
    FocusElapsedHooks,
    TaskMetadataHooks,
    AsyncPollingHooks,
    NetworkAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntegrationDescriptor {
    pub id: IntegrationId,
    pub capabilities: &'static [IntegrationCapability],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationLifecycleEvent {
    Poll,
    FocusStateChanged { focus_running: bool },
    FocusElapsed { elapsed_secs: u64 },
}

impl IntegrationLifecycleEvent {
    fn required_capability(&self) -> IntegrationCapability {
        match self {
            Self::Poll => IntegrationCapability::AsyncPollingHooks,
            Self::FocusStateChanged { .. } => IntegrationCapability::FocusLifecycleHooks,
            Self::FocusElapsed { .. } => IntegrationCapability::FocusElapsedHooks,
        }
    }
}

trait IntegrationPlugin {
    fn descriptor(&self) -> IntegrationDescriptor;
    fn on_lifecycle_event(&mut self, event: &IntegrationLifecycleEvent) -> Result<(), String>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct LoadedIntegration {
    descriptor: IntegrationDescriptor,
    plugin: Box<dyn IntegrationPlugin>,
}

pub struct IntegrationRuntime {
    integrations: Vec<LoadedIntegration>,
}

impl IntegrationRuntime {
    pub fn load(
        enabled_plugins: &[String],
        wakatime_metadata: WakatimeHeartbeatMetadata,
        wakatime_runtime: WakatimeRuntimeOptions,
    ) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut integrations: Vec<LoadedIntegration> = Vec::new();
        for plugin_name in normalized_enabled_plugins(enabled_plugins) {
            let Some(plugin_id) = IntegrationId::from_config_name(&plugin_name) else {
                warnings.push(format!("unknown integration `{plugin_name}` ignored"));
                continue;
            };
            match plugin_id {
                IntegrationId::Wakatime => {
                    let plugin = WakatimeIntegrationPlugin::new(
                        wakatime_metadata.clone(),
                        wakatime_runtime.clone(),
                    );
                    let descriptor = plugin.descriptor();
                    integrations.push(LoadedIntegration {
                        descriptor,
                        plugin: Box::new(plugin),
                    });
                }
            }
        }
        (Self { integrations }, warnings)
    }

    pub fn dispatch_lifecycle_event(
        &mut self,
        event: IntegrationLifecycleEvent,
    ) -> Result<(), String> {
        let mut errors: Vec<String> = Vec::new();
        let required_capability = event.required_capability();
        for loaded in &mut self.integrations {
            if !loaded
                .descriptor
                .capabilities
                .contains(&required_capability)
            {
                errors.push(format!(
                    "integration `{}` does not declare required capability `{:?}` for hook `{:?}`",
                    loaded.descriptor.id.config_name(),
                    required_capability,
                    event
                ));
                continue;
            }
            if let Err(error) = loaded.plugin.on_lifecycle_event(&event) {
                errors.push(format!(
                    "integration `{}` lifecycle hook `{:?}` failed: {error}",
                    loaded.descriptor.id.config_name(),
                    event
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    pub fn set_wakatime_metadata(&mut self, metadata: WakatimeHeartbeatMetadata) {
        let Some(plugin) = self.wakatime_plugin_mut() else {
            return;
        };
        plugin.set_metadata(metadata);
    }

    pub fn wakatime_runtime_state(&self) -> WakatimeRuntimeState {
        self.wakatime_plugin()
            .map(|plugin| plugin.tracker().runtime_state())
            .unwrap_or(WakatimeRuntimeState::NotConfigured)
    }

    pub fn wakatime_last_successful_heartbeat_epoch_secs(&self) -> Option<u64> {
        self.wakatime_plugin()
            .and_then(|plugin| plugin.tracker().last_successful_heartbeat_epoch_secs())
    }

    #[cfg(test)]
    pub(crate) fn has_wakatime_enabled(&self) -> bool {
        self.wakatime_plugin().is_some()
    }

    fn wakatime_plugin(&self) -> Option<&WakatimeIntegrationPlugin> {
        self.integrations.iter().find_map(|loaded| {
            (loaded.descriptor.id == IntegrationId::Wakatime)
                .then(|| {
                    loaded
                        .plugin
                        .as_any()
                        .downcast_ref::<WakatimeIntegrationPlugin>()
                })
                .flatten()
        })
    }

    fn wakatime_plugin_mut(&mut self) -> Option<&mut WakatimeIntegrationPlugin> {
        self.integrations.iter_mut().find_map(|loaded| {
            (loaded.descriptor.id == IntegrationId::Wakatime)
                .then(|| {
                    loaded
                        .plugin
                        .as_any_mut()
                        .downcast_mut::<WakatimeIntegrationPlugin>()
                })
                .flatten()
        })
    }

    #[cfg(test)]
    pub(crate) fn replace_wakatime_tracker_for_tests(&mut self, tracker: WakatimeTracker) {
        let Some(plugin) = self.wakatime_plugin_mut() else {
            return;
        };
        plugin.tracker = tracker;
    }

    #[cfg(test)]
    pub(crate) fn wakatime_tracker_mut_for_tests(&mut self) -> Option<&mut WakatimeTracker> {
        self.wakatime_plugin_mut().map(|plugin| &mut plugin.tracker)
    }

    #[cfg(test)]
    pub(crate) fn wakatime_tracker_for_tests(&self) -> Option<&WakatimeTracker> {
        self.wakatime_plugin().map(|plugin| plugin.tracker())
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

struct WakatimeIntegrationPlugin {
    tracker: WakatimeTracker,
}

impl WakatimeIntegrationPlugin {
    fn new(metadata: WakatimeHeartbeatMetadata, runtime: WakatimeRuntimeOptions) -> Self {
        Self {
            tracker: WakatimeTracker::new_with_settings(metadata, runtime),
        }
    }

    fn tracker(&self) -> &WakatimeTracker {
        &self.tracker
    }

    fn set_metadata(&mut self, metadata: WakatimeHeartbeatMetadata) {
        self.tracker.set_heartbeat_metadata(metadata);
    }
}

impl IntegrationPlugin for WakatimeIntegrationPlugin {
    fn descriptor(&self) -> IntegrationDescriptor {
        IntegrationDescriptor {
            id: IntegrationId::Wakatime,
            capabilities: &WAKATIME_CAPABILITIES,
        }
    }

    fn on_lifecycle_event(&mut self, event: &IntegrationLifecycleEvent) -> Result<(), String> {
        match event {
            IntegrationLifecycleEvent::Poll => self.tracker.poll_events(),
            IntegrationLifecycleEvent::FocusStateChanged { focus_running } => {
                if *focus_running && !self.tracker.is_tracking() {
                    self.tracker.on_focus_start();
                } else if !*focus_running && self.tracker.is_tracking() {
                    self.tracker.on_focus_stop();
                }
            }
            IntegrationLifecycleEvent::FocusElapsed { elapsed_secs } => {
                self.tracker.tick_elapsed(*elapsed_secs);
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
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
    fn poll_event_applies_wakatime_async_updates() {
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

        runtime
            .dispatch_lifecycle_event(IntegrationLifecycleEvent::Poll)
            .expect("poll hook should succeed");

        assert_eq!(
            runtime.wakatime_runtime_state(),
            WakatimeRuntimeState::Error("HTTP 500".to_string())
        );
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
