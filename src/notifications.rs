#[cfg(not(test))]
use std::io::Write;
#[cfg(not(test))]
use std::process::Command;
#[cfg(not(test))]
use std::process::ExitStatus;
use std::thread;

use crate::config::NotificationConfig;
use crate::timer::TimerPhase;

const NOTIFICATION_TITLE: &str = "focustime";

pub struct PhaseNotifier {
    settings: NotificationConfig,
}

impl PhaseNotifier {
    pub fn new(settings: NotificationConfig) -> Self {
        Self { settings }
    }

    pub fn notify_phase_completion(
        &self,
        completed_phase: TimerPhase,
        next_phase: TimerPhase,
    ) -> Option<String> {
        if !self.settings.enabled {
            return None;
        }

        let message = transition_message(completed_phase, next_phase);
        let body = message.clone();
        let settings = self.settings;
        thread::spawn(move || {
            send_desktop_notification(NOTIFICATION_TITLE, &body);
            if settings.sound {
                play_sound_alert();
            }
        });
        Some(message)
    }
}

fn transition_message(completed_phase: TimerPhase, next_phase: TimerPhase) -> String {
    format!(
        "{} complete. Next up: {}.",
        completed_phase.label(),
        next_phase.label()
    )
}

#[cfg(not(test))]
fn send_desktop_notification(title: &str, body: &str) {
    #[cfg(target_os = "windows")]
    {
        use winrt_notification::{Duration, Toast};

        let toast_result = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(title)
            .text1(body)
            .duration(Duration::Short)
            .sound(None)
            .show();
        if toast_result.is_err() {
            let _ = Command::new("msg")
                .args(["*", &format!("{title}: {body}")])
                .status();
        }
    }

    #[cfg(target_os = "macos")]
    {
        let escaped_title = escape_applescript_literal(title);
        let escaped_body = escape_applescript_literal(body);
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            escaped_body, escaped_title
        );
        let _ = Command::new("osascript").args(["-e", &script]).status();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("notify-send").args([title, body]).status();
    }
}

#[cfg(test)]
fn send_desktop_notification(_title: &str, _body: &str) {}

#[cfg(target_os = "macos")]
fn escape_applescript_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(not(test))]
fn play_sound_alert() {
    #[cfg(target_os = "windows")]
    {
        if command_succeeded(
            Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "[console]::Beep(880,180)",
                ])
                .status(),
        ) {
            return;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if command_succeeded(
            Command::new("afplay")
                .arg("/System/Library/Sounds/Glass.aiff")
                .status(),
        ) {
            return;
        }
    }

    #[cfg(target_os = "linux")]
    {
        if command_succeeded(
            Command::new("paplay")
                .arg("/usr/share/sounds/freedesktop/stereo/complete.oga")
                .status(),
        ) {
            return;
        }
        if command_succeeded(
            Command::new("aplay")
                .args(["-q", "/usr/share/sounds/alsa/Front_Center.wav"])
                .status(),
        ) {
            return;
        }
    }

    // Last-resort fallback: many terminals ignore BEL, so this may be silent.
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(b"\x07");
    let _ = stderr.flush();
}

#[cfg(test)]
fn play_sound_alert() {}

#[cfg(not(test))]
fn command_succeeded(result: std::io::Result<ExitStatus>) -> bool {
    matches!(result, Ok(status) if status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifier_returns_none_when_disabled() {
        let notifier = PhaseNotifier::new(NotificationConfig {
            enabled: false,
            sound: true,
        });
        let message = notifier.notify_phase_completion(TimerPhase::Focus, TimerPhase::ShortBreak);
        assert!(message.is_none());
    }

    #[test]
    fn notifier_builds_transition_message() {
        let notifier = PhaseNotifier::new(NotificationConfig::default());
        let message = notifier.notify_phase_completion(TimerPhase::LongBreak, TimerPhase::Focus);
        assert_eq!(
            message.as_deref(),
            Some("Long Break complete. Next up: Focus.")
        );
    }
}
