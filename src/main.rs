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
use clap::{CommandFactory, Parser};
use clap_complete::Shell;
use unicode_width::UnicodeWidthStr;

use crate::cli::{Cli, ColorChoice};
use crate::document::Document;
use crate::markdown::links::{LinkKind, Links, ResolveError, Target};
use crate::render::layout::Ctx;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let stdout = std::io::stdout();
    let is_terminal = stdout.is_terminal();

    if cli.list_themes {
        return finished(list(&stdout, &theme::loader::available()));
    }
    if cli.list_syntax_themes {
        return finished(list(&stdout, &render::code::available()));
    }
    if let Some(shell) = cli.completions {
        return finished(completions(&stdout, shell));
    }
    if cli.man {
        return finished(man(&stdout));
    }

    check_root(cli.root.as_deref())?;
    check_watch(cli.watch, cli.path.as_deref())?;

    let document = load(&cli)?;

    if cli.resolve_links {
        let blocks = markdown::ast::parse(&document.source);
        let links = Links::new(document, cli.root.as_deref());
        return finished(report_links(&stdout, &markdown::links::collect(&blocks), &links));
    }

    let theme = theme::loader::load(cli.config.as_deref(), cli.theme.as_deref())?;

    if is_terminal && !cli.plain {
        return ui::run(
            document,
            theme,
            ui::Options { mouse: !cli.no_mouse, width: cli.width, root: cli.root, watch: cli.watch },
        );
    }

    let blocks = markdown::ast::parse(&document.source);
    let links = Links::new(document, cli.root.as_deref());
    let lines = render::layout::render(&blocks, &Ctx::new(&theme, &links), width(&cli, is_terminal));

    for warning in render::code::take_warnings() {
        eprintln!("vademecum: {warning}");
    }

    let mut out = BufWriter::new(stdout.lock());
    finished(render::ansi::write_lines(&mut out, &lines, color(&cli, is_terminal)).and_then(|()| out.flush()))
}

fn list(stdout: &std::io::Stdout, names: &[String]) -> std::io::Result<()> {
    let mut out = BufWriter::new(stdout.lock());
    names.iter().try_for_each(|name| writeln!(out, "{name}")).and_then(|()| out.flush())
}

fn completions(stdout: &std::io::Stdout, shell: Shell) -> std::io::Result<()> {
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut command, name, &mut script);
    write_out(stdout, &script)
}

fn man(stdout: &std::io::Stdout) -> std::io::Result<()> {
    let mut roff = Vec::new();
    clap_mangen::Man::new(Cli::command()).render(&mut roff)?;
    write_out(stdout, &roff)
}

fn write_out(stdout: &std::io::Stdout, bytes: &[u8]) -> std::io::Result<()> {
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(bytes).and_then(|()| out.flush())
}

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

fn row(kind: &str, kind_width: usize, destination: &str, destination_width: usize, target: &str) -> String {
    let pad = |text: &str, width: usize| " ".repeat(width.saturating_sub(text.width()));
    format!("{kind}{}  {destination}{}  -> {target}", pad(kind, kind_width), pad(destination, destination_width))
}

fn target_of(resolved: Result<Target, ResolveError>, context: &Links) -> String {
    match resolved {
        Ok(Target::File(path)) => path.display().to_string(),
        Ok(Target::SameDocument) => {
            context.document.path.as_deref().map_or_else(|| "-".to_string(), |path| path.display().to_string())
        }
        Ok(Target::External) => "-".to_string(),
        Err(ResolveError::NotFound(_)) => "(broken)".to_string(),
        Err(ResolveError::Ambiguous { candidates, .. }) => format!("(ambiguous: {candidates})"),
    }
}

fn finished(written: std::io::Result<()>) -> Result<()> {
    match written {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => other.context("cannot write to stdout"),
    }
}

fn check_root(root: Option<&Path>) -> Result<()> {
    match root {
        Some(root) => {
            std::fs::read_dir(root).map(drop).with_context(|| format!("--root {}: cannot be read as a directory", root.display()))
        }
        None => Ok(()),
    }
}

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
        assert!(check_watch(false, Some(Path::new("-"))).is_ok());
        assert!(check_watch(true, None).is_ok());
    }

    #[test]
    fn a_root_is_checked_only_when_one_was_named() {
        assert!(check_root(None).is_ok(), "a discovered root was found by looking, so it exists");
        assert!(check_root(Some(Path::new("tests/fixtures/vault"))).is_ok());
        assert!(check_root(Some(Path::new("tests/fixtures/no-such-vault"))).is_err());
        assert!(check_root(Some(Path::new("tests/fixtures/note.md"))).is_err());
    }
}
