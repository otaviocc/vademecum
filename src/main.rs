mod cli;
#[allow(dead_code, reason = "read once theme files land in milestone 2")]
mod config;
mod document;
mod markdown;
mod render;
mod theme;

use std::io::{BufWriter, IsTerminal, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Parser;

use crate::cli::{Cli, ColorChoice};
use crate::document::Document;
use crate::theme::Theme;

/// Wrap width when there is no terminal to measure.
const DEFAULT_WIDTH: usize = 100;
/// Leaves a column either side of the content.
const TERMINAL_MARGIN: usize = 2;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let stdout = std::io::stdout();
    let is_terminal = stdout.is_terminal();

    let document = load(&cli)?;
    let theme = Theme::default();
    let blocks = markdown::ast::parse(&document.source);
    let lines = render::layout::render(&blocks, &theme, width(&cli, is_terminal));

    let mut out = BufWriter::new(stdout.lock());
    render::ansi::write_lines(&mut out, &lines, color(&cli, is_terminal)).context("cannot write to stdout")?;
    out.flush().context("cannot write to stdout")
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

/// `--width`, else the terminal less a margin, capped at 100.
fn width(cli: &Cli, is_terminal: bool) -> usize {
    if let Some(width) = cli.width {
        return usize::from(width).max(1);
    }
    if !is_terminal {
        return DEFAULT_WIDTH;
    }
    match crossterm::terminal::size() {
        Ok((columns, _)) => usize::from(columns).saturating_sub(TERMINAL_MARGIN).clamp(1, DEFAULT_WIDTH),
        Err(_) => DEFAULT_WIDTH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("vademecum").chain(args.iter().copied()))
    }

    #[test]
    fn an_explicit_width_wins() {
        assert_eq!(width(&cli(&["--width", "80", "x.md"]), true), 80);
        assert_eq!(width(&cli(&["--width", "80", "x.md"]), false), 80);
    }

    #[test]
    fn a_pipe_gets_the_default_width() {
        assert_eq!(width(&cli(&["x.md"]), false), DEFAULT_WIDTH);
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
