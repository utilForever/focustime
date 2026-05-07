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

fn write_recovery_snapshot(env: &TestEnv, content: &str) {
    let app_data_dir = env.root.join("focustime");
    fs::create_dir_all(&app_data_dir).expect("failed to create app data directory");
    fs::write(app_data_dir.join("session-recovery.toml"), content)
        .expect("failed to write recovery snapshot");
}

#[test]
fn status_json_success_emits_payload_on_stdout() {
    let env = TestEnv::new("status-json-success");
    let output = env.run(&["--status", "--json"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(payload.get("day").is_some());
    assert!(payload.get("selected_break_template").is_some());
    assert!(payload.get("available_break_templates").is_some());
    assert!(payload.get("focus_intention").is_some());
    assert!(payload.get("task_note").is_some());
    assert!(payload.get("goal").is_some());
    assert!(payload.get("weekly_goal").is_some());
    assert!(payload.get("monthly_goal").is_some());
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
fn session_metadata_json_reads_fallback_from_selected_task_label() {
    let env = TestEnv::new("metadata-json-read-fallback");

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
    assert_eq!(payload["focus_intention"], "Docs");
    assert_eq!(payload["task_note"], "Docs");
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
fn backup_json_copies_raw_files_even_when_malformed() {
    let env = TestEnv::new("backup-raw-malformed");
    let app_data_dir = env.root.join("focustime");
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
