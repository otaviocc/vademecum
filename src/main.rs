mod cli;
mod config;
mod document;
mod markdown;
mod render;
mod theme;
mod ui;

use std::io::{BufWriter, IsTerminal, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Parser;

use crate::cli::{Cli, ColorChoice};
use crate::document::Document;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let stdout = std::io::stdout();
    let is_terminal = stdout.is_terminal();

    // Listing themes is a question about the installation, not about a
    // document, so it answers before anything asks for a path.
    if cli.list_themes {
        return finished(list(&stdout, &theme::loader::available()));
    }
    if cli.list_syntax_themes {
        return finished(list(&stdout, &render::code::available()));
    }

    let document = load(&cli)?;
    // Loading and theming happen before the alternate screen, so their errors
    // reach stderr and a non-zero exit rather than a statusbar nobody sees.
    let theme = theme::loader::load(cli.config.as_deref(), cli.theme.as_deref())?;

    if is_terminal && !cli.plain {
        return ui::run(document, theme, ui::Options { mouse: cli.mouse, width: cli.width });
    }

    let blocks = markdown::ast::parse(&document.source);
    let lines = render::layout::render(&blocks, &theme, width(&cli, is_terminal));

    let mut out = BufWriter::new(stdout.lock());
    finished(render::ansi::write_lines(&mut out, &lines, color(&cli, is_terminal)).and_then(|()| out.flush()))
}

/// One name per line, which is what both listing flags print.
fn list(stdout: &std::io::Stdout, names: &[String]) -> std::io::Result<()> {
    let mut out = BufWriter::new(stdout.lock());
    names.iter().try_for_each(|name| writeln!(out, "{name}")).and_then(|()| out.flush())
}

/// The outcome of writing to stdout. `vademecum README.md | less -R` is
/// documented usage, and quitting the pager early closes the pipe; so does
/// `--list-themes | head`. That is the reader leaving, not a fault.
fn finished(written: std::io::Result<()>) -> Result<()> {
    match written {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => other.context("cannot write to stdout"),
    }
}

fn load(cli: &Cli) -> Result<Document> {
    match cli.path.as_deref() {
        Some(path) if path == Path::new("-") => Document::from_stdin(),
        Some(path) => Document::load(path),
        None => bail!("no input: pass a Markdown file, or `-` to read stdin"),
    }
}

/// `--color`, with `NO_COLOR` forcing it off and `auto` following the terminal.
fn color(cli: &Cli, is_terminal: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        return false;
    }
    match cli.color {
        ColorChoice::Never => false,
        ColorChoice::Always => true,
        ColorChoice::Auto => is_terminal,
    }
}

/// `--width`, else the terminal less a margin, capped at 100. A pipe and a
/// failed size query both mean "no terminal to measure".
fn width(cli: &Cli, is_terminal: bool) -> usize {
    let columns = is_terminal.then(|| crossterm::terminal::size().ok()).flatten().map(|(columns, _)| columns);
    render::layout::wrap_width(cli.width, columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("vademecum").chain(args.iter().copied()))
    }

    #[test]
    fn a_pipe_is_measured_as_having_no_terminal() {
        assert_eq!(width(&cli(&["x.md"]), false), render::layout::wrap_width(None, None));
        assert_eq!(width(&cli(&["--width", "80", "x.md"]), false), 80);
    }

    #[test]
    fn never_overrides_the_terminal() {
        assert!(!color(&cli(&["--color", "never", "x.md"]), true));
    }

    #[test]
    fn a_missing_path_is_an_error_rather_than_a_hang() {
        assert!(load(&cli(&[])).is_err());
    }
}
