//! The interactive pager: the terminal's lifetime, and the loop that reads
//! events and paints frames.

pub mod app;
pub mod input;
pub mod search;
pub mod view;

use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

use crate::document::Document;
use crate::theme::Theme;
use crate::ui::app::App;

/// How the pager was asked to run.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// `--mouse`. Off by default because capture takes the terminal's own
    /// text selection away.
    pub mouse: bool,
    /// `--width`, when the reader pinned one.
    pub width: Option<u16>,
}

/// Open `document` in the alternate screen, and return when the reader quits.
pub fn run(document: Document, theme: Theme, options: Options) -> Result<()> {
    // Before `try_init`, whose own hook chains onto whatever is installed:
    // ratatui restores raw mode and the alternate screen, then this releases
    // the mouse, so a panic never leaves the terminal swallowing clicks.
    if options.mouse {
        release_mouse_on_panic();
    }

    let mut terminal = ratatui::try_init().context("cannot open the terminal")?;
    if options.mouse {
        execute!(io::stdout(), EnableMouseCapture).context("cannot capture the mouse")?;
    }

    let outcome = terminal
        .size()
        .context("cannot measure the terminal")
        .map(|area| App::new(document, theme, options.width, area))
        .and_then(|mut app| event_loop(&mut terminal, &mut app));

    // Teardown on every path, including the failing one.
    if options.mouse {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    outcome
}

/// Draw, then block until something happens. There is no tick: an idle pager
/// costs nothing, and a burst of events — a held `j`, a spin of the wheel —
/// collapses into a single frame instead of one frame apiece.
fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| view::draw(frame, app)).context("cannot draw")?;

        apply(app, &event::read().context("cannot read input")?);
        while event::poll(Duration::ZERO).context("cannot read input")? {
            apply(app, &event::read().context("cannot read input")?);
        }

        if app.quit {
            return Ok(());
        }
    }
}

fn apply(app: &mut App, event: &event::Event) {
    if let Some(action) = input::action(event, app.mode) {
        app.apply(action);
    }
}

fn release_mouse_on_panic() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        hook(info);
    }));
}
