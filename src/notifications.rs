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

#[cfg(any(target_os = "windows", test))]
fn notify_windows_with_fallback(
    title: &str,
    body: &str,
    mut show_toast: impl FnMut(&str, &str) -> bool,
    mut send_fallback: impl FnMut(&str, &str),
) {
    if !show_toast(title, body) {
        send_fallback(title, body);
    }
}

#[cfg(not(test))]
fn send_desktop_notification(title: &str, body: &str) {
    #[cfg(target_os = "windows")]
    {
        use winrt_toast_reborn::content::audio::{Audio, Sound};
        use winrt_toast_reborn::{Toast, ToastDuration, ToastManager};

        notify_windows_with_fallback(
            title,
            body,
            |toast_title, toast_body| {
                let manager = ToastManager::new(ToastManager::POWERSHELL_AUM_ID);
                let mut toast = Toast::new();
                toast
                    .text1(toast_title)
                    .text2(toast_body)
                    .duration(ToastDuration::Short)
                    .audio(Audio::new(Sound::None));
                manager.show(&toast).is_ok()
            },
            |fallback_title, fallback_body| {
                let _ = Command::new("msg")
                    .args(["*", &format!("{fallback_title}: {fallback_body}")])
                    .status();
            },
        );
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

#[cfg(all(target_os = "macos", not(test)))]
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
    fn windows_notification_fallback_runs_when_toast_fails() {
        let mut toast_inputs = Vec::new();
        let mut fallback_inputs = Vec::new();

        notify_windows_with_fallback(
            "focustime",
            "Focus complete. Next up: Short Break.",
            |title, body| {
                toast_inputs.push((title.to_string(), body.to_string()));
                false
            },
            |title, body| {
                fallback_inputs.push((title.to_string(), body.to_string()));
            },
        );

        assert_eq!(
            toast_inputs,
            vec![(
                "focustime".to_string(),
                "Focus complete. Next up: Short Break.".to_string(),
            )]
        );
        assert_eq!(
            fallback_inputs,
            vec![(
                "focustime".to_string(),
                "Focus complete. Next up: Short Break.".to_string(),
            )]
        );
    }

    #[test]
    fn windows_notification_does_not_fallback_when_toast_succeeds() {
        let cases = [
            ("focustime", "Focus complete. Next up: Short Break."),
            ("alert", "Long Break complete. Next up: Focus."),
        ];

        for (title, body) in cases {
            let mut toast_inputs = Vec::new();
            let mut fallback_calls = 0;

            notify_windows_with_fallback(
                title,
                body,
                |toast_title, toast_body| {
                    toast_inputs.push((toast_title.to_string(), toast_body.to_string()));
                    true
                },
                |_fallback_title, _fallback_body| {
                    fallback_calls += 1;
                },
            );

            assert_eq!(toast_inputs, vec![(title.to_string(), body.to_string())]);
            assert_eq!(fallback_calls, 0);
        }
    }

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
