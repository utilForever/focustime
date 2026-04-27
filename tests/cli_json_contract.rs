use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
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

    fn run_watch(&self, args: &[&str], runtime: Duration) -> Output {
        let mut command = Command::new(focustime_bin_path());
        command.args(args);
        command.current_dir(&self.root);
        command.env("APPDATA", &self.root);
        command.env("XDG_CONFIG_HOME", &self.root);
        command.env("HOME", &self.root);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .expect("failed to spawn focustime integration command");
        thread::sleep(runtime);
        let _ = child.kill();
        child
            .wait_with_output()
            .expect("failed to collect watch command output")
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
    assert!(payload.get("focus_intention").is_some());
    assert!(payload.get("task_note").is_some());
    assert!(payload.get("goal").is_some());
    assert!(payload.get("weekly_goal").is_some());
    assert!(payload.get("monthly_goal").is_some());
    assert!(payload["goal"].get("carry_over").is_some());
    assert!(payload["weekly_goal"].get("carry_over").is_some());
    assert!(payload["monthly_goal"].get("carry_over").is_some());
    assert!(payload.get("selected_task_goal").is_some());
    assert!(payload.get("live").is_some());
    assert!(payload["live"].get("focus_intention").is_some());
    assert!(payload["live"].get("task_note").is_some());
}

#[test]
fn task_goal_json_sets_and_reads_per_task_target() {
    let env = TestEnv::new("task-goal-json");

    let select_output = env.run(&["--task", "Docs", "--json"]);
    assert_eq!(select_output.status.code(), Some(0));
    assert!(stderr_text(&select_output).trim().is_empty());

    let set_output = env.run(&["--task-goal", "Docs:120,4", "--json"]);
    assert_eq!(set_output.status.code(), Some(0));
    assert!(stderr_text(&set_output).trim().is_empty());
    let set_payload: Value =
        serde_json::from_slice(&set_output.stdout).expect("stdout should be JSON");
    assert_eq!(set_payload["updated"], true);
    assert_eq!(set_payload["task_label"], "Docs");
    assert_eq!(set_payload["configured"], true);
    assert_eq!(set_payload["minutes_target"], 120);
    assert_eq!(set_payload["pomodoros_target"], 4);

    let read_output = env.run(&["--task-goal", "Docs", "--json"]);
    assert_eq!(read_output.status.code(), Some(0));
    assert!(stderr_text(&read_output).trim().is_empty());
    let read_payload: Value =
        serde_json::from_slice(&read_output.stdout).expect("stdout should be JSON");
    assert_eq!(read_payload["updated"], false);
    assert_eq!(read_payload["task_label"], "Docs");
    assert_eq!(read_payload["configured"], true);
    assert_eq!(read_payload["minutes_target"], 120);
    assert_eq!(read_payload["pomodoros_target"], 4);
}

#[test]
fn status_watch_json_streams_multiple_snapshots() {
    let env = TestEnv::new("status-watch-json");
    let output = env.run_watch(
        &["--status", "--watch=1", "--json"],
        Duration::from_millis(2400),
    );

    assert!(stderr_text(&output).trim().is_empty());

    let stdout = stdout_text(&output);
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(
        lines.len() >= 2,
        "expected at least two JSON snapshots, got {} lines: {:?}",
        lines.len(),
        lines
    );

    for line in lines {
        let payload: Value =
            serde_json::from_str(line).expect("snapshot line should be valid JSON");
        assert!(payload.get("day").is_some());
        assert!(payload.get("live").is_some());
    }
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

#[test]
fn blocking_preview_json_emits_payload_on_stdout() {
    let env = TestEnv::new("blocking-preview-json");
    let output = env.run(&["--blocking-preview", "--json"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(payload.get("hosts_file_path").is_some());
    assert!(payload.get("action").is_some());
    assert!(payload.get("would_change").is_some());
    assert!(payload.get("effective_blocked_sites_count").is_some());
    assert!(payload.get("effective_blocked_sites").is_some());
    assert!(payload.get("section").is_some());
}
