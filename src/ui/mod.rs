//! The interactive pager: the terminal's lifetime, and the loop that reads
//! events and paints frames.

pub mod app;
pub mod input;
pub mod search;
pub mod view;

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};

use anyhow::{Context, Result};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

use crate::document::Document;
use crate::theme::Theme;
use crate::ui::app::App;

/// Why the pager woke up. Every source of change posts one of these onto a
/// single channel, so the loop blocks on one `recv` and there is no timer
/// anywhere: an idle reader costs nothing.
enum Wake {
    /// The terminal reported something.
    Input(event::Event),
    /// Terminal input ended, so nothing can reach the pager any more.
    InputLost,
}

/// How the pager was asked to run.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// `--mouse`. Off by default because capture takes the terminal's own
    /// text selection away.
    pub mouse: bool,
    /// `--width`, when the reader pinned one.
    pub width: Option<u16>,
    /// `--root`, when the reader named the vault themselves.
    pub root: Option<std::path::PathBuf>,
}

/// Open `document` in the alternate screen, and return when the reader quits.
pub fn run(document: Document, theme: Theme, options: Options) -> Result<()> {
    // Before `try_init`, whose own hook chains onto whatever is installed:
    // ratatui restores raw mode and the alternate screen, then this releases
    // the mouse, so a panic never leaves the terminal swallowing clicks.
    if options.mouse {
        release_mouse_on_panic();
    }

    let (tx, rx) = mpsc::channel();

    let mut terminal = ratatui::try_init().context("cannot open the terminal")?;
    spawn_input(tx);

    // Past this point nothing may use `?`: the alternate screen is up and raw
    // mode is on, so an early return would hand the reader's shell back inside
    // it. Every failure has to fall through to the teardown below instead.
    let outcome = capture_mouse(options.mouse)
        .and_then(|()| terminal.size().context("cannot measure the terminal"))
        .map(|area| App::new(document, theme, &options, area))
        .and_then(|mut app| event_loop(&mut terminal, &mut app, &rx));

    // Teardown on every path, including the failing one.
    if options.mouse {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    outcome
}

/// Draw, then block until something happens. There is no tick: an idle pager
/// costs nothing, and a burst of wakes — a held `j`, a spin of the wheel — is
/// drained into a single frame instead of one frame apiece.
fn event_loop(terminal: &mut DefaultTerminal, app: &mut App, rx: &Receiver<Wake>) -> Result<()> {
    loop {
        terminal.draw(|frame| view::draw(frame, app)).context("cannot draw")?;

        // Every sender is gone, so nothing can ever wake the pager again.
        let Ok(wake) = rx.recv() else { return Ok(()) };
        handle(app, wake);
        while let Ok(wake) = rx.try_recv() {
            handle(app, wake);
        }

        if app.quit {
            return Ok(());
        }
    }
}

fn handle(app: &mut App, wake: Wake) {
    match wake {
        Wake::Input(event) => apply(app, &event),
        // Nothing can reach the pager any more, so there is nothing left for it
        // to do but leave the terminal the way it found it.
        Wake::InputLost => app.quit = true,
    }
}

fn apply(app: &mut App, event: &event::Event) {
    if let Some(action) = input::action(event, app.mode) {
        app.apply(action);
    }
}

/// Forward terminal events onto the pager's channel.
///
/// Detached, and deliberately: a blocking `event::read` cannot be interrupted,
/// so the thread ends when the process does. It holds nothing the teardown
/// needs.
fn spawn_input(tx: Sender<Wake>) {
    std::thread::spawn(move || {
        while let Ok(event) = event::read() {
            if tx.send(Wake::Input(event)).is_err() {
                return;
            }
        }
        let _ = tx.send(Wake::InputLost);
    });
}

fn capture_mouse(wanted: bool) -> Result<()> {
    if wanted {
        execute!(io::stdout(), EnableMouseCapture).context("cannot capture the mouse")?;
    }
    Ok(())
}

fn release_mouse_on_panic() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        hook(info);
    }));
}
