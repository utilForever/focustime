use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestEnv {
    root: PathBuf,
}

impl TestEnv {
    fn new(label: &str) -> Self {
        let unique = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "focustime-cli-contract-{label}-{}-{now}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("failed to create temp test directory");
        Self { root }
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut command = Command::new(focustime_bin_path());
        command.args(args);
        command.current_dir(&self.root);
        command.env("APPDATA", &self.root);
        command.env("XDG_CONFIG_HOME", &self.root);
        command.env("HOME", &self.root);
        command
            .output()
            .expect("failed to run focustime integration command")
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn focustime_bin_path() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_focustime")
        .map(PathBuf::from)
        .expect("CARGO_BIN_EXE_focustime not set")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn status_json_success_emits_payload_on_stdout() {
    let env = TestEnv::new("status-json-success");
    let output = env.run(&["--status", "--json"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(payload.get("day").is_some());
    assert!(payload.get("live").is_some());
}

#[test]
fn parse_errors_in_json_mode_emit_usage_envelope() {
    let env = TestEnv::new("json-parse-error");
    let output = env.run(&["--status", "--unknown", "--json"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "usage");
    assert_eq!(payload["error"]["exit_code"], 2);
    assert!(
        payload["error"]["message"]
            .as_str()
            .expect("error message should be a string")
            .contains("Unknown option")
    );
}

#[test]
fn runtime_errors_in_json_mode_emit_runtime_envelope() {
    let env = TestEnv::new("json-runtime-error");
    let output = env.run(&["--resume", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "runtime");
    assert_eq!(payload["error"]["exit_code"], 1);
    assert!(
        payload["error"]["message"]
            .as_str()
            .expect("error message should be a string")
            .contains("Cannot resume")
    );
}

#[test]
fn text_parse_errors_still_use_stderr() {
    let env = TestEnv::new("text-parse-error");
    let output = env.run(&["--unknown"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stdout_text(&output).trim().is_empty());
    assert!(stderr_text(&output).contains("Unknown option"));
}
