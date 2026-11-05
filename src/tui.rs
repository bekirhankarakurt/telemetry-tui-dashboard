//! Terminal setup, teardown and the main event loop.

use std::io::{self, Stdout};
use std::panic;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::net::Msg;
use crate::ui;

/// Restores the terminal when dropped, including during unwinding.
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Leave raw mode and the alternate screen. Safe to call more than once.
pub fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

/// Enter raw mode and the alternate screen.
pub fn setup() -> Result<(Terminal<CrosstermBackend<Stdout>>, TerminalGuard)> {
    enable_raw_mode().context("cannot enable raw mode (is stdout a terminal?)")?;
    let guard = TerminalGuard;
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen).context("cannot enter the alternate screen")?;
    let terminal = Terminal::new(CrosstermBackend::new(out))?;
    Ok((terminal, guard))
}

/// Run until the user quits.
pub fn run(app: &mut App, rx: &Receiver<Msg>, refresh: Duration) -> Result<()> {
    let (mut terminal, _guard) = setup()?;
    terminal.clear()?;
    while !app.quit {
        while let Ok(msg) = rx.try_recv() {
            app.handle_msg(msg);
        }
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(refresh)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.on_key(key);
                }
            }
        }
    }
    Ok(())
}
