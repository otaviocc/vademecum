//! The interactive pager: the terminal's lifetime, and the loop that reads
//! events and paints frames.

pub mod app;
pub mod input;
pub mod search;
pub mod view;

use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};

use anyhow::{Context, Result};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

use crate::document::Document;
use crate::theme::Theme;
use crate::ui::app::App;
use crate::ui::input::Action;
use crate::watch;

/// Why the pager woke up. Every source of change posts one of these onto a
/// single channel, so the loop blocks on one `recv` and there is no timer
/// anywhere: an idle reader costs nothing.
enum Wake {
    /// The terminal reported something.
    Input(event::Event),
    /// Something in the watched directory was written.
    Changed(watch::Change),
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
    /// `--watch`. Only the pager honours it: stdout mode writes the document
    /// once and has nothing to redraw.
    pub watch: bool,
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

    // Before the terminal is taken, so a watch the reader asked for and cannot
    // have is an error on stderr and a non-zero exit rather than a failure
    // inside the alternate screen, where `?` is not allowed.
    let mut watcher = start_watching(&options, document.path.as_deref(), &tx)?;

    let mut terminal = ratatui::try_init().context("cannot open the terminal")?;
    spawn_input(tx);

    // Past this point nothing may use `?`: the alternate screen is up and raw
    // mode is on, so an early return would hand the reader's shell back inside
    // it. Every failure has to fall through to the teardown below instead.
    let outcome = capture_mouse(options.mouse)
        .and_then(|()| terminal.size().context("cannot measure the terminal"))
        .map(|area| App::new(document, theme, &options, area))
        .and_then(|mut app| event_loop(&mut terminal, &mut app, &rx, &mut watcher));

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
fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rx: &Receiver<Wake>,
    watcher: &mut Option<watch::Watcher>,
) -> Result<()> {
    loop {
        terminal.draw(|frame| view::draw(frame, app)).context("cannot draw")?;

        // Every sender is gone, so nothing can ever wake the pager again.
        let Ok(wake) = rx.recv() else { return Ok(()) };
        handle(app, wake, watcher);
        while let Ok(wake) = rx.try_recv() {
            handle(app, wake, watcher);
        }

        if app.quit {
            return Ok(());
        }

        // After the burst, not inside it: a change that arrived for the
        // document the reader has just left is answered against the name that
        // was armed when it was written, and only then does the watch move.
        rearm(app, watcher);
    }
}

fn handle(app: &mut App, wake: Wake, watcher: &mut Option<watch::Watcher>) {
    match wake {
        Wake::Input(event) => apply(app, &event),
        // Whatever else was written in that directory is not the reader's
        // business, and re-reading for it would cost them their place.
        Wake::Changed(change) => {
            if watcher.as_ref().is_some_and(|watcher| watcher.wrote(&change)) {
                app.apply(Action::Reload);
            }
        }
        // Nothing can reach the pager any more, so there is nothing left for it
        // to do but leave the terminal the way it found it.
        Wake::InputLost => app.quit = true,
    }
}

/// Keep the watch pointed at what the reader is reading. `arm` is idempotent,
/// so this costs nothing on the keypresses that go nowhere.
fn rearm(app: &mut App, watcher: &mut Option<watch::Watcher>) {
    let (Some(watcher), Some(path)) = (watcher.as_mut(), app.path()) else { return };
    // Owned, and the borrow of the document ends with it: the report that
    // follows needs the pager mutably.
    let failure = watcher.arm(path).err().map(|error| error.to_string());
    if let Some(failure) = failure {
        app.report(&failure);
    }
}

/// The watch, when `--watch` asked for one and the document has a file to
/// watch. Armed here, before the first frame, so the reader's first save
/// reloads like every one after it.
fn start_watching(options: &Options, path: Option<&Path>, tx: &Sender<Wake>) -> Result<Option<watch::Watcher>> {
    let (true, Some(path)) = (options.watch, path) else { return Ok(None) };

    let tx = tx.clone();
    let mut watcher = watch::Watcher::new(move |change| {
        let _ = tx.send(Wake::Changed(change));
    })
    .context("--watch")?;
    watcher.arm(path).context("--watch")?;
    Ok(Some(watcher))
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
