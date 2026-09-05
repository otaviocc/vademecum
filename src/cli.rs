//! Command-line surface.
//!
//! This is the flag table from README.md, verbatim. Parsing only: no
//! validation, no environment handling, no terminal detection — those live in
//! the modules that consume these values.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// A modern `man` for reading Markdown.
#[derive(Debug, Parser)]
#[command(name = "vademecum", version, about, long_about = None)]
pub struct Cli {
    /// Markdown file to open, or `-` to read from stdin.
    pub path: Option<PathBuf>,

    /// Write styled output to stdout even when stdout is a TTY.
    #[arg(long)]
    pub plain: bool,

    /// ANSI colors in stdout mode.
    #[arg(long, value_name = "WHEN", value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Wrap width. Defaults to min(terminal width, 100).
    #[arg(long, value_name = "N")]
    pub width: Option<u16>,

    /// Built-in or user theme by name.
    #[arg(long, value_name = "NAME")]
    pub theme: Option<String>,

    /// Explicit theme file, overriding `--theme`.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Vault root for wikilink resolution.
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Re-render when the file changes on disk.
    #[arg(long)]
    pub watch: bool,

    /// Enable mouse capture (wheel scroll).
    #[arg(long)]
    pub mouse: bool,

    /// Print the available theme names and exit.
    #[arg(long)]
    pub list_themes: bool,

    /// Print the available syntect theme names and exit.
    #[arg(long)]
    pub list_syntax_themes: bool,
}

/// When to emit ANSI colors in stdout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    /// Color only when stdout is a TTY.
    Auto,
    /// Always color.
    Always,
    /// Never color.
    Never,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn defaults_are_inert() {
        let cli = Cli::parse_from(["vademecum", "README.md"]);
        assert_eq!(cli.path, Some(PathBuf::from("README.md")));
        assert_eq!(cli.color, ColorChoice::Auto);
        assert!(!cli.plain);
        assert!(!cli.watch);
        assert!(!cli.mouse);
        assert!(cli.width.is_none());
        assert!(cli.theme.is_none());
        assert!(cli.config.is_none());
        assert!(cli.root.is_none());
        assert!(!cli.list_themes);
        assert!(!cli.list_syntax_themes);
    }

    #[test]
    fn every_flag_parses() {
        let cli = Cli::parse_from([
            "vademecum",
            "--plain",
            "--color",
            "never",
            "--width",
            "80",
            "--theme",
            "catppuccin-mocha",
            "--config",
            "theme.toml",
            "--root",
            "notes",
            "--watch",
            "--mouse",
            "--list-themes",
            "--list-syntax-themes",
            "notes/index.md",
        ]);
        assert_eq!(cli.path, Some(PathBuf::from("notes/index.md")));
        assert!(cli.plain);
        assert_eq!(cli.color, ColorChoice::Never);
        assert_eq!(cli.width, Some(80));
        assert_eq!(cli.theme.as_deref(), Some("catppuccin-mocha"));
        assert_eq!(cli.config, Some(PathBuf::from("theme.toml")));
        assert_eq!(cli.root, Some(PathBuf::from("notes")));
        assert!(cli.watch);
        assert!(cli.mouse);
        assert!(cli.list_themes);
        assert!(cli.list_syntax_themes);
    }

    #[test]
    fn dash_means_stdin() {
        let cli = Cli::parse_from(["vademecum", "-"]);
        assert_eq!(cli.path, Some(PathBuf::from("-")));
    }
}
