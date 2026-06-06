use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
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
            "focustime-v014-regression-{label}-{}-{now}-{unique}",
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

    fn config_path(&self) -> PathBuf {
        self.app_data_dir().join("config.toml")
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
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct RemovedCliCase {
    issue: &'static str,
    args: &'static [&'static str],
    removed_flag: &'static str,
    replacement_hint: &'static str,
}

const REMOVED_CLI_CASES: &[RemovedCliCase] = &[
    RemovedCliCase {
        issue: "#299",
        args: &["--migrate", "--json"],
        removed_flag: "--migrate",
        replacement_hint: "--config-migrate",
    },
    RemovedCliCase {
        issue: "#299",
        args: &["--dry-run", "--json"],
        removed_flag: "--dry-run",
        replacement_hint: "--config-migrate",
    },
    RemovedCliCase {
        issue: "#413",
        args: &["--sync-backup", "--json"],
        removed_flag: "--sync-backup",
        replacement_hint: "--backup",
    },
    RemovedCliCase {
        issue: "#413",
        args: &["--sync-restore", "--json"],
        removed_flag: "--sync-restore",
        replacement_hint: "--restore",
    },
    RemovedCliCase {
        issue: "#413",
        args: &["--sync-passphrase=secret", "--json"],
        removed_flag: "--sync-passphrase",
        replacement_hint: "no direct replacement",
    },
];

#[test]
fn v014_removed_cli_surfaces_stay_retired_with_json_usage_errors() {
    let env = TestEnv::new("removed-cli");

    for case in REMOVED_CLI_CASES {
        let output = env.run(case.args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "removed flag {} from {} should fail as usage",
            case.removed_flag,
            case.issue
        );
        assert!(
            stderr_text(&output).trim().is_empty(),
            "JSON usage errors should stay on stdout for {}",
            case.removed_flag
        );
        let payload = parse_json_stdout(&output);
        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "usage");
        assert_eq!(payload["error"]["exit_code"], 2);
        let error_message = payload["error"]["message"]
            .as_str()
            .expect("error message should be a string");
        assert!(
            error_message.contains(case.removed_flag),
            "removed flag {} should be named in its error",
            case.removed_flag
        );
        assert!(
            error_message.contains(case.replacement_hint),
            "removed flag {} should include replacement guidance `{}`",
            case.removed_flag,
            case.replacement_hint
        );
    }
}

#[test]
fn v014_config_migration_preview_and_apply_cover_merged_profile_presets() {
    let env = TestEnv::new("config-migration");
    let original_config = r#"
schema_version = 1
selected_profile = "deep_work"

[[session_templates]]
name = "Deep Flow"
task_label = "Docs"
profile = "custom"
blocklist_profile = "Default"

[[weekday_profile_rules]]
day = "mon"
profile = "classic"
blocklist_profile = "Default"

[profile_automation.standard]
strict_mode = true
[profile_automation.standard.notifications]
enabled = false

[profile_automation.deep_work]
strict_mode = false
[profile_automation.deep_work.notifications]
enabled = true
sound = true
"#;
    write_config(&env, original_config);

    let preview = env.run(&["--config-migrate", "--json"]);
    assert_eq!(preview.status.code(), Some(0));
    assert!(stderr_text(&preview).trim().is_empty());
    let preview_payload = parse_json_stdout(&preview);
    assert_eq!(preview_payload["action"], "config-migrate");
    assert_eq!(preview_payload["applied"], false);
    assert_eq!(preview_payload["changed"], true);
    assert_eq!(preview_payload["detected_schema_version"], 1);
    assert_eq!(preview_payload["status"], "warning");
    assert!(
        preview_payload["steps"]
            .as_array()
            .expect("steps should be an array")
            .iter()
            .any(|step| step["from_schema_version"] == 1 && step["to_schema_version"] == 2)
    );
    assert!(
        preview_payload["findings"]
            .as_array()
            .expect("findings should be an array")
            .iter()
            .any(|finding| finding["code"] == "config.legacy_profile_token")
    );
    let preview_config = fs::read_to_string(env.config_path()).expect("config should still exist");
    assert_eq!(
        preview_config, original_config,
        "preview mode must not rewrite config.toml"
    );

    let apply = env.run(&["--config-migrate-apply", "--json"]);
    assert_eq!(apply.status.code(), Some(0));
    assert!(stderr_text(&apply).trim().is_empty());
    let apply_payload = parse_json_stdout(&apply);
    assert_eq!(apply_payload["action"], "config-migrate-apply");
    assert_eq!(apply_payload["applied"], true);
    assert_eq!(apply_payload["changed"], true);
    assert!(apply_payload["backup_path"].as_str().is_some());

    let migrated = fs::read_to_string(env.config_path()).expect("migrated config should exist");
    assert!(migrated.contains("schema_version = 2"));
    assert!(migrated.contains("selected_profile = \"standard\""));
    assert!(migrated.contains("[profile_automation.standard]"));
    assert!(migrated.contains("sound = true"));
    assert!(!migrated.contains("[profile_automation.deep_work]"));
    assert!(!migrated.contains("profile = \"classic\""));
    assert!(!migrated.contains("profile = \"custom\""));
}

#[test]
fn v014_runtime_stats_fallback_stays_removed_for_canonical_persistence() {
    let env = TestEnv::new("stats-fallback");

    let baseline = env.run(&["--status", "--json"]);
    assert_eq!(baseline.status.code(), Some(0));
    assert!(stderr_text(&baseline).trim().is_empty());
    let baseline_payload = parse_json_stdout(&baseline);
    let day = baseline_payload["day"]
        .as_str()
        .expect("day should be a string")
        .to_string();

    write_stats_snapshot(&env.legacy_stats_path(), &day, 7, 4200);
    let legacy_only = env.run(&["--status", "--json"]);
    assert_eq!(legacy_only.status.code(), Some(0));
    assert!(stderr_text(&legacy_only).trim().is_empty());
    let legacy_only_payload = parse_json_stdout(&legacy_only);
    assert_eq!(legacy_only_payload["today"]["pomodoros_completed"], 0);

    write_stats_snapshot(&env.canonical_stats_path(), &day, 3, 1800);
    let canonical = env.run(&["--status", "--json"]);
    assert_eq!(canonical.status.code(), Some(0));
    assert!(stderr_text(&canonical).trim().is_empty());
    let canonical_payload = parse_json_stdout(&canonical);
    assert_eq!(canonical_payload["today"]["pomodoros_completed"], 3);
}

fn focustime_bin_path() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_focustime")
        .map(PathBuf::from)
        .expect("CARGO_BIN_EXE_focustime not set")
}

fn write_config(env: &TestEnv, content: &str) {
    let app_data_dir = env.app_data_dir();
    fs::create_dir_all(&app_data_dir).expect("failed to create app data directory");
    fs::write(env.config_path(), content).expect("failed to write config");
}

fn write_stats_snapshot(path: &Path, day: &str, pomodoros: u32, focused_seconds: u64) {
    let content = format!(
        "[daily.\"{day}\"]\npomodoros_completed = {pomodoros}\nfocused_seconds = {focused_seconds}\n"
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create stats parent directory");
    }
    fs::write(path, content).expect("failed to write stats snapshot");
}

fn parse_json_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should be JSON")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}
