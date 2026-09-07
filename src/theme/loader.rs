//! Finding a theme, reading it, and merging it over the defaults.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ratatui::style::{Modifier, Style};
use serde::Deserialize;

use crate::theme::Theme;
use crate::theme::color::{ColorError, ColorSpec};
use crate::theme::elements::{self, Element, ElementFile};
use crate::theme::palette::{Palette, PaletteFile};

const BUILT_IN: [(&str, &str); 4] = [
    ("ansi", include_str!("../../themes/ansi.toml")),
    ("kanagawa-dragon", include_str!("../../themes/kanagawa-dragon.toml")),
    ("catppuccin-mocha", include_str!("../../themes/catppuccin-mocha.toml")),
    ("catppuccin-latte", include_str!("../../themes/catppuccin-latte.toml")),
];

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("{path}: cannot be read")]
    Unreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: not a theme file")]
    Malformed {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("{path}: {key}")]
    BadColor {
        path: String,
        key: String,
        #[source]
        source: ColorError,
    },
    #[error("{path}: {key}: {value:?} is not a modifier: expected bold, italic, underline, dim, reversed, or crossed_out")]
    BadModifier { path: String, key: String, value: String },
    #[error("no theme named {name:?}. Available: {available}")]
    Unknown { name: String, available: String },
}

#[derive(Debug, Default, Deserialize)]
struct ThemeFile {
    name: Option<String>,
    syntax_theme: Option<String>,
    #[serde(default)]
    palette: PaletteFile,
    #[serde(default)]
    elements: BTreeMap<String, ElementFile>,
    #[serde(flatten)]
    unknown: BTreeMap<String, toml::Value>,
}

pub fn load(config: Option<&Path>, name: Option<&str>) -> Result<Theme, ThemeError> {
    let (theme, warnings) = resolve(config, name, crate::config::config_dir().as_deref())?;
    for warning in warnings {
        eprintln!("vademecum: {warning}");
    }
    Ok(theme)
}

pub fn available() -> Vec<String> {
    available_in(crate::config::config_dir().as_deref())
}

fn available_in(config_dir: Option<&Path>) -> Vec<String> {
    let mut names: Vec<String> = BUILT_IN.iter().map(|(name, _)| (*name).to_owned()).collect();

    let mut user: Vec<String> = config_dir
        .map(|dir| dir.join("themes"))
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "toml"))
        .filter_map(|entry| entry.path().file_stem().map(|stem| stem.to_string_lossy().into_owned()))
        .filter(|stem| !names.contains(stem))
        .collect();
    user.sort_by_key(|name| name.to_lowercase());

    names.append(&mut user);
    names
}

fn resolve(config: Option<&Path>, name: Option<&str>, config_dir: Option<&Path>) -> Result<(Theme, Vec<String>), ThemeError> {
    if let Some(path) = config {
        return from_file(path);
    }
    if let Some(name) = name {
        return by_name(name, config_dir);
    }
    if let Some(path) = config_dir.map(|dir| dir.join("theme.toml")).filter(|path| path.is_file()) {
        return from_file(&path);
    }
    from_source(BUILT_IN[0].0, BUILT_IN[0].1)
}

fn by_name(name: &str, config_dir: Option<&Path>) -> Result<(Theme, Vec<String>), ThemeError> {
    let user: Option<PathBuf> = config_dir
        .filter(|_| addresses_a_theme(name))
        .map(|dir| dir.join("themes").join(format!("{name}.toml")))
        .filter(|path| path.is_file());
    if let Some(path) = user {
        return from_file(&path);
    }
    match BUILT_IN.iter().find(|(built_in, _)| *built_in == name) {
        Some((built_in, source)) => from_source(built_in, source),
        None => Err(ThemeError::Unknown { name: name.to_owned(), available: available_in(config_dir).join(", ") }),
    }
}

