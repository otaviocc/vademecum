mod cli;
mod config;
mod document;
mod markdown;
mod render;
mod theme;
mod ui;
mod watch;

use std::io::{BufWriter, IsTerminal, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Parser;
use unicode_width::UnicodeWidthStr;

use crate::cli::{Cli, ColorChoice};
use crate::document::Document;
use crate::markdown::links::{LinkKind, Links, ResolveError, Target};
use crate::render::layout::Ctx;

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

    // A root that cannot be read is a mistake in the command, so it is caught
    // here — before the document is loaded, before the alternate screen, and
    // before the lazy walk that would otherwise swallow it.
    check_root(cli.root.as_deref())?;
    check_watch(cli.watch, cli.path.as_deref())?;

    let document = load(&cli)?;

    // Where the links go is a question about the vault, not about the theme,
    // so it answers before a theme is loaded and before anything is rendered.
    if cli.resolve_links {
        let blocks = markdown::ast::parse(&document.source);
        let links = Links::new(document, cli.root.as_deref());
        return finished(report_links(&stdout, &markdown::links::collect(&blocks), &links));
    }

    // Loading and theming happen before the alternate screen, so their errors
    // reach stderr and a non-zero exit rather than a statusbar nobody sees.
    let theme = theme::loader::load(cli.config.as_deref(), cli.theme.as_deref())?;

    if is_terminal && !cli.plain {
        return ui::run(document, theme, ui::Options { mouse: cli.mouse, width: cli.width, root: cli.root, watch: cli.watch });
    }

    let blocks = markdown::ast::parse(&document.source);
    let links = Links::new(document, cli.root.as_deref());
    let lines = render::layout::render(&blocks, &Ctx::new(&theme, &links), width(&cli, is_terminal));

    // Layout does not print its own warnings — in the pager it would paint over
    // the alternate screen — so stdout mode collects them and puts them where
    // they have always gone.
    for warning in render::code::take_warnings() {
        eprintln!("vademecum: {warning}");
    }

    let mut out = BufWriter::new(stdout.lock());
    finished(render::ansi::write_lines(&mut out, &lines, color(&cli, is_terminal)).and_then(|()| out.flush()))
}

/// One name per line, which is what both listing flags print.
fn list(stdout: &std::io::Stdout, names: &[String]) -> std::io::Result<()> {
    let mut out = BufWriter::new(stdout.lock());
    names.iter().try_for_each(|name| writeln!(out, "{name}")).and_then(|()| out.flush())
}

/// `--resolve-links`: one line per link, in source order — kind, destination,
/// and what resolution made of it. The columns are padded to the widest entry
/// so a document's links read as a table rather than as a ragged list.
fn report_links(stdout: &std::io::Stdout, links: &[LinkKind], context: &Links) -> std::io::Result<()> {
    let destinations: Vec<String> = links.iter().map(LinkKind::destination).collect();
    let kind_width = links.iter().map(|link| link.name().width()).max().unwrap_or(0);
    let destination_width = destinations.iter().map(|destination| destination.width()).max().unwrap_or(0);

    let mut out = BufWriter::new(stdout.lock());
    for (link, destination) in links.iter().zip(&destinations) {
        let target = target_of(context.resolve(link), context);
        writeln!(out, "{}", row(link.name(), kind_width, destination, destination_width, &target))?;
    }
    out.flush()
}

/// One row of the report, padded to the column widths.
///
/// The padding is counted in display columns, not bytes: `{:width$}` counts
/// characters, and a destination like `[[日本語ノート]]` is six characters wide
/// and eighteen bytes long, either of which would misalign the table.
fn row(kind: &str, kind_width: usize, destination: &str, destination_width: usize, target: &str) -> String {
    let pad = |text: &str, width: usize| " ".repeat(width.saturating_sub(text.width()));
    format!("{kind}{}  {destination}{}  -> {target}", pad(kind, kind_width), pad(destination, destination_width))
}

/// The third column: a path, `-` for a link that names no file, or the reason
/// a Local or Wiki link could not be followed.
fn target_of(resolved: Result<Target, ResolveError>, context: &Links) -> String {
    match resolved {
        Ok(Target::File(path)) => path.display().to_string(),
        // The document it was written in is the document it points at.
        Ok(Target::SameDocument) => {
            context.document.path.as_deref().map_or_else(|| "-".to_string(), |path| path.display().to_string())
        }
        Ok(Target::External) => "-".to_string(),
        Err(ResolveError::NotFound(_)) => "(broken)".to_string(),
        Err(ResolveError::Ambiguous { candidates, .. }) => format!("(ambiguous: {candidates})"),
    }
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

/// `--root` names the vault wikilinks are searched in. The walk that searches
/// it is silent about an unreadable directory, which is right for one buried in
/// a vault and wrong for the one the reader typed: it would render a document
/// of broken links and explain none of them.
///
/// `read_dir` answers all three questions at once — does it exist, is it a
/// directory, can it be read — and is exactly what the walk would go on to do.
/// A *discovered* root needs no check, having been found by looking.
fn check_root(root: Option<&Path>) -> Result<()> {
    match root {
        Some(root) => {
            std::fs::read_dir(root).map(drop).with_context(|| format!("--root {}: cannot be read as a directory", root.display()))
        }
        None => Ok(()),
    }
}

/// `--watch` asks to re-read a file whenever it changes. A pipe names no file
/// to re-read and offers no second chance to be read at all, so asking to watch
/// one is a mistake in the command.
///
/// The order matters: this runs *before* the document is loaded, because
/// `Document::from_stdin` reads to EOF. Checking afterwards would leave
/// `vademecum --watch -` typed at a terminal waiting for input it was never
/// going to use.
fn check_watch(watch: bool, path: Option<&Path>) -> Result<()> {
    match watch && path == Some(Path::new("-")) {
        true => bail!("--watch: cannot watch stdin; pass a file path instead"),
        false => Ok(()),
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
    fn a_report_row_is_padded_in_columns_rather_than_bytes() {
        // Six characters, twelve columns, eighteen bytes: every one of the
        // three gives a different answer, and only columns line the table up.
        let wide = row("wiki", 8, "日本語ノート", 12, "(broken)");
        let plain = row("wiki", 8, "abcdefghijkl", 12, "(broken)");
        assert_eq!(wide.find("->"), plain.find("->").map(|_| wide.find("->").expect("an arrow")));
        assert_eq!(
            wide.split(" -> ").next().expect("a left column").width(),
            plain.split(" -> ").next().expect("a left column").width()
        );
        assert!(wide.starts_with("wiki      日本語ノート  -> "), "{wide:?}");
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

    #[test]
    fn watching_needs_a_file_and_says_so_before_reading_anything() {
        assert!(check_watch(true, Some(Path::new("-"))).is_err());
        assert!(check_watch(true, Some(Path::new("notes.md"))).is_ok());
        // Without the flag a dash is the ordinary way to read a pipe.
        assert!(check_watch(false, Some(Path::new("-"))).is_ok());
        // No path at all is `load`'s complaint to make, not this one's.
        assert!(check_watch(true, None).is_ok());
    }

    #[test]
    fn a_root_is_checked_only_when_one_was_named() {
        assert!(check_root(None).is_ok(), "a discovered root was found by looking, so it exists");
        assert!(check_root(Some(Path::new("tests/fixtures/vault"))).is_ok());
        assert!(check_root(Some(Path::new("tests/fixtures/no-such-vault"))).is_err());
        // A file is not a vault, and `read_dir` is what says so.
        assert!(check_root(Some(Path::new("tests/fixtures/note.md"))).is_err());
    }
}
