//! Command-line surface.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "vademecum", version, about, long_about = None)]
pub struct Cli {
    #[arg(help = "Markdown file to open, or `-` to read from stdin")]
    pub path: Option<PathBuf>,

    #[arg(long, help = "Write styled output to stdout even when stdout is a TTY")]
    pub plain: bool,

    #[arg(long, value_name = "WHEN", value_enum, default_value_t = ColorChoice::Auto, help = "ANSI colors in stdout mode")]
    pub color: ColorChoice,

    #[arg(long, value_name = "N", help = "Maximum wrap width. Defaults to 100; a narrower terminal, less a column, wins")]
    pub width: Option<u16>,

    #[arg(long, value_name = "NAME", help = "Built-in or user theme by name")]
    pub theme: Option<String>,

    #[arg(long, value_name = "FILE", help = "Explicit theme file, overriding `--theme`")]
    pub config: Option<PathBuf>,

    #[arg(long, value_name = "DIR", help = "Vault root for wikilink resolution")]
    pub root: Option<PathBuf>,

    #[arg(long, help = "Re-render when the file changes on disk")]
    pub watch: bool,

    #[arg(long, help = "Disable mouse capture, keeping the terminal's own text selection")]
    pub no_mouse: bool,

    #[arg(long, help = "Print the available theme names and exit")]
    pub list_themes: bool,

    #[arg(long, help = "Print the available syntect theme names and exit")]
    pub list_syntax_themes: bool,

    #[arg(long, help = "Print each link and its resolved path, then exit")]
    pub resolve_links: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    #[value(help = "Color only when stdout is a TTY")]
    Auto,
    #[value(help = "Always color")]
    Always,
    #[value(help = "Never color")]
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
        assert!(!cli.no_mouse);
        assert!(cli.width.is_none());
        assert!(cli.theme.is_none());
        assert!(cli.config.is_none());
        assert!(cli.root.is_none());
        assert!(!cli.list_themes);
        assert!(!cli.list_syntax_themes);
        assert!(!cli.resolve_links);
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
            "--no-mouse",
            "--list-themes",
            "--list-syntax-themes",
            "--resolve-links",
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
        assert!(cli.no_mouse);
        assert!(cli.list_themes);
        assert!(cli.list_syntax_themes);
        assert!(cli.resolve_links);
    }

    #[test]
    fn dash_means_stdin() {
        let cli = Cli::parse_from(["vademecum", "-"]);
        assert_eq!(cli.path, Some(PathBuf::from("-")));
    }
}
