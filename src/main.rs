mod app;
mod blocker;
mod config;
mod timer;
mod ui;
mod wakatime;

use std::{
    io,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;

/// RAII guard that restores the terminal on drop, ensuring cleanup on any exit path.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(e) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
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
                let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
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
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}

fn main() -> io::Result<()> {
    let mut guard = TerminalGuard::new()?;
    run_app(&mut guard.terminal)
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut app = App::new();
    let mut last_tick = Instant::now();
    let mut tick_accumulator: u64 = 0; // milliseconds accumulated towards next second

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
        {
            app.handle_key(key);
        }

        if last_tick.elapsed() >= tick_rate {
            let elapsed_ms = last_tick.elapsed().as_millis() as u64;
            last_tick = Instant::now();

            if app.is_running() {
                tick_accumulator += elapsed_ms;
                let mut elapsed_secs: u64 = 0;
                while tick_accumulator >= 1000 {
                    tick_accumulator -= 1000;
                    elapsed_secs += 1;
                    app.on_tick();
                }
                // Advance WakaTime once per UI frame to avoid burst heartbeats
                // after a suspend/resume catch-up.
                if elapsed_secs > 0 {
                    app.on_wakatime_elapsed(elapsed_secs);
                }
            } else {
                tick_accumulator = 0;
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
