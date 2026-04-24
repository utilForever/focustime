mod app;
mod blocker;
mod cli;
mod config;
mod notifications;
mod schedule;
mod session_recovery;
mod stats;
mod task_labels;
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
        Event,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;
use app::should_handle_key;
use cli::{
    CliAction, OutputMode, emit_cli_error, execute_command, parse_args_with_contract,
    runtime_error, usage_text,
};

/// RAII guard that restores the terminal on drop, ensuring cleanup on any exit path.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
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

    let mut app = App::new();
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
        CliAction::RunTui { start_immediately } => {
            if start_immediately && let Err(error) = app.start_focus_for_cli() {
                let cli_error = runtime_error(OutputMode::Text, error);
                if let Err(render_error) = emit_cli_error(&cli_error) {
                    eprintln!("{render_error}");
                }
                process::exit(cli_error.exit_code());
            }
        }
    }

    let mut guard = TerminalGuard::new()?;
    run_app(&mut guard.terminal, app)
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mut app: App) -> io::Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let mut tick_accumulator: u64 = 0; // milliseconds accumulated towards next second

    loop {
        app.poll_wakatime_status();
        terminal.draw(|frame| ui::render(frame, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        handle_terminal_event(&mut app, timeout)?;
        tick_timer_if_due(&mut app, &mut last_tick, &mut tick_accumulator, tick_rate);

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_terminal_event(app: &mut App, timeout: Duration) -> io::Result<()> {
    if !event::poll(timeout)? {
        return Ok(());
    }

    match event::read()? {
        Event::Key(key) if should_handle_key(&key) => app.handle_key(key),
        Event::Paste(text) => app.handle_paste(text),
        _ => {}
    }
    Ok(())
}

fn tick_timer_if_due(
    app: &mut App,
    last_tick: &mut Instant,
    tick_accumulator: &mut u64,
    tick_rate: Duration,
) {
    if last_tick.elapsed() < tick_rate {
        return;
    }

    let elapsed_ms = last_tick.elapsed().as_millis() as u64;
    *last_tick = Instant::now();
    if !app.is_running() {
        *tick_accumulator = 0;
        return;
    }

    advance_running_timer(app, tick_accumulator, elapsed_ms);
}

fn advance_running_timer(app: &mut App, tick_accumulator: &mut u64, elapsed_ms: u64) {
    *tick_accumulator += elapsed_ms;
    let mut elapsed_secs: u64 = 0;
    while *tick_accumulator >= 1000 {
        *tick_accumulator -= 1000;
        elapsed_secs += 1;
    }
    let is_catchup = elapsed_secs > 1;
    for _ in 0..elapsed_secs {
        app.on_tick(is_catchup);
    }
    // Advance WakaTime once per UI frame to avoid burst heartbeats
    // after a suspend/resume catch-up.
    if elapsed_secs > 0 {
        app.on_wakatime_elapsed(elapsed_secs);
    }
}
