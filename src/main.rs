mod app;
mod timer;
mod ui;

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

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
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
                while tick_accumulator >= 1000 {
                    tick_accumulator -= 1000;
                    app.on_tick();
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
