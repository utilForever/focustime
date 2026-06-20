use std::{
    fs,
    path::PathBuf,
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
            "focustime-v015-cleanup-{label}-{}-{now}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("failed to create temp test directory");
        Self { root }
    }

    fn app_data_dir(&self) -> PathBuf {
        self.root.join("focustime")
    }

    fn config_path(&self) -> PathBuf {
        self.app_data_dir().join("config.toml")
    }

    fn write_config(&self, content: &str) {
        fs::create_dir_all(self.app_data_dir()).expect("failed to create app data directory");
        fs::write(self.config_path(), content).expect("failed to write config");
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

#[test]
fn v015_cleanup_docs_keep_matrix_and_release_guidance_aligned() {
    let matrix = include_str!("../REGRESSION_MATRIX.md");
    let readme = include_str!("../README.md");
    let changelog = include_str!("../CHANGELOG.md");
    let contributing = include_str!("../CONTRIBUTING.md");

    for required in [
        "v0.15.x",
        "Cleanup roadmap documentation names supported replacements",
        "Deprecated config compatibility fields stay visible in diagnostics",
        "Merged legacy profile names continue to migrate to canonical presets",
        "Removed migration-window and encrypted sync flags stay unavailable",
        "Retired blocklist category config no longer adds config-doctor warnings",
        "Retired blocklist category config still migrates into profile-level `sites` and `allowlist_sites`",
        "Daemon local API lifecycle commands report retirement guidance",
        "WakaTime integration runtime exposes only supported tracking calls",
        "poll_wakatime_events_applies_async_updates",
        "disabled_wakatime_runtime_ignores_supported_hooks",
        "cargo test --test v015_cleanup_regression",
    ] {
        assert!(
            matrix.contains(required),
            "regression matrix should include `{required}`"
        );
    }

    for required in [
        "### v0.15.x cleanup roadmap",
        "Legacy timer duration fields",
        "Legacy automation and blocklist top-level fields",
        "Retired blocklist category config is migration-only",
        "Removed migration-window flags (`--migrate`, `--dry-run`)",
        "Retired encrypted sync flags (`--sync-backup`, `--sync-restore`, `--sync-passphrase`)",
        "Duplicate schedule/session start entry points",
        "Daemon local API lifecycle (`--daemon-start`, `--daemon-status`, `--daemon-stop`, `--daemon-port`, `/v1/*`)",
        "Broad integration lifecycle/capability hooks",
        "supported WakaTime integration runtime calls",
        "Runtime dependency cleanup candidates",
        "`tiny_http`",
        "`getrandom`",
        "`ureq` JSON feature",
        "`chrono-tz`",
        "`base64` daemon usage",
    ] {
        assert!(
            readme.contains(required),
            "README cleanup roadmap should include `{required}`"
        );
    }

    assert!(changelog.contains("v0.15.x cleanup roadmap and deprecation notices"));
    assert!(changelog.contains("Integration runtime hook narrowing (#453)"));
    assert!(contributing.contains("cargo test --test v015_cleanup_regression"));
    assert!(contributing.contains("v0.15.x cleanup releases"));
    assert!(contributing.contains("deprecated daemon bearer-token generation"));
    assert!(matrix.contains("Runtime dependency removal candidates stay documented"));
}

#[test]
fn v015_deprecated_config_paths_report_supported_replacements() {
    let env = TestEnv::new("deprecated-config");
    env.write_config(
        r#"
schema_version = 2
focus_secs = 1800
short_break_secs = 360
long_break_secs = 1200
long_break_interval = 3
blocked_sites = ["youtube.com"]
strict_mode = true

[notifications]
enabled = true
sound = true
"#,
    );

    let output = env.run(&["--config-doctor", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload = parse_json_stdout(&output);
    assert_eq!(payload["action"], "config-doctor");
    assert_eq!(payload["status"], "warning");
    let findings = payload["findings"]
        .as_array()
        .expect("findings should be an array");
    assert_finding_contains_all(
        findings,
        "config.deprecated_field_in_use",
        &["focus_secs", "[custom_profile]"],
    );
    assert_finding_contains_all(
        findings,
        "config.deprecated_field_in_use",
        &["notifications", "[profile_automation.<preset>]"],
    );
    assert_finding_contains_all(
        findings,
        "config.deprecated_field_in_use",
        &["blocked_sites", "blocklist profile"],
    );
}

#[test]
fn v015_merged_profile_paths_keep_migration_guidance() {
    let env = TestEnv::new("merged-profiles");
    env.write_config(
        r#"
schema_version = 1
selected_profile = "deep_work"

[[session_templates]]
name = "Classic Flow"
profile = "classic"

[profile_automation.custom]
strict_mode = true
"#,
    );

    let output = env.run(&["--config-migrate", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr_text(&output).trim().is_empty());

    let payload = parse_json_stdout(&output);
    assert_eq!(payload["action"], "config-migrate");
    assert_eq!(payload["changed"], true);
    assert!(
        payload["steps"]
            .as_array()
            .expect("steps should be an array")
            .iter()
            .any(|step| step["from_schema_version"] == 1 && step["to_schema_version"] == 2)
    );
    let findings = payload["findings"]
        .as_array()
        .expect("findings should be an array");
    assert_finding_contains_all(
        findings,
        "config.legacy_profile_token",
        &["selected_profile", "standard"],
    );
    assert_finding_contains_all(
        findings,
        "config.legacy_profile_token",
        &["session_templates[0].profile", "basic"],
    );
    assert_finding_contains_all(
        findings,
        "config.legacy_profile_token",
        &[
            "[profile_automation.custom]",
            "[profile_automation.advanced]",
        ],
    );
    assert!(findings.iter().all(|finding| {
        finding["remediation"].as_str().is_some_and(|remediation| {
            remediation.contains("basic")
                && remediation.contains("standard")
                && remediation.contains("advanced")
        })
    }));
}

#[test]
fn v015_removed_command_paths_keep_supported_json_guidance_only() {
    let env = TestEnv::new("removed-commands");

    for (flag, replacement) in [
        ("--migrate", "--config-migrate"),
        ("--dry-run", "--config-migrate"),
        ("--sync-backup", "--backup"),
        ("--sync-restore", "--restore"),
        ("--sync-passphrase=secret", "no direct replacement"),
    ] {
        let output = env.run(&[flag, "--json"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(
            stderr_text(&output).trim().is_empty(),
            "JSON usage errors should stay on stdout for {flag}"
        );
        let payload = parse_json_stdout(&output);
        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "usage");
        assert_eq!(payload["error"]["exit_code"], 2);
        let message = payload["error"]["message"]
            .as_str()
            .expect("error message should be a string");
        let hint = payload["error"]["hint"]
            .as_str()
            .expect("removed flag should include a replacement hint");
        assert!(
            message.contains(flag.split('=').next().unwrap_or(flag)),
            "removed flag should be named in its error: {message}"
        );
        assert!(
            hint.contains(replacement),
            "removed flag should include replacement hint `{replacement}`: {hint}"
        );
    }

    for flag in ["--usage-signals", "--blocking-preview"] {
        let output = env.run(&[flag, "--json"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(
            stderr_text(&output).trim().is_empty(),
            "JSON usage errors should stay on stdout for {flag}"
        );
        let payload = parse_json_stdout(&output);
        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "usage");
        assert_eq!(payload["error"]["exit_code"], 2);
        let message = payload["error"]["message"]
            .as_str()
            .expect("error message should be a string");
        assert!(
            message.contains(flag),
            "removed flag should be named in its error: {message}"
        );
        assert!(
            payload["error"].get("hint").is_none(),
            "removed command-surface ballast should not add a replacement-only hint for {flag}"
        );
    }
}

#[test]
fn v015_removed_command_text_errors_keep_supported_replacement_guidance_only() {
    let env = TestEnv::new("removed-commands-text");

    let (flag, expected_hint) = (
        "--sync-backup",
        "Hint: Use `--backup` for local portable recovery workflows.",
    );
    let output = env.run(&[flag]);

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
    let stderr = stderr_text(&output);
    assert!(stderr.contains(&format!("Unknown option `{flag}`")));
    assert!(stderr.contains(expected_hint));

    let output = env.run(&["--usage-signals"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
    let stderr = stderr_text(&output);
    assert!(stderr.contains("Unknown option `--usage-signals`"));
    assert!(!stderr.contains("Hint:"));
}

fn focustime_bin_path() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_focustime")
        .map(PathBuf::from)
        .expect("CARGO_BIN_EXE_focustime not set")
}

fn parse_json_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should be JSON")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn assert_finding_contains_all(findings: &[Value], code: &str, needles: &[&str]) {
    assert!(
        findings.iter().any(|finding| {
            if finding["code"] != code {
                return false;
            }
            let haystack = format!(
                "{}\n{}",
                finding["message"].as_str().unwrap_or_default(),
                finding["remediation"].as_str().unwrap_or_default()
            );
            needles.iter().all(|needle| haystack.contains(needle))
        }),
        "expected finding `{code}` containing {needles:?} in {findings:#?}"
    );
}
