use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use serde_json::{Value, json};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize)]
struct DaemonStateFile {
    pid: u32,
    host: String,
    port: u16,
    token: String,
    started_at_epoch_secs: i64,
}

struct DaemonGuard<'a> {
    env: &'a TestEnv,
}

impl Drop for DaemonGuard<'_> {
    fn drop(&mut self) {
        let _ = self.env.run(&["--daemon-stop", "--json"]);
    }
}

fn start_daemon(env: &TestEnv) -> DaemonGuard<'_> {
    let output = env.run(&["--daemon-start", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["action"], "daemon-start");
    assert!(payload["daemon"]["pid"].as_u64().is_some_and(|pid| pid > 0));
    assert_eq!(payload["daemon"]["host"], "127.0.0.1");
    assert!(
        payload["daemon"]["port"]
            .as_u64()
            .is_some_and(|port| port > 0)
    );
    DaemonGuard { env }
}

fn load_daemon_state(env: &TestEnv) -> DaemonStateFile {
    let state_path = env.app_data_dir().join("daemon-state.toml");
    let content = fs::read_to_string(state_path).expect("failed to read daemon state file");
    toml::from_str::<DaemonStateFile>(&content).expect("failed to parse daemon state file")
}

fn daemon_get_json(state: &DaemonStateFile, path: &str) -> Value {
    let url = format!("http://{}:{}{path}", state.host, state.port);
    let auth_header = format!("Bearer {}", state.token);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();
    let response = agent
        .get(&url)
        .header("Authorization", &auth_header)
        .call()
        .expect("daemon GET request failed");
    response
        .into_body()
        .read_json::<Value>()
        .expect("daemon GET response body should be valid JSON")
}

fn daemon_post_json(state: &DaemonStateFile, path: &str, payload: Value) -> Value {
    let url = format!("http://{}:{}{path}", state.host, state.port);
    let auth_header = format!("Bearer {}", state.token);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();
    let response = agent
        .post(&url)
        .header("Authorization", &auth_header)
        .send_json(payload)
        .expect("daemon POST request failed");
    response
        .into_body()
        .read_json::<Value>()
        .expect("daemon POST response body should be valid JSON")
}

