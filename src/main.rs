mod app;
mod blocker;
mod calendar;
mod cli;
mod config;
mod error;
mod integration;
mod notifications;
mod schedule;
mod session_recovery;
mod stats;
mod task_labels;
mod temporary_allowlist;
mod timer;
mod ui;
mod wakatime;

use std::{
    io, process,
    time::{Duration, Instant},
};

use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyEvent,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;
use app::should_handle_key;
use cli::{
    CliAction, emit_cli_error, execute_command, parse_args_with_contract, runtime_error, usage_text,
};

/// RAII guard that restores the terminal on drop, ensuring cleanup on any exit path.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

#[derive(Debug)]
enum RuntimeEvent {
    Key(KeyEvent),
    Paste(String),
    TimerElapsed(TimerElapsed),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimerElapsed {
    elapsed_secs: u64,
    is_catchup: bool,
}

struct RuntimeClock {
    tick_rate: Duration,
    last_tick: Instant,
    tick_accumulator_ms: u64,
}

impl RuntimeClock {
    fn new(tick_rate: Duration) -> Self {
        Self {
            tick_rate,
            last_tick: Instant::now(),
            tick_accumulator_ms: 0,
        }
    }

    fn poll_timeout(&self) -> Duration {
        self.tick_rate
            .checked_sub(self.last_tick.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    fn timer_elapsed_if_due(&mut self, timer_running: bool) -> Option<TimerElapsed> {
        if self.last_tick.elapsed() < self.tick_rate {
            return None;
        }

        let elapsed_ms = self.last_tick.elapsed().as_millis() as u64;
        self.last_tick = Instant::now();
        self.advance_by(timer_running, elapsed_ms)
    }

    fn advance_by(&mut self, timer_running: bool, elapsed_ms: u64) -> Option<TimerElapsed> {
        if !timer_running {
            self.tick_accumulator_ms = 0;
            return None;
        }

        self.tick_accumulator_ms += elapsed_ms;
        let elapsed_secs = self.tick_accumulator_ms / 1000;
        self.tick_accumulator_ms %= 1000;
        if elapsed_secs == 0 {
            return None;
        }

        Some(TimerElapsed {
            elapsed_secs,
            is_catchup: elapsed_secs > 1,
        })
    }
}

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(e) = execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        ) {
            let _ = disable_raw_mode();
            return Err(e);
        }
        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(e) => {
                // Alternate screen and mouse capture are already active; undo them
                // before returning since Drop won't run on an unconstructed value.
                let mut stdout = io::stdout();
                let _ = execute!(
                    stdout,
                    LeaveAlternateScreen,
                    DisableMouseCapture,
                    DisableBracketedPaste
                );
                let _ = disable_raw_mode();
                Err(e)
            }
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste
        );
        let _ = self.terminal.show_cursor();
    }
}

fn main() -> io::Result<()> {
    let cli_action = match parse_args_with_contract(std::env::args_os().skip(1)) {
        Ok(action) => action,
        Err(error) => {
            if let Err(render_error) = emit_cli_error(&error) {
                eprintln!("{render_error}");
            }
            process::exit(error.exit_code());
        }
    };

    match cli_action {
        CliAction::ShowHelp => {
            println!("{}", usage_text());
            return Ok(());
        }
        CliAction::RunCommand(command) => {
            let output_mode = command.output;
            if let Err(error) = execute_command(command) {
                let cli_error = runtime_error(output_mode, error);
                if let Err(render_error) = emit_cli_error(&cli_error) {
                    eprintln!("{render_error}");
                }
                process::exit(cli_error.exit_code());
            }
            return Ok(());
        }
        CliAction::RunTui => {}
    }

    let mut guard = TerminalGuard::new()?;
    let mut app = App::new();
    app.record_current_screen_usage();
    run_app(&mut guard.terminal, app)
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mut app: App) -> io::Result<()> {
    let mut clock = RuntimeClock::new(Duration::from_millis(100));

    loop {
        app.poll_wakatime_status();
        terminal.draw(|frame| ui::render(frame, &app))?;

        if let Some(event) = read_terminal_event(clock.poll_timeout())? {
            dispatch_runtime_event(&mut app, event);
        }
        if let Some(elapsed) = clock.timer_elapsed_if_due(app.is_running()) {
            dispatch_runtime_event(&mut app, RuntimeEvent::TimerElapsed(elapsed));
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn read_terminal_event(timeout: Duration) -> io::Result<Option<RuntimeEvent>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }

    let event = match event::read()? {
        Event::Key(key) if should_handle_key(&key) => Some(RuntimeEvent::Key(key)),
        Event::Paste(text) => Some(RuntimeEvent::Paste(text)),
        _ => None,
    };
    Ok(event)
}

fn dispatch_runtime_event(app: &mut App, event: RuntimeEvent) {
    match event {
        RuntimeEvent::Key(key) => app.handle_key(key),
        RuntimeEvent::Paste(text) => app.handle_paste(text),
        RuntimeEvent::TimerElapsed(elapsed) => advance_running_timer(app, elapsed),
    }
}

fn advance_running_timer(app: &mut App, elapsed: TimerElapsed) {
    for _ in 0..elapsed.elapsed_secs {
        app.on_tick(elapsed.is_catchup);
    }
    // Advance WakaTime once per UI frame to avoid burst heartbeats
    // after a suspend/resume catch-up.
    app.on_wakatime_elapsed(elapsed.elapsed_secs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_clock_accumulates_subsecond_ticks() {
        let mut clock = RuntimeClock::new(Duration::from_millis(100));

        assert_eq!(clock.advance_by(true, 400), None);
        assert_eq!(
            clock.advance_by(true, 600),
            Some(TimerElapsed {
                elapsed_secs: 1,
                is_catchup: false,
            })
        );
    }

    #[test]
    fn runtime_clock_marks_multi_second_catchup() {
        let mut clock = RuntimeClock::new(Duration::from_millis(100));

        assert_eq!(
            clock.advance_by(true, 2500),
            Some(TimerElapsed {
                elapsed_secs: 2,
                is_catchup: true,
            })
        );
        assert_eq!(clock.tick_accumulator_ms, 500);
    }

    #[test]
    fn runtime_clock_clears_accumulator_when_timer_is_not_running() {
        let mut clock = RuntimeClock::new(Duration::from_millis(100));

        assert_eq!(clock.advance_by(true, 900), None);
        assert_eq!(clock.advance_by(false, 500), None);
        assert_eq!(
            clock.advance_by(true, 500),
            None,
            "stale partial time must not leak into a newly started timer"
        );
    }
}