fn addresses_a_theme(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

fn from_file(path: &Path) -> Result<(Theme, Vec<String>), ThemeError> {
    let label = path.display().to_string();
    let source = std::fs::read_to_string(path).map_err(|source| ThemeError::Unreadable { path: label.clone(), source })?;
    from_source(&label, &source)
}

fn from_source(label: &str, source: &str) -> Result<(Theme, Vec<String>), ThemeError> {
    let file: ThemeFile = toml::from_str(source).map_err(|source| ThemeError::Malformed { path: label.to_owned(), source })?;
    file.into_theme(label)
}

impl ThemeFile {
    fn into_theme(self, path: &str) -> Result<(Theme, Vec<String>), ThemeError> {
        let mut warnings = Vec::new();
        for key in self.unknown.keys() {
            warnings.push(format!("{path}: unknown key `{key}`"));
        }

        let palette = self.resolve_palette(path, &mut warnings)?;
        let mut styles: Vec<Style> = Element::ALL.iter().map(|element| elements::default_style(*element, &palette)).collect();

        for (key, overrides) in &self.elements {
            let Some(element) = Element::from_key(key) else {
                warnings.push(format!("{path}: unknown element `{key}`"));
                continue;
            };
            for unknown in overrides.unknown.keys() {
                warnings.push(format!("{path}: elements.{key}: unknown key `{unknown}`"));
            }
            styles[element as usize] = patch(styles[element as usize], overrides, &palette, path, &format!("elements.{key}"))?;
        }

        let name = self.name.unwrap_or_else(|| path.to_owned());
        Ok((Theme { palette, styles, name, syntax_theme: self.syntax_theme }, warnings))
    }

    fn resolve_palette(&self, path: &str, warnings: &mut Vec<String>) -> Result<Palette, ThemeError> {
        let mut palette = Palette::default();
        for unknown in self.palette.unknown.keys() {
            warnings.push(format!("{path}: palette: unknown key `{unknown}`"));
        }
        for (slot, spec) in self.palette.slots() {
            let Some(spec) = spec else { continue };
            let color = spec.resolve().map_err(|source| ThemeError::BadColor {
                path: path.to_owned(),
                key: format!("palette.{slot}"),
                source,
            })?;
            PaletteFile::set(&mut palette, slot, color);
        }
        Ok(palette)
    }
}

fn patch(default: Style, overrides: &ElementFile, palette: &Palette, path: &str, key: &str) -> Result<Style, ThemeError> {
    let color = |spec: &ColorSpec, field: &str| -> Result<_, ThemeError> {
        spec.resolve_against(palette).map_err(|source| ThemeError::BadColor {
            path: path.to_owned(),
            key: format!("{key}.{field}"),
            source,
        })
    };

    let mut style = default;
    if let Some(spec) = &overrides.fg {
        style = style.fg(color(spec, "fg")?);
    }
    if let Some(spec) = &overrides.bg {
        style = match spec.removes_color() {
            true => Style { bg: None, ..style },
            false => style.bg(color(spec, "bg")?),
        };
    }
    if let Some(names) = &overrides.modifiers {
        let mut modifiers = Modifier::empty();
        for name in names {
            let Some(modifier) = elements::modifier(name) else {
                return Err(ThemeError::BadModifier {
                    path: path.to_owned(),
                    key: format!("{key}.modifiers"),
                    value: name.clone(),
                });
            };
            modifiers |= modifier;
        }
        style.add_modifier = modifiers;
        style.sub_modifier = Modifier::empty();
    }
    Ok(style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn theme(source: &str) -> Theme {
        from_source("test.toml", source).expect("the theme loads").0
    }

    fn built_in(name: &str) -> Theme {
        let source = BUILT_IN.iter().find(|(built_in, _)| *built_in == name).expect("a built-in by that name").1;
        theme(source)
    }

    fn warnings(source: &str) -> Vec<String> {
        from_source("test.toml", source).expect("the theme loads").1
    }

    fn write(dir: &Path, relative: &str, contents: &str) -> PathBuf {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, contents).expect("write");
        path
    }

    #[test]
    fn every_built_in_resolves_every_element() {
        for (name, source) in BUILT_IN {
            let (theme, warnings) = from_source(name, source).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(theme.name, name);
            assert!(theme.syntax_theme.is_some(), "{name} names no syntax theme");
            assert!(warnings.is_empty(), "{name} warned: {warnings:?}");
            for element in Element::ALL {
                let _ = theme.style(element);
            }
        }
    }

    #[test]
    fn the_embedded_ansi_theme_is_the_built_in_default() {
        let embedded = theme(BUILT_IN[0].1);
        let default = Theme::default();
        assert_eq!(embedded.palette, default.palette);
        for element in Element::ALL {
            assert_eq!(embedded.style(element), default.style(element), "{element:?} differs from the default");
        }
    }

    #[test]
    fn a_partial_theme_inherits_the_readable_defaults() {
        let theme = theme("name = \"mine\"\n[palette]\naccent = \"green\"\n");
        for element in [Element::InlineCode, Element::CodeBlock, Element::CodeBlockLang] {
            assert_eq!(theme.style(element).bg, None, "{element:?} inherited a background");
        }
        for element in [Element::SearchMatch, Element::SearchCurrent] {
            assert_eq!(theme.style(element).fg, Some(Color::Black), "{element:?}");
        }
    }

    #[test]
    fn a_theme_with_a_real_subtle_can_have_its_code_background() {
        for name in ["catppuccin-mocha", "catppuccin-latte", "kanagawa-dragon"] {
            let concrete = built_in(name);
            for element in [Element::InlineCode, Element::CodeBlock, Element::CodeBlockLang] {
                assert_eq!(concrete.style(element).bg, Some(concrete.palette.subtle), "{name} {element:?}");
            }
        }
    }

    #[test]
    fn the_ansi_theme_gives_code_no_background() {
        let ansi = built_in("ansi");
        for element in [Element::InlineCode, Element::CodeBlock, Element::CodeBlockLang] {
            assert_eq!(ansi.style(element).bg, None, "{element:?} still paints a background");
            assert!(ansi.style(element).fg.is_some(), "{element:?} has to say something, having no background");
        }

        assert_eq!(ansi.style(Element::CursorLine).bg, Some(ansi.palette.cursor));
        assert_ne!(ansi.style(Element::CursorLine).bg, ansi.style(Element::CodeBlock).bg);
    }

    #[test]
    fn a_theme_that_bands_its_code_still_shows_the_cursor_line_over_it() {
        for name in ["catppuccin-mocha", "catppuccin-latte", "kanagawa-dragon"] {
            let concrete = built_in(name);
            assert_ne!(concrete.palette.cursor, concrete.palette.subtle, "{name} cursor is the code band");
            assert_ne!(
                concrete.style(Element::CursorLine).bg,
                concrete.style(Element::CodeBlock).bg,
                "{name} loses the cursor line inside a code block"
            );
        }

        let ansi = built_in("ansi");
        assert_eq!(ansi.palette.cursor, ansi.palette.subtle, "ansi has 16 colors and one grey to spend");
    }

    #[test]
    fn the_ansi_search_highlights_name_a_foreground_that_reads() {
        let ansi = built_in("ansi");
        for element in [Element::SearchMatch, Element::SearchCurrent] {
            assert_eq!(ansi.style(element).fg, Some(Color::Black), "{element:?}");
            assert_ne!(ansi.style(element).fg, Some(ansi.palette.background), "{element:?} inverts against nothing");
            assert!(ansi.style(element).bg.is_some(), "{element:?} still needs something to invert against");
        }
        assert_ne!(ansi.style(Element::SearchMatch).bg, ansi.style(Element::SearchCurrent).bg);
        assert!(ansi.style(Element::SearchCurrent).add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn a_background_can_be_removed_but_a_foreground_cannot() {
        let palette = Palette::default();
        assert!(ColorSpec::Name(String::from("none")).removes_color());
        assert!(!ColorSpec::Name(String::from("reset")).removes_color(), "reset is a color: the terminal's own");
        let error = ColorSpec::Name(String::from("none")).resolve_against(&palette).expect_err("none is not a color");
        assert!(error.to_string().contains("bg"), "{error}");
    }

    #[test]
    fn a_palette_only_file_moves_every_element_that_derives_from_it() {
        let theme = theme(
            r##"[palette]
accent = "#ff0000""##,
        );
        assert_eq!(theme.style(Element::Heading1).fg, Some(Color::Rgb(0xFF, 0, 0)));
        assert_eq!(theme.style(Element::ListBullet).fg, Some(Color::Rgb(0xFF, 0, 0)));
        assert_eq!(theme.style(Element::Link).fg, Some(Palette::default().highlight));
    }

    #[test]
    fn an_element_override_keeps_the_defaults_it_does_not_mention() {
        let theme = theme(
            r#"[elements.heading1]
fg = "error""#,
        );
        assert_eq!(theme.style(Element::Heading1).fg, Some(Palette::default().error));
        assert!(theme.style(Element::Heading1).add_modifier.contains(Modifier::BOLD), "bold was dropped");
    }

    #[test]
    fn an_empty_modifier_list_clears_them() {
        let theme = theme(
            r#"[elements.heading1]
modifiers = []"#,
        );
        assert_eq!(theme.style(Element::Heading1).add_modifier, Modifier::empty());
        assert_eq!(theme.style(Element::Heading1).fg, Some(Palette::default().accent), "the color went with them");
    }

    #[test]
    fn an_element_color_resolves_against_this_file_s_palette() {
        let theme = theme(
            r##"[palette]
accent = "#00ff00"

[elements.link]
fg = "accent""##,
        );
        assert_eq!(theme.style(Element::Link).fg, Some(Color::Rgb(0, 0xFF, 0)));
    }

    #[test]
    fn indices_and_literals_are_colors_too() {
        let theme = theme(
            r#"[elements.link]
fg = 208
bg = "light_blue"
modifiers = ["italic", "dim"]"#,
        );
        let style = theme.style(Element::Link);
        assert_eq!(style.fg, Some(Color::Indexed(208)));
        assert_eq!(style.bg, Some(Color::LightBlue));
        assert_eq!(style.add_modifier, Modifier::ITALIC | Modifier::DIM);
    }

    #[test]
    fn unknown_keys_warn_and_the_theme_still_loads() {
        let warnings = warnings(
            r#"nonesuch = 1

[palette]
acccent = "red"

[elements.headin1]
fg = "red"

[elements.link]
colour = "red""#,
        );
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("unknown key `nonesuch`")), "{warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("palette: unknown key `acccent`")), "{warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("unknown element `headin1`")), "{warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("elements.link: unknown key `colour`")), "{warnings:?}");
    }

    #[test]
    fn an_unreadable_value_is_an_error() {
        assert!(from_source("t", "[palette]\naccent = \"blurple\"").is_err());
        assert!(from_source("t", "[palette]\naccent = \"accent\"").is_err(), "a slot cannot name a slot");
        assert!(from_source("t", "[elements.link]\nfg = \"blurple\"").is_err());
        assert!(from_source("t", "[elements.link]\nmodifiers = [\"blinky\"]").is_err());
        assert!(from_source("t", "this is not toml").is_err());
    }

    #[test]
    fn the_error_names_the_file_and_the_key() {
        let error = from_source("mine.toml", "[elements.link]\nfg = \"blurple\"").expect_err("blurple is not a color");
        let message = error.to_string();
        assert!(message.contains("mine.toml"), "{message}");
        assert!(message.contains("elements.link.fg"), "{message}");
    }

    #[test]
    fn an_error_states_its_cause_once() {
        let error = from_source("mine.toml", "[elements.link]\nfg = \"blurple\"").expect_err("blurple is not a color");
        let source = std::error::Error::source(&error).expect("the color error is the cause").to_string();
        assert!(!error.to_string().contains(&source), "{error} already contains {source}");
    }

    #[test]
    fn a_theme_name_cannot_step_outside_the_themes_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "outside.toml", "[palette]\naccent = \"#000005\"");
        write(dir.path(), "themes/inside.toml", "[palette]\naccent = \"#000006\"");

        for name in ["../outside", "..\\outside", "..", ".", "", "/etc/passwd"] {
            let error = resolve(None, Some(name), Some(dir.path()));
            assert!(error.is_err(), "{name:?} reached a file a theme name should not address");
        }

        let (theme, _) = resolve(Some(&dir.path().join("outside.toml")), None, Some(dir.path())).expect("the theme loads");
        assert_eq!(theme.palette.accent, Color::Rgb(0, 0, 5));
    }

    #[test]
    fn config_wins_over_theme_which_wins_over_the_config_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write(root, "theme.toml", "[palette]\naccent = \"#000001\"");
        write(root, "themes/mine.toml", "[palette]\naccent = \"#000002\"");
        let explicit = write(root, "explicit.toml", "[palette]\naccent = \"#000003\"");

        let accent = |config: Option<&Path>, name: Option<&str>| {
            resolve(config, name, Some(root)).expect("the theme loads").0.palette.accent
        };

        assert_eq!(accent(Some(&explicit), Some("mine")), Color::Rgb(0, 0, 3), "--config must win");
        assert_eq!(accent(None, Some("mine")), Color::Rgb(0, 0, 2), "--theme must win over theme.toml");
        assert_eq!(accent(None, None), Color::Rgb(0, 0, 1), "theme.toml must win over the built-in");
    }

    #[test]
    fn with_nothing_configured_the_built_in_default_is_used() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (theme, _) = resolve(None, None, Some(dir.path())).expect("the theme loads");
        assert_eq!(theme.name, "ansi");
        assert_eq!(theme.palette, Palette::default());

        let (nowhere, _) = resolve(None, None, None).expect("the theme loads");
        assert_eq!(nowhere.palette, Palette::default());
    }

    #[test]
    fn a_user_theme_shadows_a_built_in_of_the_same_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/kanagawa-dragon.toml", "[palette]\naccent = \"#000004\"");
        let (theme, _) = resolve(None, Some("kanagawa-dragon"), Some(dir.path())).expect("the theme loads");
        assert_eq!(theme.palette.accent, Color::Rgb(0, 0, 4));
    }

    #[test]
    fn an_unknown_theme_name_is_an_error_that_names_the_alternatives() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/mine.toml", "");
        let error = resolve(None, Some("nonesuch"), Some(dir.path())).expect_err("there is no such theme");
        let message = error.to_string();
        assert!(message.contains("nonesuch"), "{message}");
        assert!(message.contains("kanagawa-dragon"), "{message}");
        assert!(message.contains("mine"), "{message}");
    }

    #[test]
    fn a_missing_config_file_is_an_error_rather_than_a_silent_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(resolve(Some(&dir.path().join("gone.toml")), None, Some(dir.path())).is_err());
    }

    #[test]
    fn listing_puts_the_built_ins_first_and_user_themes_in_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/zebra.toml", "");
        write(dir.path(), "themes/Apple.toml", "");
        write(dir.path(), "themes/ansi.toml", "");
        write(dir.path(), "themes/notes.md", "");
        std::fs::create_dir_all(dir.path().join("themes/folder.toml")).expect("mkdir");

        let names = available_in(Some(dir.path()));
        assert_eq!(names, ["ansi", "kanagawa-dragon", "catppuccin-mocha", "catppuccin-latte", "Apple", "zebra"]);
        assert_eq!(available_in(None), ["ansi", "kanagawa-dragon", "catppuccin-mocha", "catppuccin-latte"]);
    }
}
