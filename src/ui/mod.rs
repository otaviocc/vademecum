//! The interactive pager: the terminal's lifetime, and the event loop.

pub mod app;
pub mod clipboard;
pub mod input;
pub mod listing;
pub mod outline;
pub mod properties;
pub mod search;
pub mod tty;
pub mod view;

use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};

use anyhow::{Context, Result, bail};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

use crate::document::Document;
use crate::theme::Theme;
use crate::ui::app::App;
use crate::ui::input::Action;
use crate::watch;

enum Wake {
    Input(event::Event),
    Changed(watch::Change),
    InputLost(String),
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub mouse: bool,
    pub width: Option<u16>,
    pub root: Option<std::path::PathBuf>,
    pub watch: bool,
}

pub fn run(document: Document, theme: Theme, options: Options) -> Result<()> {
    if options.mouse {
        release_mouse_on_panic();
    }

    let (tx, rx) = mpsc::channel();

    let mut watcher = start_watching(&options, document.path.as_deref(), &tx)?;

    tty::adopt_controlling_terminal()?;

    let mut terminal = ratatui::try_init().context("cannot open the terminal")?;
    spawn_input(tx);

    let outcome = capture_mouse(options.mouse)
        .and_then(|()| terminal.size().context("cannot measure the terminal"))
        .map(|area| App::new(document, theme, &options, area))
        .and_then(|mut app| event_loop(&mut terminal, &mut app, &rx, &mut watcher));

    if options.mouse {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    outcome
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rx: &Receiver<Wake>,
    watcher: &mut Option<watch::Watcher>,
) -> Result<()> {
    loop {
        terminal.draw(|frame| view::draw(frame, app)).context("cannot draw")?;

        let Ok(wake) = rx.recv() else { return Ok(()) };
        handle(app, wake, watcher)?;
        while let Ok(wake) = rx.try_recv() {
            handle(app, wake, watcher)?;
        }

        if app.quit {
            return Ok(());
        }

        rearm(app, watcher);
    }
}

fn handle(app: &mut App, wake: Wake, watcher: &mut Option<watch::Watcher>) -> Result<()> {
    match wake {
        Wake::Input(event) => apply(app, &event),
        Wake::Changed(change) => {
            if watcher.as_ref().is_some_and(|watcher| watcher.wrote(&change)) {
                app.apply(Action::Reload);
            }
        }
        Wake::InputLost(error) => bail!("cannot read keyboard input: {error}"),
    }
    drain_copy(app);
    Ok(())
}

fn drain_copy(app: &mut App) {
    let Some(text) = app.take_copy() else { return };
    if let Err(error) = clipboard::copy(&text) {
        app.report(&format!("could not copy: {error}"));
    }
}

fn rearm(app: &mut App, watcher: &mut Option<watch::Watcher>) {
    let (Some(watcher), Some(path)) = (watcher.as_mut(), app.path()) else { return };
    let failure = watcher.arm(path).err().map(|error| error.to_string());
    if let Some(failure) = failure {
        app.report(&failure);
    }
}

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
    let Some(action) = input::action(event, app.mode) else { return };
    app.apply(action);
}

fn spawn_input(tx: Sender<Wake>) {
    std::thread::spawn(move || {
        loop {
            let event = match event::read() {
                Ok(event) => event,
                Err(error) => {
                    let _ = tx.send(Wake::InputLost(error.to_string()));
                    return;
                }
            };
            if tx.send(Wake::Input(event)).is_err() {
                return;
            }
        }
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
