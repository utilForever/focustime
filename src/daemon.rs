use std::fs;
use std::io;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use getrandom::fill as random_fill;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::app::App;
use crate::config::app_data_path;
use crate::timer::{TimerPhase, TimerStatus};

const DAEMON_STATE_FILE_NAME: &str = "daemon-state.toml";
const DAEMON_CHILD_ENV: &str = "FOCUSTIME_DAEMON_CHILD";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(6);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(100);
const LOOP_POLL_INTERVAL: Duration = Duration::from_millis(100);
const HTTP_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(target_os = "windows")]
const DETACHED_PROCESS: u32 = 0x0000_0008;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonConnectionInfo {
    pub pid: u32,
    pub host: String,
    pub port: u16,
    pub started_at_epoch_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStartResult {
    pub already_running: bool,
    pub info: DaemonConnectionInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatusResult {
    pub running: bool,
    pub info: Option<DaemonConnectionInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStopResult {
    pub was_running: bool,
    pub stopped: bool,
    pub info: Option<DaemonConnectionInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DaemonStateDisk {
    pid: u32,
    host: String,
    port: u16,
    token: String,
    started_at_epoch_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct TaskSelectRequest {
    label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct MetadataRequest {
    value: String,
}

struct DaemonStateGuard {
    pid: u32,
}

impl Drop for DaemonStateGuard {
    fn drop(&mut self) {
        let _ = clear_state_file_if_matches_pid(self.pid);
    }
}

pub fn is_daemon_child_process() -> bool {
    std::env::var_os(DAEMON_CHILD_ENV).is_some()
}

pub fn start_background(port: Option<u16>) -> Result<DaemonStartResult, String> {
    let status = status()?;
    if status.running {
        return Ok(DaemonStartResult {
            already_running: true,
            info: status
                .info
                .ok_or_else(|| "Daemon status is running but metadata is missing.".to_string())?,
        });
    }

    let mut child = spawn_daemon_child_process(port)?;
    let state = wait_for_daemon_ready(child.id(), &mut child)?;
    Ok(DaemonStartResult {
        already_running: false,
        info: state.as_connection_info(),
    })
}

pub fn status() -> Result<DaemonStatusResult, String> {
    let Some(state) = load_state_file()? else {
        return Ok(DaemonStatusResult {
            running: false,
            info: None,
        });
    };

    match ping_health(&state) {
        Ok(()) => Ok(DaemonStatusResult {
            running: true,
            info: Some(state.as_connection_info()),
        }),
        Err(_) => {
            clear_state_file()?;
            Ok(DaemonStatusResult {
                running: false,
                info: Some(state.as_connection_info()),
            })
        }
    }
}

pub fn stop() -> Result<DaemonStopResult, String> {
    let Some(state) = load_state_file()? else {
        return Ok(DaemonStopResult {
            was_running: false,
            stopped: false,
            info: None,
        });
    };
    let info = state.as_connection_info();

    if ping_health(&state).is_err() {
        clear_state_file()?;
        return Ok(DaemonStopResult {
            was_running: false,
            stopped: false,
            info: Some(info),
        });
    }

    request_stop(&state)?;
    let mut stopped = false;
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        thread::sleep(READY_POLL_INTERVAL);
        let current = load_state_file()?;
        if current.is_none() {
            stopped = true;
            break;
        }
        if ping_health(&state).is_err() {
            let _ = clear_state_file();
            stopped = true;
            break;
        }
    }

    Ok(DaemonStopResult {
        was_running: true,
        stopped,
        info: Some(info),
    })
}

pub fn run_foreground(port: Option<u16>) -> Result<(), String> {
    if let Some(existing_state) = load_state_file()? {
        if ping_health(&existing_state).is_ok() {
            return Err(format!(
                "Daemon is already running on {}:{}.",
                existing_state.host, existing_state.port
            ));
        }
        clear_state_file()?;
    }

    let bind_port = port.unwrap_or(0);
    let server = Server::http(format!("127.0.0.1:{bind_port}"))
        .map_err(|error| format!("Failed to bind daemon API server: {error}"))?;
    let server_port = parse_bound_port(&server.server_addr().to_string())?;
    let token = generate_token()?;
    let started_at_epoch_secs = current_epoch_secs();
    let pid = std::process::id();
    let state = DaemonStateDisk {
        pid,
        host: "127.0.0.1".to_string(),
        port: server_port,
        token,
        started_at_epoch_secs,
    };
    save_state_file(&state)?;
    let _state_guard = DaemonStateGuard { pid };

    let mut app = App::new();
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let mut tick_accumulator_ms: u64 = 0;
    let mut should_stop = false;
    while !should_stop {
        app.poll_wakatime_status();
        tick_if_due(
            &mut app,
            tick_rate,
            &mut last_tick,
            &mut tick_accumulator_ms,
        );
        match server.recv_timeout(LOOP_POLL_INTERVAL) {
            Ok(Some(request)) => {
                should_stop = handle_api_request(request, &mut app, &state.token)?;
            }
            Ok(None) => {}
            Err(error) => return Err(format!("Daemon receive loop failed: {error}")),
        }
    }

    Ok(())
}

fn spawn_daemon_child_process(port: Option<u16>) -> Result<Child, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Failed to resolve executable path: {error}"))?;
    let mut command = Command::new(executable);
    command.arg("--daemon-start");
    if let Some(port) = port {
        command.arg(format!("--daemon-port={port}"));
    }
    command.env(DAEMON_CHILD_ENV, "1");
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    command
        .spawn()
        .map_err(|error| format!("Failed to start daemon process: {error}"))
}

fn wait_for_daemon_ready(expected_pid: u32, child: &mut Child) -> Result<DaemonStateDisk, String> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(exit_status) = child
            .try_wait()
            .map_err(|error| format!("Failed to inspect daemon process state: {error}"))?
        {
            return Err(format!(
                "Daemon process exited early with status {exit_status}."
            ));
        }

        if let Some(state) = load_state_file()? {
            if state.pid == expected_pid && ping_health(&state).is_ok() {
                return Ok(state);
            }
        }
        thread::sleep(READY_POLL_INTERVAL);
    }
    Err("Daemon did not become ready before timeout.".to_string())
}

fn ping_health(state: &DaemonStateDisk) -> Result<(), String> {
    let url = daemon_endpoint_url(state, "/v1/health");
    let auth_header = format!("Bearer {}", state.token);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .into();
    agent
        .get(&url)
        .header("Authorization", &auth_header)
        .call()
        .map(|_| ())
        .map_err(|error| format!("Daemon health request failed: {error}"))
}

fn request_stop(state: &DaemonStateDisk) -> Result<(), String> {
    let url = daemon_endpoint_url(state, "/v1/daemon/stop");
    let auth_header = format!("Bearer {}", state.token);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .into();
    agent
        .post(&url)
        .header("Authorization", &auth_header)
        .send_json(json!({}))
        .map(|_| ())
        .map_err(|error| format!("Daemon stop request failed: {error}"))
}

fn daemon_endpoint_url(state: &DaemonStateDisk, path: &str) -> String {
    format!("http://{}:{}{path}", state.host, state.port)
}

fn parse_bound_port(address: &str) -> Result<u16, String> {
    address
        .rsplit(':')
        .next()
        .ok_or_else(|| format!("Failed to parse daemon server address `{address}`."))?
        .parse::<u16>()
        .map_err(|error| format!("Failed to parse daemon server port from `{address}`: {error}"))
}

fn generate_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    random_fill(&mut bytes)
        .map_err(|error| format!("Failed to generate daemon auth token: {error}"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn current_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

fn tick_if_due(
    app: &mut App,
    tick_rate: Duration,
    last_tick: &mut Instant,
    tick_accumulator_ms: &mut u64,
) {
    if last_tick.elapsed() < tick_rate {
        return;
    }

    let elapsed_ms = last_tick.elapsed().as_millis() as u64;
    *last_tick = Instant::now();
    if app.is_running() {
        *tick_accumulator_ms += elapsed_ms;
        let mut elapsed_secs = 0u64;
        while *tick_accumulator_ms >= 1000 {
            *tick_accumulator_ms -= 1000;
            elapsed_secs += 1;
        }
        let is_catchup = elapsed_secs > 1;
        for _ in 0..elapsed_secs {
            app.on_tick(is_catchup);
        }
        if elapsed_secs > 0 {
            app.on_wakatime_elapsed(elapsed_secs);
        }
    } else {
        *tick_accumulator_ms = 0;
    }
}

fn handle_api_request(mut request: Request, app: &mut App, token: &str) -> Result<bool, String> {
    if !request_is_loopback(&request) {
        respond_error(
            request,
            403,
            "forbidden",
            "Only loopback clients can access the local API.".to_string(),
        )?;
        return Ok(false);
    }
    if !request_has_valid_token(&request, token) {
        respond_error(
            request,
            401,
            "unauthorized",
            "Missing or invalid bearer token.".to_string(),
        )?;
        return Ok(false);
    }

    let path = request.url().split('?').next().unwrap_or("/").to_string();
    match (request.method(), path.as_str()) {
        (&Method::Get, "/v1/health") => {
            respond_ok(
                request,
                json!({
                    "status": "ok",
                    "timestamp_epoch_secs": current_epoch_secs()
                }),
            )?;
            Ok(false)
        }
        (&Method::Get, "/v1/status") => {
            respond_ok(request, api_timer_state(app))?;
            Ok(false)
        }
        (&Method::Post, "/v1/timer/start") => {
            run_timer_action(request, app, App::start_focus_for_cli)?;
            Ok(false)
        }
        (&Method::Post, "/v1/timer/pause") => {
            run_timer_action(request, app, App::pause_for_cli)?;
            Ok(false)
        }
        (&Method::Post, "/v1/timer/resume") => {
            run_timer_action(request, app, App::resume_for_cli)?;
            Ok(false)
        }
        (&Method::Post, "/v1/timer/stop") => {
            run_timer_action(request, app, App::stop_for_cli)?;
            Ok(false)
        }
        (&Method::Post, "/v1/timer/next") => {
            run_timer_action(request, app, App::next_phase_for_cli)?;
            Ok(false)
        }
        (&Method::Post, "/v1/task/select") => {
            let body: TaskSelectRequest = parse_json_body(&mut request)?;
            match app.select_task_label_for_cli(&body.label) {
                Ok(created) => respond_ok(
                    request,
                    json!({
                        "created": created,
                        "state": api_timer_state(app)
                    }),
                )?,
                Err(error) => {
                    respond_error(request, 400, "invalid_request", error)?;
                }
            }
            Ok(false)
        }
        (&Method::Post, "/v1/session/focus-intention") => {
            let body: MetadataRequest = parse_json_body(&mut request)?;
            match app.set_focus_intention_for_cli(&body.value) {
                Ok(()) => respond_ok(
                    request,
                    json!({
                        "focus_intention": app.focus_intention_for_cli(),
                        "state": api_timer_state(app)
                    }),
                )?,
                Err(error) => respond_error(request, 400, "invalid_request", error)?,
            }
            Ok(false)
        }
        (&Method::Post, "/v1/session/task-note") => {
            let body: MetadataRequest = parse_json_body(&mut request)?;
            match app.set_task_note_for_cli(&body.value) {
                Ok(()) => respond_ok(
                    request,
                    json!({
                        "task_note": app.task_note_for_cli(),
                        "state": api_timer_state(app)
                    }),
                )?,
                Err(error) => respond_error(request, 400, "invalid_request", error)?,
            }
            Ok(false)
        }
        (&Method::Post, "/v1/workflow/schedule-delay") => {
            match app.schedule_delay_for_cli() {
                Ok(delayed_until) => respond_ok(
                    request,
                    json!({
                        "delayed_until": delayed_until,
                        "state": api_timer_state(app)
                    }),
                )?,
                Err(error) => respond_error(request, 400, "invalid_request", error)?,
            }
            Ok(false)
        }
        (&Method::Post, "/v1/workflow/break-glass/trigger") => {
            run_workflow_action(request, app, App::trigger_break_glass_for_cli)?;
            Ok(false)
        }
        (&Method::Post, "/v1/workflow/break-glass/cancel") => {
            run_workflow_action(request, app, App::cancel_break_glass_for_cli)?;
            Ok(false)
        }
        (&Method::Post, "/v1/daemon/stop") => {
            respond_ok(
                request,
                json!({
                    "stopping": true
                }),
            )?;
            Ok(true)
        }
        _ => {
            respond_error(
                request,
                404,
                "not_found",
                format!("Unknown endpoint `{path}`."),
            )?;
            Ok(false)
        }
    }
}

fn run_timer_action(
    request: Request,
    app: &mut App,
    action: fn(&mut App) -> Result<(), String>,
) -> Result<(), String> {
    match action(app) {
        Ok(()) => respond_ok(request, api_timer_state(app)),
        Err(error) => respond_error(request, 400, "invalid_request", error),
    }
}

fn run_workflow_action(
    request: Request,
    app: &mut App,
    action: fn(&mut App) -> Result<(), String>,
) -> Result<(), String> {
    match action(app) {
        Ok(()) => respond_ok(request, json!({ "state": api_timer_state(app) })),
        Err(error) => respond_error(request, 400, "invalid_request", error),
    }
}

fn api_timer_state(app: &App) -> serde_json::Value {
    let (phase, status, remaining_secs, pomodoros_completed) = app.timer_state_for_cli();
    json!({
        "phase": timer_phase_id(phase),
        "status": timer_status_id(status),
        "remaining_secs": remaining_secs,
        "pomodoros_completed": pomodoros_completed,
        "selected_profile": app.selected_profile_name(),
        "selected_task_label": app.selected_task_label_for_cli(),
        "focus_intention": app.focus_intention_for_cli(),
        "task_note": app.task_note_for_cli(),
        "selected_blocklist_profile": app.selected_blocklist_profile_name_for_cli()
    })
}

fn timer_phase_id(phase: TimerPhase) -> &'static str {
    match phase {
        TimerPhase::Focus => "focus",
        TimerPhase::ShortBreak => "short-break",
        TimerPhase::LongBreak => "long-break",
    }
}

fn timer_status_id(status: TimerStatus) -> &'static str {
    match status {
        TimerStatus::Idle => "idle",
        TimerStatus::Running => "running",
        TimerStatus::Paused => "paused",
    }
}

fn parse_json_body<T: for<'de> Deserialize<'de>>(request: &mut Request) -> Result<T, String> {
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|error| format!("Failed to read request body: {error}"))?;
    serde_json::from_str::<T>(&body).map_err(|error| format!("Invalid JSON body: {error}"))
}

fn request_is_loopback(request: &Request) -> bool {
    request
        .remote_addr()
        .map(|address| address.ip().is_loopback())
        .unwrap_or(true)
}

fn request_has_valid_token(request: &Request, token: &str) -> bool {
    let expected = format!("Bearer {token}");
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .map(|header| header.value.as_str() == expected)
        .unwrap_or(false)
}

fn respond_ok(request: Request, data: serde_json::Value) -> Result<(), String> {
    respond_json(request, 200, json!({ "ok": true, "data": data }))
}

fn respond_error(request: Request, status: u16, code: &str, message: String) -> Result<(), String> {
    respond_json(
        request,
        status,
        json!({
            "ok": false,
            "error": {
                "code": code,
                "message": message
            }
        }),
    )
}

fn respond_json(request: Request, status: u16, payload: serde_json::Value) -> Result<(), String> {
    let body = serde_json::to_string(&payload)
        .map_err(|error| format!("Failed to encode API response JSON: {error}"))?;
    let content_type = Header::from_bytes("Content-Type", "application/json")
        .map_err(|_| "Failed to build JSON response header.".to_string())?;
    let response = Response::from_string(body)
        .with_status_code(StatusCode(status))
        .with_header(content_type);
    request
        .respond(response)
        .map_err(|error| format!("Failed to send API response: {error}"))
}

fn daemon_state_path() -> Result<PathBuf, String> {
    app_data_path(DAEMON_STATE_FILE_NAME)
        .ok_or_else(|| "Could not determine application data path for daemon state.".to_string())
}

fn load_state_file() -> Result<Option<DaemonStateDisk>, String> {
    let path = daemon_state_path()?;
    match fs::read_to_string(path) {
        Ok(content) => toml::from_str::<DaemonStateDisk>(&content)
            .map(Some)
            .map_err(|error| format!("Failed to parse daemon state: {error}")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Failed to read daemon state: {error}")),
    }
}

fn save_state_file(state: &DaemonStateDisk) -> Result<(), String> {
    let path = daemon_state_path()?;
    write_atomic_toml(&path, state).map_err(|error| format!("Failed to save daemon state: {error}"))
}

fn clear_state_file() -> Result<(), String> {
    let path = daemon_state_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Failed to remove daemon state: {error}")),
    }
}

fn clear_state_file_if_matches_pid(expected_pid: u32) -> Result<(), String> {
    let Some(state) = load_state_file()? else {
        return Ok(());
    };
    if state.pid == expected_pid {
        clear_state_file()?;
    }
    Ok(())
}

fn write_atomic_toml(path: &Path, value: &DaemonStateDisk) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, content)?;

    #[cfg(target_os = "windows")]
    {
        match fs::rename(&tmp_path, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(path)?;
                fs::rename(&tmp_path, path)
            }
            Err(error) => {
                let _ = fs::remove_file(&tmp_path);
                Err(error)
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        match fs::rename(&tmp_path, path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&tmp_path);
                Err(error)
            }
        }
    }
}

impl DaemonStateDisk {
    fn as_connection_info(&self) -> DaemonConnectionInfo {
        DaemonConnectionInfo {
            pid: self.pid,
            host: self.host.clone(),
            port: self.port,
            started_at_epoch_secs: self.started_at_epoch_secs,
        }
    }
}