fn wait_until_daemon_stopped(env: &TestEnv) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let output = env.run(&["--daemon-status", "--json"]);
        assert_eq!(output.status.code(), Some(0));
        assert!(stderr_text(&output).trim().is_empty());
        let payload: Value =
            serde_json::from_slice(&output.stdout).expect("daemon status should be JSON");
        if payload["running"] == Value::Bool(false) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("daemon did not stop before timeout");
}

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

    fn app_data_dir(&self) -> PathBuf {
        self.root.join("focustime")
    }

    fn stats_canonical_dir(&self) -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            self.root.join("localappdata").join("focustime")
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.root.join(".state").join("focustime")
        }
    }

    fn legacy_stats_path(&self) -> PathBuf {
        self.app_data_dir().join("stats.toml")
    }

    fn canonical_stats_path(&self) -> PathBuf {
        self.stats_canonical_dir().join("stats.toml")
    }

    fn run(&self, args: &[&str]) -> Output {
        let capture_id = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let stdout_path = self.root.join(format!("command-stdout-{capture_id}.log"));
        let stderr_path = self.root.join(format!("command-stderr-{capture_id}.log"));
        let stdout_file = fs::File::create(&stdout_path).expect("failed to create stdout capture");
        let stderr_file = fs::File::create(&stderr_path).expect("failed to create stderr capture");
        let mut command = Command::new(focustime_bin_path());
        command.args(args);
        command.current_dir(&self.root);
        command.env("APPDATA", &self.root);
        command.env("LOCALAPPDATA", self.root.join("localappdata"));
        command.env("XDG_CONFIG_HOME", &self.root);
        command.env("XDG_STATE_HOME", self.root.join(".state"));
        command.env("XDG_DATA_HOME", self.root.join(".data"));
        command.env("HOME", &self.root);
        command.stdout(Stdio::from(stdout_file));
        command.stderr(Stdio::from(stderr_file));
        let status = command
            .status()
            .expect("failed to run focustime integration command");
        let stdout = fs::read(&stdout_path).expect("failed to read stdout capture");
        let stderr = fs::read(&stderr_path).expect("failed to read stderr capture");
        let _ = fs::remove_file(stdout_path);
        let _ = fs::remove_file(stderr_path);
        Output {
            status,
            stdout,
            stderr,
        }
    }

    fn spawn_watch(&self, args: &[&str]) -> Child {
        let mut command = Command::new(focustime_bin_path());
        command.args(args);
        command.current_dir(&self.root);
        command.env("APPDATA", &self.root);
        command.env("LOCALAPPDATA", self.root.join("localappdata"));
        command.env("XDG_CONFIG_HOME", &self.root);
        command.env("XDG_STATE_HOME", self.root.join(".state"));
        command.env("XDG_DATA_HOME", self.root.join(".data"));
        command.env("HOME", &self.root);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        command
            .spawn()
            .expect("failed to spawn focustime integration command")
    }

    fn run_watch(&self, args: &[&str], runtime: Duration) -> Output {
        let mut child = self.spawn_watch(args);
        thread::sleep(runtime);
        let _ = child.kill();
        child
            .wait_with_output()
            .expect("failed to collect watch command output")
    }

    #[cfg(unix)]
    fn run_watch_with_sigint(&self, args: &[&str], runtime: Duration) -> Output {
        let child = self.spawn_watch(args);
        thread::sleep(runtime);
        let interrupt_status = Command::new("kill")
            .arg("-INT")
            .arg(child.id().to_string())
            .status()
            .expect("failed to send SIGINT to watch command");
        assert!(
            interrupt_status.success(),
            "kill -INT should succeed, got status: {interrupt_status}"
        );
        child
            .wait_with_output()
            .expect("failed to collect watch command output after SIGINT")
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

fn assert_json_error_contract(
    output: &Output,
    expected_exit_code: i32,
    expected_kind: &str,
) -> Value {
    assert_eq!(output.status.code(), Some(expected_exit_code));
    assert!(stderr_text(output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], expected_kind);
    assert_eq!(payload["error"]["exit_code"], expected_exit_code);
    assert!(
        payload["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.trim().is_empty()),
        "error message should be a non-empty string"
    );
    payload
}

fn write_recovery_snapshot(env: &TestEnv, content: &str) {
    let app_data_dir = env.app_data_dir();
    fs::create_dir_all(&app_data_dir).expect("failed to create app data directory");
    fs::write(app_data_dir.join("session-recovery.toml"), content)
        .expect("failed to write recovery snapshot");
}

fn write_stats_snapshot(path: &std::path::Path, day: &str, pomodoros: u32, focused_seconds: u64) {
    let content = format!(
        "[daily.\"{day}\"]\npomodoros_completed = {pomodoros}\nfocused_seconds = {focused_seconds}\n"
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create stats parent directory");
    }
    fs::write(path, content).expect("failed to write stats snapshot");
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
    assert!(payload.get("temporary_allowlist_active_count").is_some());
    assert!(
        payload
            .get("temporary_allowlist_next_expiry_remaining_secs")
            .is_some()
    );
    assert!(
        payload
            .get("temporary_allowlist_next_expiry_epoch_secs")
            .is_some()
    );
    assert!(payload.get("temporary_allowlist_active").is_some());
    assert!(payload["goal"].get("carry_over").is_some());
    assert!(payload["weekly_goal"].get("carry_over").is_some());
    assert!(payload["monthly_goal"].get("carry_over").is_some());
    assert!(payload.get("selected_task_goal").is_some());
    assert!(payload.get("focus_score").is_some());
    assert!(payload["focus_score"].get("available").is_some());
    assert!(
        payload["focus_score"]
            .get("consistency_score_pct")
            .is_some()
    );
    assert!(payload["focus_score"].get("completion_score_pct").is_some());
    assert!(payload["focus_score"].get("focus_score_pct").is_some());
    assert!(payload.get("live").is_some());
    assert!(payload["live"].get("focus_intention").is_some());
    assert!(payload["live"].get("task_note").is_some());
    assert!(payload.get("comparison").is_some());
    assert!(payload["comparison"].get("dimension").is_some());
    assert!(payload["comparison"].get("task_filter").is_some());
    assert!(payload["comparison"].get("profile_filter").is_some());
    assert!(payload["comparison"].get("time_of_day_filter").is_some());
    assert!(payload["comparison"].get("limit").is_some());
    assert!(payload["comparison"].get("rows").is_some());
}

#[test]
fn temporary_allowlist_add_json_is_reflected_in_status_json() {
    let env = TestEnv::new("temporary-allowlist-json");

    let add_output = env.run(&["--allowlist-site-add-temporary=reddit.com=120s", "--json"]);
    assert_eq!(add_output.status.code(), Some(0));
    assert!(stderr_text(&add_output).trim().is_empty());
    let add_payload: Value = serde_json::from_slice(&add_output.stdout).expect("stdout JSON");
    assert_eq!(add_payload["action"], "allowlist-site-add-temporary");
    assert_eq!(add_payload["updated"], true);
    assert_eq!(add_payload["added"], 1);
    assert_eq!(add_payload["active"][0]["site"], "reddit.com");

    let status_output = env.run(&["--status", "--json"]);
    assert_eq!(status_output.status.code(), Some(0));
    assert!(stderr_text(&status_output).trim().is_empty());
    let status_payload: Value = serde_json::from_slice(&status_output.stdout).expect("stdout JSON");
    assert_eq!(status_payload["temporary_allowlist_active_count"], 1);
    assert_eq!(
        status_payload["temporary_allowlist_next_expiry_remaining_secs"],
        status_payload["temporary_allowlist_active"][0]["remaining_secs"]
    );
    assert_eq!(
        status_payload["temporary_allowlist_next_expiry_epoch_secs"],
        status_payload["temporary_allowlist_active"][0]["expires_at_epoch_secs"]
    );
    assert_eq!(
        status_payload["temporary_allowlist_active"][0]["site"],
        "reddit.com"
    );
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
fn task_goal_json_reads_unconfigured_selected_task_goal() {
    let env = TestEnv::new("task-goal-json-unconfigured");

    let select_output = env.run(&["--task", "Docs", "--json"]);
    assert_eq!(select_output.status.code(), Some(0));
    assert!(stderr_text(&select_output).trim().is_empty());

    let read_output = env.run(&["--task-goal", "Docs", "--json"]);
    assert_eq!(read_output.status.code(), Some(0));
    assert!(stderr_text(&read_output).trim().is_empty());
    let read_payload: Value =
        serde_json::from_slice(&read_output.stdout).expect("stdout should be JSON");
    assert_eq!(read_payload["updated"], false);
    assert_eq!(read_payload["task_label"], "Docs");
    assert_eq!(read_payload["configured"], false);
    assert_eq!(read_payload["minutes_target"], 0);
    assert_eq!(read_payload["pomodoros_target"], 0);
    assert_eq!(read_payload["focused_minutes"], 0);
    assert_eq!(read_payload["pomodoros_completed"], 0);
    assert_eq!(read_payload["met"], false);
}

#[test]
fn session_metadata_json_does_not_fallback_from_selected_task_label() {
    let env = TestEnv::new("metadata-json-read-no-fallback");

    let select_output = env.run(&["--task", "Docs", "--json"]);
    assert_eq!(select_output.status.code(), Some(0));
    assert!(stderr_text(&select_output).trim().is_empty());

    let read_output = env.run(&["--focus-intention", "--json"]);
    assert_eq!(read_output.status.code(), Some(0));
    assert!(stderr_text(&read_output).trim().is_empty());

    let payload: Value =
        serde_json::from_slice(&read_output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["action"], "focus-intention");
    assert_eq!(payload["updated"], false);
    assert!(payload["focus_intention"].is_null());
    assert!(payload["task_note"].is_null());
    assert_eq!(payload["timer"]["selected_task_label"], "Docs");
}

#[test]
fn session_metadata_json_set_and_read_updates_recovery_backed_state() {
    let env = TestEnv::new("metadata-json-set-read");
    write_recovery_snapshot(
        &env,
        r#"phase = "focus"
status = "running"
remaining_secs = 1200
pomodoros_completed = 2
selected_task_label = "Docs"
focus_intention = "Write docs"
task_note = "Draft outline"
selected_profile = "classic"
"#,
    );

    let set_note_output = env.run(&["--task-note", "Capture blockers", "--json"]);
    assert_eq!(set_note_output.status.code(), Some(0));
    assert!(stderr_text(&set_note_output).trim().is_empty());
    let set_note_payload: Value =
        serde_json::from_slice(&set_note_output.stdout).expect("stdout should be JSON");
    assert_eq!(set_note_payload["action"], "task-note");
    assert_eq!(set_note_payload["updated"], true);
    assert_eq!(set_note_payload["focus_intention"], "Write docs");
    assert_eq!(set_note_payload["task_note"], "Capture blockers");

    let set_focus_output = env.run(&["--focus-intention=Deep Work", "--json"]);
    assert_eq!(set_focus_output.status.code(), Some(0));
    assert!(stderr_text(&set_focus_output).trim().is_empty());
    let set_focus_payload: Value =
        serde_json::from_slice(&set_focus_output.stdout).expect("stdout should be JSON");
    assert_eq!(set_focus_payload["action"], "focus-intention");
    assert_eq!(set_focus_payload["updated"], true);
    assert_eq!(set_focus_payload["focus_intention"], "Deep Work");
    assert_eq!(set_focus_payload["task_note"], "Capture blockers");

    let read_output = env.run(&["--task-note", "--json"]);
    assert_eq!(read_output.status.code(), Some(0));
    assert!(stderr_text(&read_output).trim().is_empty());
    let read_payload: Value = serde_json::from_slice(&read_output.stdout).expect("stdout JSON");
    assert_eq!(read_payload["action"], "task-note");
    assert_eq!(read_payload["updated"], false);
    assert_eq!(read_payload["focus_intention"], "Deep Work");
    assert_eq!(read_payload["task_note"], "Capture blockers");
}

#[test]
fn session_metadata_set_json_requires_active_focus_session() {
    let env = TestEnv::new("metadata-json-set-requires-active-session");
    let output = env.run(&["--focus-intention", "Write docs", "--json"]);

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
            .contains("focus session is not active or paused")
    );
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

#[cfg(unix)]
#[test]
fn status_watch_json_sigint_exits_cleanly_without_partial_snapshot() {
    let env = TestEnv::new("status-watch-json-sigint");
    let output = env.run_watch_with_sigint(
        &["--status", "--watch=1", "--json"],
        Duration::from_millis(1400),
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let stdout = stdout_text(&output);
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(
        !lines.is_empty(),
        "expected at least one snapshot before SIGINT, got none"
    );

    for line in lines {
        let payload: Value =
            serde_json::from_str(line).expect("SIGINT watch output should stay valid NDJSON");
        assert!(payload.get("day").is_some());
        assert!(payload.get("live").is_some());
    }
}

#[test]
fn backup_and_restore_json_round_trip_config_and_stats_files() {
    let env = TestEnv::new("backup-restore-json");
    let backup_dir = env.root.join("backup");
    let backup_arg = format!("--backup={}", backup_dir.display());
    let restore_arg = format!("--restore={}", backup_dir.display());

    let set_goal_output = env.run(&["--goal=120,4", "--json"]);
    assert_eq!(set_goal_output.status.code(), Some(0));
    assert!(stderr_text(&set_goal_output).trim().is_empty());

    let set_task_goal_output = env.run(&["--task-goal=Docs:90,3", "--json"]);
    assert_eq!(set_task_goal_output.status.code(), Some(0));
    assert!(stderr_text(&set_task_goal_output).trim().is_empty());

    let backup_output = env.run(&[backup_arg.as_str(), "--json"]);
    assert_eq!(backup_output.status.code(), Some(0));
    assert!(stderr_text(&backup_output).trim().is_empty());
    let backup_payload: Value =
        serde_json::from_slice(&backup_output.stdout).expect("stdout should be JSON");
    assert!(backup_payload.get("backup_dir").is_some());
    assert!(backup_payload.get("config_backup_path").is_some());
    assert!(backup_payload.get("stats_backup_path").is_some());
    assert!(backup_dir.join("config.toml").is_file());
    assert!(backup_dir.join("stats.toml").is_file());

    let mutate_goal_output = env.run(&["--goal=15,1", "--json"]);
    assert_eq!(mutate_goal_output.status.code(), Some(0));
    assert!(stderr_text(&mutate_goal_output).trim().is_empty());

    let mutate_task_goal_output = env.run(&["--task-goal=Docs:30,1", "--json"]);
    assert_eq!(mutate_task_goal_output.status.code(), Some(0));
    assert!(stderr_text(&mutate_task_goal_output).trim().is_empty());

    let restore_output = env.run(&[restore_arg.as_str(), "--json"]);
    assert_eq!(restore_output.status.code(), Some(0));
    assert!(stderr_text(&restore_output).trim().is_empty());
    let restore_payload: Value =
        serde_json::from_slice(&restore_output.stdout).expect("stdout should be JSON");
    assert!(restore_payload.get("restore_dir").is_some());
    assert!(restore_payload.get("config_restored_path").is_some());
    assert!(restore_payload.get("stats_restored_path").is_some());

    let goal_output = env.run(&["--goal", "--json"]);
    assert_eq!(goal_output.status.code(), Some(0));
    let goal_payload: Value = serde_json::from_slice(&goal_output.stdout).unwrap();
    assert_eq!(goal_payload["minutes_target"], 120);
    assert_eq!(goal_payload["pomodoros_target"], 4);

    let task_goal_output = env.run(&["--task-goal", "Docs", "--json"]);
    assert_eq!(task_goal_output.status.code(), Some(0));
    let task_goal_payload: Value = serde_json::from_slice(&task_goal_output.stdout).unwrap();
    assert_eq!(task_goal_payload["minutes_target"], 90);
    assert_eq!(task_goal_payload["pomodoros_target"], 3);
}

#[test]
fn feature_inventory_json_exports_scored_report_artifacts() {
    let env = TestEnv::new("feature-inventory-json");
    let report_dir = env.root.join("reports");
    let inventory_arg = format!("--feature-inventory={}", report_dir.display());

    let output = env.run(&[inventory_arg.as_str(), "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(payload.get("export_dir").is_some());
    assert!(payload.get("json_path").is_some());
    assert!(payload.get("markdown_path").is_some());
    assert!(
        payload["total_features"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
    assert!(payload.get("keep_count").is_some());
    assert!(payload.get("merge_count").is_some());
    assert!(payload.get("remove_count").is_some());

    assert!(report_dir.join("FEATURE_INVENTORY.json").is_file());
    assert!(report_dir.join("FEATURE_INVENTORY.md").is_file());

    let inventory_payload: Value = serde_json::from_slice(
        &fs::read(report_dir.join("FEATURE_INVENTORY.json"))
            .expect("failed to read exported feature inventory JSON"),
    )
    .expect("feature inventory export JSON should parse");
    assert_eq!(inventory_payload["schema_version"], 5);
    assert_eq!(
        inventory_payload["cleanup_signal_support"]["deprecated_cli_flag"],
        "--usage-signals"
    );
    assert_eq!(
        inventory_payload["cleanup_signal_support"]["replacement_cli_flag"],
        "--feature-inventory"
    );
    assert!(
        inventory_payload["cleanup_signal_support"]["retained_dimensions"]
            .as_array()
            .is_some_and(|dimensions| dimensions.iter().any(|dimension| dimension == "commands"))
    );
}

#[test]
fn task_goal_json_writes_stats_to_canonical_path_only() {
    let env = TestEnv::new("stats-canonical-only");

    let output = env.run(&["--task-goal=Docs:90,3", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let canonical_stats_path = env.canonical_stats_path();
    let legacy_stats_path = env.legacy_stats_path();
    assert!(canonical_stats_path.is_file());
    assert!(!legacy_stats_path.is_file());
}

#[test]
fn status_json_uses_canonical_stats_without_legacy_fallback() {
    let env = TestEnv::new("stats-canonical-read");

    let baseline_status = env.run(&["--status", "--json"]);
    assert_eq!(baseline_status.status.code(), Some(0));
    assert!(stderr_text(&baseline_status).trim().is_empty());
    let baseline_payload: Value =
        serde_json::from_slice(&baseline_status.stdout).expect("stdout should be JSON");
    let day = baseline_payload["day"]
        .as_str()
        .expect("day should be a string")
        .to_string();

    let canonical_stats_path = env.canonical_stats_path();
    write_stats_snapshot(&canonical_stats_path, &day, 3, 1800);

    let canonical_status = env.run(&["--status", "--json"]);
    assert_eq!(canonical_status.status.code(), Some(0));
    assert!(stderr_text(&canonical_status).trim().is_empty());
    let canonical_payload: Value =
        serde_json::from_slice(&canonical_status.stdout).expect("stdout should be JSON");
    assert_eq!(canonical_payload["today"]["pomodoros_completed"], 3);

    fs::remove_file(&canonical_stats_path).expect("failed to remove canonical stats file");
    let fallback_status = env.run(&["--status", "--json"]);
    assert_eq!(fallback_status.status.code(), Some(0));
    assert!(stderr_text(&fallback_status).trim().is_empty());
    let fallback_payload: Value =
        serde_json::from_slice(&fallback_status.stdout).expect("stdout should be JSON");
    assert_eq!(fallback_payload["today"]["pomodoros_completed"], 0);
}

#[test]
fn backup_json_copies_raw_files_even_when_malformed() {
    let env = TestEnv::new("backup-raw-malformed");
    let app_data_dir = env.app_data_dir();
    fs::create_dir_all(&app_data_dir).expect("failed to create app data directory");
    let seed_output = env.run(&["--task", "Docs", "--json"]);
    assert_eq!(seed_output.status.code(), Some(0));
    assert!(stderr_text(&seed_output).trim().is_empty());

    let malformed_config = "this is not valid toml !!!\n";
    fs::write(app_data_dir.join("config.toml"), malformed_config).expect("failed to write config");

    let backup_dir = env.root.join("backup");
    let backup_arg = format!("--backup={}", backup_dir.display());
    let output = env.run(&[backup_arg.as_str(), "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(payload.get("backup_dir").is_some());
    assert_eq!(
        fs::read_to_string(backup_dir.join("config.toml")).unwrap(),
        malformed_config
    );
    assert!(backup_dir.join("stats.toml").is_file());
}

#[test]
fn restore_json_fails_when_backup_is_missing_stats_file() {
    let env = TestEnv::new("restore-missing-stats");
    let backup_dir = env.root.join("broken-backup");
    fs::create_dir_all(&backup_dir).expect("failed to create backup directory");
    fs::write(backup_dir.join("config.toml"), "focus_secs = 1500\n")
        .expect("failed to write config backup file");
    let restore_arg = format!("--restore={}", backup_dir.display());

    let output = env.run(&[restore_arg.as_str(), "--json"]);
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
            .contains("missing `stats.toml`")
    );
}

#[test]
fn start_json_success_emits_timer_payload_on_stdout() {
    let env = TestEnv::new("start-json-success");
    let select_output = env.run(&["--task", "Docs", "--json"]);
    assert_eq!(select_output.status.code(), Some(0));
    assert!(stderr_text(&select_output).trim().is_empty());

    let output = env.run(&["--start", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["action"], "start");
    assert_eq!(payload["timer"]["phase"], "focus");
    assert_eq!(payload["timer"]["status"], "running");
    assert_eq!(payload["timer"]["selected_task_label"], "Docs");
}

#[test]
fn start_then_status_json_reports_running_recovery_state_across_processes() {
    let env = TestEnv::new("start-status-cross-process");
    let select_output = env.run(&["--task", "Docs", "--json"]);
    assert_eq!(select_output.status.code(), Some(0));
    assert!(stderr_text(&select_output).trim().is_empty());

    let start_output = env.run(&["--start", "--json"]);
    assert_eq!(start_output.status.code(), Some(0));
    assert!(stderr_text(&start_output).trim().is_empty());

    let status_output = env.run(&["--status", "--json"]);
    assert_eq!(status_output.status.code(), Some(0));
    assert!(stderr_text(&status_output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&status_output.stdout).expect("stdout JSON");
    assert_eq!(payload["live"]["state_source"], "recovery");
    assert_eq!(payload["live"]["in_progress"], true);
    assert_eq!(payload["live"]["phase"], "focus");
    assert_eq!(payload["live"]["status"], "running");
    assert_eq!(payload["live"]["selected_task_label"], "Docs");
    assert!(
        payload["live"]["remaining_secs"]
            .as_u64()
            .expect("remaining secs should be u64")
            > 0
    );
}

#[test]
fn status_json_reconciles_elapsed_recovery_snapshot_from_disk() {
    let env = TestEnv::new("status-json-reconcile-elapsed");
    write_recovery_snapshot(
        &env,
        r#"phase = "focus"
status = "running"
remaining_secs = 1
pomodoros_completed = 0
selected_task_label = "Docs"
focus_intention = "Write docs"
task_note = "API section"
selected_profile = "classic"
captured_at_epoch_secs = 0
"#,
    );

    let status_output = env.run(&["--status", "--json"]);
    assert_eq!(status_output.status.code(), Some(0));
    assert!(stderr_text(&status_output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&status_output.stdout).expect("stdout JSON");
    assert_eq!(payload["live"]["state_source"], "recovery");
    assert_eq!(payload["live"]["in_progress"], true);
    assert_eq!(payload["live"]["phase"], "short-break");
    assert_eq!(payload["live"]["status"], "idle");
    assert_eq!(payload["live"]["pomodoros_completed"], 1);
    assert_eq!(payload["live"]["selected_task_label"], "Docs");
    assert!(payload["live"]["focus_intention"].is_null());
    assert!(payload["live"]["task_note"].is_null());
    assert!(
        payload["live"]["remaining_secs"]
            .as_u64()
            .expect("remaining secs should be u64")
            > 0
    );
}

#[test]
fn start_json_requires_selected_task_label() {
    let env = TestEnv::new("start-json-requires-task");
    let output = env.run(&["--start", "--json"]);

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
            .contains("select a task label first")
    );
}

#[test]
fn parse_errors_in_json_mode_emit_usage_envelope() {
    let env = TestEnv::new("json-parse-error");
    let output = env.run(&["--status", "--unknown", "--json"]);

    let payload = assert_json_error_contract(&output, 2, "usage");
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

    let payload = assert_json_error_contract(&output, 1, "runtime");
    assert!(
        payload["error"]["message"]
            .as_str()
            .expect("error message should be a string")
            .contains("Cannot resume")
    );
}

#[test]
fn parse_errors_in_json_mode_preserve_contract_across_parser_stages() {
    let env = TestEnv::new("json-parse-contract-matrix");
    let cases: [(&[&str], &str); 5] = [
        (&["--status", "--unknown", "--json"], "Unknown option"),
        (
            &["--schedule-set", "--json"],
            "`--schedule-set` requires a JSON payload",
        ),
        (
            &["--status", "--watch=0", "--json"],
            "positive whole number of seconds",
        ),
        (&["--task=", "--json"], "`--task=` requires a task label"),
        (
            &["--status", "--watch", "--watch=2", "--json"],
            "can only be specified once",
        ),
    ];

    for (args, message_fragment) in cases {
        let output = env.run(args);
        let payload = assert_json_error_contract(&output, 2, "usage");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("error message should be a string")
                .contains(message_fragment),
            "expected message fragment `{message_fragment}` for args {:?}, got {}",
            args,
            payload["error"]["message"]
        );
    }
}

#[test]
fn runtime_errors_in_json_mode_preserve_contract_across_command_families() {
    let env = TestEnv::new("json-runtime-contract-matrix");
    let cases: [(&[&str], &str); 4] = [
        (&["--start", "--json"], "select a task label first"),
        (&["--pause", "--json"], "Cannot pause"),
        (&["--resume", "--json"], "Cannot resume"),
        (
            &["--focus-intention", "Write docs", "--json"],
            "focus session is not active or paused",
        ),
    ];

    for (args, message_fragment) in cases {
        let output = env.run(args);
        let payload = assert_json_error_contract(&output, 1, "runtime");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("error message should be a string")
                .contains(message_fragment),
            "expected message fragment `{message_fragment}` for args {:?}, got {}",
            args,
            payload["error"]["message"]
        );
    }
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
fn text_runtime_errors_still_use_stderr() {
    let env = TestEnv::new("text-runtime-error");
    let output = env.run(&["--resume"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_text(&output).trim().is_empty());
    assert!(stderr_text(&output).contains("Cannot resume"));
}

#[test]
fn blocking_preview_json_emits_payload_on_stdout() {
    let env = TestEnv::new("blocking-preview-json");
    let output = env.run(&["--blocking-preview", "--json"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["deprecated"], true);
    assert!(
        payload["replacement"]
            .as_str()
            .is_some_and(|replacement| replacement.contains("--diagnostics"))
    );
    assert!(payload.get("hosts_file_path").is_some());
    assert!(payload.get("action").is_some());
    assert!(payload.get("would_change").is_some());
    assert!(payload.get("effective_blocked_sites_count").is_some());
    assert!(payload.get("effective_blocked_sites").is_some());
    assert!(payload.get("section").is_some());
}

#[test]
fn diagnostics_json_includes_blocking_preview_payload() {
    let env = TestEnv::new("diagnostics-blocking-preview-json");
    let output = env.run(&["--diagnostics", "--json"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(payload["action"], "diagnostics");
    assert!(payload.get("setup").is_some());
    assert!(payload.get("config_doctor").is_some());
    assert!(payload.get("config_migration").is_some());
    assert_eq!(payload["blocking_preview"]["status"], "ok");
    let preview = &payload["blocking_preview"]["preview"];
    assert_eq!(preview["deprecated"], false);
    assert!(preview.get("replacement").is_none());
    assert!(preview.get("backend").is_some());
    assert!(preview.get("action").is_some());
    assert!(preview.get("would_change").is_some());
    assert!(preview.get("effective_blocked_sites_count").is_some());
}

#[test]
fn strict_mode_is_profile_scoped_via_cli_commands() {
    let env = TestEnv::new("strict-profile-scoped");

    let select_deep_work = env.run(&["--profile", "deep-work", "--json"]);
    assert_eq!(select_deep_work.status.code(), Some(0));
    assert!(stderr_text(&select_deep_work).trim().is_empty());

    let set_strict_on = env.run(&["--strict=on", "--json"]);
    assert_eq!(set_strict_on.status.code(), Some(0));
    assert!(stderr_text(&set_strict_on).trim().is_empty());
    let set_payload: Value = serde_json::from_slice(&set_strict_on.stdout).unwrap();
    assert_eq!(set_payload["updated"], true);
    assert_eq!(set_payload["strict_mode"], true);

    let select_custom = env.run(&["--profile", "custom", "--json"]);
    assert_eq!(select_custom.status.code(), Some(0));
    assert!(stderr_text(&select_custom).trim().is_empty());

    let custom_strict = env.run(&["--strict", "--json"]);
    assert_eq!(custom_strict.status.code(), Some(0));
    assert!(stderr_text(&custom_strict).trim().is_empty());
    let custom_payload: Value = serde_json::from_slice(&custom_strict.stdout).unwrap();
    assert_eq!(custom_payload["strict_mode"], false);

    let select_deep_work_again = env.run(&["--profile", "deep-work", "--json"]);
    assert_eq!(select_deep_work_again.status.code(), Some(0));
    assert!(stderr_text(&select_deep_work_again).trim().is_empty());

    let deep_work_strict = env.run(&["--strict", "--json"]);
    assert_eq!(deep_work_strict.status.code(), Some(0));
    assert!(stderr_text(&deep_work_strict).trim().is_empty());
    let deep_work_payload: Value = serde_json::from_slice(&deep_work_strict.stdout).unwrap();
    assert_eq!(deep_work_payload["strict_mode"], true);
}

#[test]
fn recurring_schedule_is_profile_scoped_via_cli_commands() {
    let env = TestEnv::new("schedule-profile-scoped");

    let select_deep_work = env.run(&["--profile", "deep-work", "--json"]);
    assert_eq!(select_deep_work.status.code(), Some(0));
    assert!(stderr_text(&select_deep_work).trim().is_empty());

    let set_schedule = env.run(&[
        "--schedule-set={\"windows\":[{\"days\":[\"mon\"],\"start\":\"09:00\",\"end\":\"11:00\"}],\"exception_dates\":[],\"one_time_windows\":[]}",
        "--json",
    ]);
    assert_eq!(set_schedule.status.code(), Some(0));
    assert!(stderr_text(&set_schedule).trim().is_empty());
    let set_payload: Value = serde_json::from_slice(&set_schedule.stdout).unwrap();
    assert_eq!(set_payload["updated"], true);
    assert_eq!(
        set_payload["schedule"]["windows"].as_array().unwrap().len(),
        1
    );

    let select_custom = env.run(&["--profile", "custom", "--json"]);
    assert_eq!(select_custom.status.code(), Some(0));
    assert!(stderr_text(&select_custom).trim().is_empty());

    let custom_schedule = env.run(&["--schedule", "--json"]);
    assert_eq!(custom_schedule.status.code(), Some(0));
    assert!(stderr_text(&custom_schedule).trim().is_empty());
    let custom_payload: Value = serde_json::from_slice(&custom_schedule.stdout).unwrap();
    assert_eq!(
        custom_payload["schedule"]["windows"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let select_deep_work_again = env.run(&["--profile", "deep-work", "--json"]);
    assert_eq!(select_deep_work_again.status.code(), Some(0));
    assert!(stderr_text(&select_deep_work_again).trim().is_empty());

    let deep_work_schedule = env.run(&["--schedule", "--json"]);
    assert_eq!(deep_work_schedule.status.code(), Some(0));
    assert!(stderr_text(&deep_work_schedule).trim().is_empty());
    let deep_work_payload: Value = serde_json::from_slice(&deep_work_schedule.stdout).unwrap();
    assert_eq!(
        deep_work_payload["schedule"]["windows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn daemon_lifecycle_json_contract_round_trip() {
    let env = TestEnv::new("daemon-lifecycle-json");

    let start_output = env.run(&["--daemon-start", "--json"]);
    assert_eq!(start_output.status.code(), Some(0));
    assert!(stderr_text(&start_output).trim().is_empty());
    let start_payload: Value =
        serde_json::from_slice(&start_output.stdout).expect("stdout should be JSON");
    assert_eq!(start_payload["action"], "daemon-start");
    assert_eq!(start_payload["already_running"], false);
    assert!(
        start_payload["daemon"]["pid"]
            .as_u64()
            .is_some_and(|pid| pid > 0)
    );
    assert_eq!(start_payload["daemon"]["host"], "127.0.0.1");
    assert!(
        start_payload["daemon"]["port"]
            .as_u64()
            .is_some_and(|port| port > 0)
    );
    assert!(
        start_payload["daemon"]["started_at_epoch_secs"]
            .as_i64()
            .is_some_and(|timestamp| timestamp > 0)
    );

    let status_output = env.run(&["--daemon-status", "--json"]);
    assert_eq!(status_output.status.code(), Some(0));
    assert!(stderr_text(&status_output).trim().is_empty());
    let status_payload: Value =
        serde_json::from_slice(&status_output.stdout).expect("stdout should be JSON");
    assert_eq!(status_payload["action"], "daemon-status");
    assert_eq!(status_payload["running"], true);
    assert_eq!(
        status_payload["daemon"]["pid"],
        start_payload["daemon"]["pid"]
    );
    assert_eq!(status_payload["daemon"]["host"], "127.0.0.1");
    assert_eq!(
        status_payload["daemon"]["port"],
        start_payload["daemon"]["port"]
    );

    let stop_output = env.run(&["--daemon-stop", "--json"]);
    assert_eq!(stop_output.status.code(), Some(0));
    assert!(stderr_text(&stop_output).trim().is_empty());
    let stop_payload: Value = serde_json::from_slice(&stop_output.stdout).expect("stdout JSON");
    assert_eq!(stop_payload["action"], "daemon-stop");
    assert_eq!(stop_payload["was_running"], true);
    assert_eq!(stop_payload["stopped"], true);
    assert_eq!(
        stop_payload["daemon"]["pid"],
        start_payload["daemon"]["pid"]
    );

    let status_after_stop = env.run(&["--daemon-status", "--json"]);
    assert_eq!(status_after_stop.status.code(), Some(0));
    assert!(stderr_text(&status_after_stop).trim().is_empty());
    let status_after_stop_payload: Value =
        serde_json::from_slice(&status_after_stop.stdout).expect("stdout JSON");
    assert_eq!(status_after_stop_payload["action"], "daemon-status");
    assert_eq!(status_after_stop_payload["running"], false);
}

#[test]
fn daemon_local_api_supports_timer_and_metadata_controls() {
    let env = TestEnv::new("daemon-local-api-controls");
    let _daemon_guard = start_daemon(&env);
    let state = load_daemon_state(&env);
    assert!(state.pid > 0);
    assert_eq!(state.host, "127.0.0.1");
    assert!(state.port > 0);
    assert!(!state.token.trim().is_empty());
    assert!(state.started_at_epoch_secs > 0);

    let health = daemon_get_json(&state, "/v1/health");
    assert_eq!(health["ok"], true);
    assert_eq!(health["data"]["status"], "ok");

    let task_select = daemon_post_json(&state, "/v1/task/select", json!({ "label": "Docs" }));
    assert_eq!(task_select["ok"], true);
    assert!(task_select["data"]["state"].is_object());

    let timer_start = daemon_post_json(&state, "/v1/timer/start", json!({}));
    assert_eq!(timer_start["ok"], true);
    assert_eq!(timer_start["data"]["phase"], "focus");
    assert_eq!(timer_start["data"]["status"], "running");
    assert_eq!(timer_start["data"]["selected_task_label"], "Docs");

    let focus_intention = daemon_post_json(
        &state,
        "/v1/session/focus-intention",
        json!({ "value": "Write docs" }),
    );
    assert_eq!(focus_intention["ok"], true);
    assert_eq!(focus_intention["data"]["focus_intention"], "Write docs");
    assert_eq!(focus_intention["data"]["state"]["status"], "running");

    let stop = daemon_post_json(&state, "/v1/daemon/stop", json!({}));
    assert_eq!(stop["ok"], true);
    assert_eq!(stop["data"]["stopping"], true);
    wait_until_daemon_stopped(&env);
}
