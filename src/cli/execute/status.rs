use std::{
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::cli::{
    AppConfig, FocusStats, OutputMode, StatusOutput, build_status_output, flush_stdout, print_json,
    print_json_compact, print_status_output,
};

use super::data::stats_load_options;

const WATCH_INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub(super) static WATCH_INTERRUPTED: AtomicBool = AtomicBool::new(false);
static WATCH_INTERRUPT_HANDLER: OnceLock<Result<(), String>> = OnceLock::new();
pub(super) fn execute_status_command(
    output: OutputMode,
    watch_interval_secs: Option<u64>,
) -> Result<(), String> {
    if let Some(interval_secs) = watch_interval_secs {
        return execute_status_watch_command(output, interval_secs);
    }

    let payload = load_status_output()?;
    emit_status_output(&payload, output, false)
}

pub(super) fn execute_status_watch_command(
    output: OutputMode,
    interval_secs: u64,
) -> Result<(), String> {
    if interval_secs == 0 {
        return Err("`--watch` interval must be greater than 0 seconds.".to_string());
    }

    install_watch_interrupt_handler()?;
    WATCH_INTERRUPTED.store(false, Ordering::SeqCst);
    let interval = Duration::from_secs(interval_secs);
    let mut next_deadline = Instant::now();

    loop {
        if WATCH_INTERRUPTED.load(Ordering::SeqCst) {
            break;
        }

        let payload = load_status_output()?;
        emit_status_output(&payload, output, true)?;
        flush_stdout()?;

        next_deadline = next_watch_deadline(next_deadline, interval, Instant::now());
        if wait_for_next_watch_tick(next_deadline) {
            break;
        }
    }

    Ok(())
}

fn install_watch_interrupt_handler() -> Result<(), String> {
    WATCH_INTERRUPT_HANDLER
        .get_or_init(|| unsafe { install_platform_watch_interrupt_handler() })
        .clone()
}

pub(super) fn next_watch_deadline(
    previous_deadline: Instant,
    interval: Duration,
    now: Instant,
) -> Instant {
    let mut deadline = previous_deadline + interval;
    while deadline <= now {
        deadline += interval;
    }
    deadline
}

pub(super) fn wait_for_next_watch_tick(deadline: Instant) -> bool {
    loop {
        if WATCH_INTERRUPTED.load(Ordering::SeqCst) {
            return true;
        }

        let now = Instant::now();
        if now >= deadline {
            return false;
        }

        let sleep_for = deadline
            .saturating_duration_since(now)
            .min(WATCH_INTERRUPT_POLL_INTERVAL);
        thread::sleep(sleep_for);
    }
}

#[cfg(unix)]
unsafe fn install_platform_watch_interrupt_handler() -> Result<(), String> {
    unsafe extern "C" fn handle_sigint(_signal: i32) {
        WATCH_INTERRUPTED.store(true, Ordering::SeqCst);
    }

    unsafe extern "C" {
        fn signal(signum: i32, handler: unsafe extern "C" fn(i32)) -> unsafe extern "C" fn(i32);
    }

    const SIGINT: i32 = 2;

    let _previous = unsafe { signal(SIGINT, handle_sigint) };
    Ok(())
}

#[cfg(target_os = "windows")]
unsafe fn install_platform_watch_interrupt_handler() -> Result<(), String> {
    unsafe extern "system" fn handle_console_ctrl(ctrl_type: u32) -> i32 {
        const CTRL_C_EVENT: u32 = 0;
        const CTRL_BREAK_EVENT: u32 = 1;
        if ctrl_type == CTRL_C_EVENT || ctrl_type == CTRL_BREAK_EVENT {
            WATCH_INTERRUPTED.store(true, Ordering::SeqCst);
            return 1;
        }
        0
    }

    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            handler_routine: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }

    let installed = unsafe { SetConsoleCtrlHandler(Some(handle_console_ctrl), 1) };
    if installed == 0 {
        Err(
            "Failed to install watch interrupt handler: SetConsoleCtrlHandler returned 0."
                .to_string(),
        )
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
unsafe fn install_platform_watch_interrupt_handler() -> Result<(), String> {
    Ok(())
}

fn load_status_output() -> Result<StatusOutput, String> {
    let config = AppConfig::load().normalized();
    let stats = FocusStats::load_with_options(stats_load_options(&config))
        .map_err(|error| format!("Failed to load stats: {error}"))?;
    Ok(build_status_output(&config, &stats))
}

fn emit_status_output(
    payload: &StatusOutput,
    output: OutputMode,
    watch_mode: bool,
) -> Result<(), String> {
    match output {
        OutputMode::Text => {
            print_status_output(payload);
            if watch_mode {
                println!();
            }
        }
        OutputMode::Json => {
            if watch_mode {
                print_json_compact(payload)?;
            } else {
                print_json(payload)?;
            }
        }
    }
    Ok(())
}
