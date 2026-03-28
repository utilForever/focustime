mod app;
mod blocker;
mod config;
mod timer;
mod ui;

use std::{
    io,
    time::{Duration, Instant},
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::{App, InputMode};

/// RAII guard that restores the terminal on drop, ensuring cleanup on any exit path.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    keyboard_enhancement: bool,
}

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();

        // Push keyboard enhancement flags when the terminal supports it (e.g. Windows
        // Terminal, Kitty, WezTerm). This forces the kitty keyboard protocol which
        // correctly distinguishes Press / Repeat / Release events and prevents
        // duplicate key actions caused by ConPTY forwarding both keydown and keyup
        // as Press events.
        let keyboard_enhancement =
            crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
        if keyboard_enhancement
            && let Err(e) = execute!(
                stdout,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
            )
        {
            let _ = disable_raw_mode();
            return Err(e);
        }

        if let Err(e) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
            if keyboard_enhancement {
                let _ = execute!(stdout, PopKeyboardEnhancementFlags);
            }
            let _ = disable_raw_mode();
            return Err(e);
        }

        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self {
                terminal,
                keyboard_enhancement,
            }),
            Err(e) => {
                let mut stdout = io::stdout();
                if keyboard_enhancement {
                    let _ = execute!(stdout, PopKeyboardEnhancementFlags);
                }
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
        if self.keyboard_enhancement {
            let _ = execute!(self.terminal.backend_mut(), PopKeyboardEnhancementFlags);
        }
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

        // Block up to `timeout` waiting for the first event, then drain every
        // event that is already queued.  In Normal mode we deduplicate by key
        // code: press + release (or any other duplicate delivery) fires exactly
        // one action per physical keypress regardless of what the terminal
        // sends.  In AddingSite mode every event is forwarded so fast typing is
        // not impaired.
        if event::poll(timeout)? {
            let mut seen: std::collections::HashSet<KeyCode> = std::collections::HashSet::new();
            loop {
                if let Event::Key(key) = event::read()?
                    && key.kind != KeyEventKind::Release
                {
                    let should_process = match app.input_mode {
                        InputMode::Normal => seen.insert(key.code),
                        InputMode::AddingSite => true,
                    };
                    if should_process {
                        app.handle_key(key);
                    }
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            let elapsed_ms = last_tick.elapsed().as_millis() as u64;
            last_tick = Instant::now();

            if app.is_running() {
                tick_accumulator += elapsed_ms;
                while tick_accumulator >= 1000 {
                    tick_accumulator -= 1000;
                    app.on_tick();
                }
            } else {
                tick_accumulator = 0;
            }
        }

        if app.should_quit {
            app.cleanup();
            break;
        }
    }

    Ok(())
}
