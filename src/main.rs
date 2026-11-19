//! AMO Monitor — a modern, customizable terminal system monitor.

use std::io;
use std::panic;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::cursor::Hide;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use amo_monitor::app::App;
use amo_monitor::ui;

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("amo-monitor {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let _guard = TerminalGuard;

    let mut app = App::new();
    let mut last_tick = Instant::now();

    while app.running {
        terminal.draw(|frame| ui::render(frame, &app))?;

        let timeout = app.refresh_interval().min(Duration::from_millis(50));
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key);
                }
            }
        }

        if last_tick.elapsed() >= app.refresh_interval() {
            app.tick();
            last_tick = Instant::now();
        }
    }

    // Persist on clean shutdown as well (covers quick q after a config change).
    let _ = app.config.save();
    Ok(())
}
