//! Finding a theme, reading it, and merging it over the defaults.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ratatui::style::{Modifier, Style};
use serde::Deserialize;

use crate::theme::Theme;
use crate::theme::color::{ColorError, ColorSpec};
use crate::theme::elements::{self, Element, ElementFile};
use crate::theme::palette::{Palette, PaletteFile};

const DEFAULT: &str = "handbook";

const BUILT_IN: [(&str, &str); 5] = [
    ("handbook", include_str!("../../themes/handbook.toml")),
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
    #[error("{path}: base {name:?} loops back on itself: {chain}")]
    Cycle { path: String, name: String, chain: String },
}

#[derive(Debug, Default, Deserialize)]
struct ThemeFile {
    base: Option<String>,
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
        return from_file(path, config_dir);
    }
    if let Some(name) = name {
        return by_name(name, config_dir);
    }
    if let Some(path) = config_dir.map(|dir| dir.join("theme.toml")).filter(|path| path.is_file()) {
        return from_file(&path, config_dir);
    }
    by_name(DEFAULT, config_dir)
}

fn by_name(name: &str, config_dir: Option<&Path>) -> Result<(Theme, Vec<String>), ThemeError> {
    let (label, source) = source_of(name, config_dir)?;
    theme_from(&label, &source, config_dir, &mut vec![name.to_owned()])
}

fn source_of(name: &str, config_dir: Option<&Path>) -> Result<(String, String), ThemeError> {
    let user: Option<PathBuf> = config_dir
        .filter(|_| addresses_a_theme(name))
        .map(|dir| dir.join("themes").join(format!("{name}.toml")))
        .filter(|path| path.is_file());
    if let Some(path) = user {
        let label = path.display().to_string();
        let source = std::fs::read_to_string(&path).map_err(|source| ThemeError::Unreadable { path: label.clone(), source })?;
        return Ok((label, source));
    }
    match BUILT_IN.iter().find(|(built_in, _)| *built_in == name) {
        Some((built_in, source)) => Ok(((*built_in).to_owned(), (*source).to_owned())),
        None => Err(ThemeError::Unknown { name: name.to_owned(), available: available_in(config_dir).join(", ") }),
    }
}

fn addresses_a_theme(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

fn from_file(path: &Path, config_dir: Option<&Path>) -> Result<(Theme, Vec<String>), ThemeError> {
    let label = path.display().to_string();
    let source = std::fs::read_to_string(path).map_err(|source| ThemeError::Unreadable { path: label.clone(), source })?;
    from_source(&label, &source, config_dir)
}

fn from_source(label: &str, source: &str, config_dir: Option<&Path>) -> Result<(Theme, Vec<String>), ThemeError> {
    theme_from(label, source, config_dir, &mut Vec::new())
}

fn theme_from(
    label: &str,
    source: &str,
    config_dir: Option<&Path>,
    chain: &mut Vec<String>,
) -> Result<(Theme, Vec<String>), ThemeError> {
    let (file, warnings) = compose(label, source, config_dir, chain)?;
    let (theme, _) = file.build(label)?;
    Ok((theme, warnings))
}

fn compose(
    label: &str,
    source: &str,
    config_dir: Option<&Path>,
    chain: &mut Vec<String>,
) -> Result<(ThemeFile, Vec<String>), ThemeError> {
    let mut file: ThemeFile =
        toml::from_str(source).map_err(|source| ThemeError::Malformed { path: label.to_owned(), source })?;
    let mut warnings = file.check(label)?;

    let Some(name) = file.base.take() else { return Ok((file, warnings)) };

    if chain.contains(&name) {
        chain.push(name.clone());
        return Err(ThemeError::Cycle { path: label.to_owned(), name, chain: chain.join(" -> ") });
    }

    let (base_label, base_source) = source_of(&name, config_dir)?;
    chain.push(name);
    let (base, inherited) = compose(&base_label, &base_source, config_dir, chain)?;
    warnings.extend(inherited);
    Ok((file.merge(base), warnings))
}

impl ThemeFile {
    fn merge(self, base: Self) -> Self {
        let mut elements = base.elements;
        for (key, overrides) in self.elements {
            let merged = match elements.remove(&key) {
                Some(inherited) => overrides.merge(inherited),
                None => overrides,
            };
            elements.insert(key, merged);
        }

        let mut unknown = base.unknown;
        unknown.extend(self.unknown);

        Self {
            base: None,
            name: self.name.or(base.name),
            syntax_theme: self.syntax_theme.or(base.syntax_theme),
            palette: self.palette.merge(base.palette),
            elements,
            unknown,
        }
    }

    fn check(&self, path: &str) -> Result<Vec<String>, ThemeError> {
        Ok(self.build(path)?.1)
    }

    fn build(&self, path: &str) -> Result<(Theme, Vec<String>), ThemeError> {
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

        let name = self.name.clone().unwrap_or_else(|| path.to_owned());
        Ok((Theme { palette, styles, name, syntax_theme: self.syntax_theme.clone() }, warnings))
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
        if self.palette.cursor.is_none() && self.palette.subtle.is_some() {
            palette.cursor = palette.subtle;
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
        from_source("test.toml", source, None).expect("the theme loads").0
    }

    fn built_in(name: &str) -> Theme {
        let source = BUILT_IN.iter().find(|(built_in, _)| *built_in == name).expect("a built-in by that name").1;
        theme(source)
    }

    fn concrete_built_ins() -> Vec<(&'static str, Theme)> {
        BUILT_IN.iter().filter(|(name, _)| *name != "ansi").map(|(name, source)| (*name, theme(source))).collect()
    }

    fn derived(dir: &Path, source: &str) -> Theme {
        from_source("child.toml", source, Some(dir)).expect("the theme loads").0
    }

    fn derived_warnings(dir: &Path, source: &str) -> Vec<String> {
        from_source("child.toml", source, Some(dir)).expect("the theme loads").1
    }

    fn warnings(source: &str) -> Vec<String> {
        from_source("test.toml", source, None).expect("the theme loads").1
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
            let (theme, warnings) = from_source(name, source, None).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(theme.name, name);
            assert!(theme.syntax_theme.is_some(), "{name} names no syntax theme");
            let file: ThemeFile = toml::from_str(source).expect("a built-in parses");
            assert!(file.base.is_none(), "{name} names a base, so the built-ins are no longer a flat set");
            assert!(warnings.is_empty(), "{name} warned: {warnings:?}");
            for element in Element::ALL {
                let _ = theme.style(element);
            }
        }
    }

    #[test]
    fn the_embedded_ansi_theme_is_the_merge_base_every_file_that_names_no_base_inherits() {
        let embedded = built_in("ansi");
        let default = Theme::default();
        assert_eq!(embedded.palette, default.palette);
        for element in Element::ALL {
            assert_eq!(embedded.style(element), default.style(element), "{element:?} differs from the merge base");
        }
    }

    #[test]
    fn a_file_that_names_a_base_starts_from_that_theme_rather_than_the_ansi_defaults() {
        let derived = theme("base = \"catppuccin-mocha\"\n");
        let mocha = built_in("catppuccin-mocha");

        assert_eq!(derived.palette, mocha.palette);
        for element in Element::ALL {
            assert_eq!(derived.style(element), mocha.style(element), "{element:?} did not come from the base");
        }
    }

    #[test]
    fn an_override_on_top_of_a_base_wins_in_that_slot_alone() {
        let derived = theme("base = \"catppuccin-mocha\"\n[palette]\naccent = \"#ff0000\"\n");
        let mocha = built_in("catppuccin-mocha");

        assert_eq!(derived.palette.accent, Color::Rgb(255, 0, 0));
        assert_eq!(derived.style(Element::Heading1).fg, Some(Color::Rgb(255, 0, 0)), "headings derive from the accent");
        assert_eq!(derived.palette.highlight, mocha.palette.highlight, "an unnamed slot moved");
        assert_eq!(derived.style(Element::Link), mocha.style(Element::Link));
    }

    #[test]
    fn a_child_that_moves_subtle_re_derives_the_elements_that_derive_from_it() {
        let derived = theme("base = \"catppuccin-mocha\"\n[palette]\nsubtle = \"#010203\"\n");

        assert_eq!(derived.style(Element::CodeBlock).bg, Some(Color::Rgb(1, 2, 3)), "the base's band did not follow");
        assert_eq!(derived.style(Element::InlineCode).bg, Some(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn a_base_that_bands_its_code_keeps_the_band_when_the_child_only_changes_a_colour() {
        let derived = theme("base = \"catppuccin-mocha\"\n[palette]\naccent = \"#ff0000\"\n");
        let mocha = built_in("catppuccin-mocha");

        for element in [Element::InlineCode, Element::CodeBlock, Element::CodeBlockLang] {
            assert_eq!(derived.style(element).bg, mocha.style(element).bg, "{element:?} lost its band");
        }
    }

    #[test]
    fn syntax_theme_and_name_come_from_the_base_until_the_child_names_them() {
        let inherited = theme("base = \"catppuccin-mocha\"\n");
        assert_eq!(inherited.name, "catppuccin-mocha");
        assert_eq!(inherited.syntax_theme.as_deref(), Some("base16-mocha.dark"));

        let named = theme("base = \"catppuccin-mocha\"\nname = \"mine\"\nsyntax_theme = \"InspiredGitHub\"\n");
        assert_eq!(named.name, "mine");
        assert_eq!(named.syntax_theme.as_deref(), Some("InspiredGitHub"));
    }

    #[test]
    fn the_base_key_is_not_an_unknown_key() {
        assert!(warnings("base = \"ansi\"\n").is_empty(), "the key that names a base was reported as unknown");
    }

    #[test]
    fn a_child_that_names_subtle_leaves_the_bases_cursor_alone() {
        let derived = theme("base = \"catppuccin-mocha\"\n[palette]\nsubtle = \"#010203\"\n");
        let mocha = built_in("catppuccin-mocha");

        assert_eq!(derived.palette.cursor, mocha.palette.cursor, "the base named a cursor deliberately");
        assert_ne!(derived.style(Element::CursorLine).bg, derived.style(Element::CodeBlock).bg);
    }

    #[test]
    fn the_cursor_still_follows_subtle_when_no_file_in_the_chain_names_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/banded.toml", "[palette]\nsubtle = \"#010203\"\n");

        let theme = derived(dir.path(), "base = \"banded\"\n");
        assert_eq!(theme.palette.cursor, Color::Rgb(1, 2, 3), "no file named a cursor, so it should follow the band");
        assert_eq!(theme.style(Element::CursorLine).bg, Some(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn a_field_the_child_does_not_mention_keeps_the_bases() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/mine.toml", "[elements.heading1]\nfg = \"red\"\nmodifiers = [\"italic\"]\n");

        let theme = derived(dir.path(), "base = \"mine\"\n[elements.heading1]\nbg = \"blue\"\n");
        let heading = theme.style(Element::Heading1);
        assert_eq!(heading.fg, Some(Color::Red), "the base's foreground was dropped");
        assert_eq!(heading.bg, Some(Color::Blue));
        assert!(heading.add_modifier.contains(Modifier::ITALIC), "the base's modifiers were dropped");
        assert!(!heading.add_modifier.contains(Modifier::BOLD), "the base replaced the default modifiers");
    }

    #[test]
    fn a_none_background_in_the_child_removes_the_bases_band() {
        let theme = derived(
            tempfile::tempdir().expect("tempdir").path(),
            "base = \"catppuccin-mocha\"\n[elements.code_block]\nbg = \"none\"\n",
        );
        assert_eq!(theme.style(Element::CodeBlock).bg, None, "a child cannot un-band code it inherited");
    }

    #[test]
    fn a_none_background_in_the_base_survives_a_child_that_only_names_a_foreground() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/bare.toml", "[palette]\nsubtle = \"#010203\"\n[elements.code_block]\nbg = \"none\"\n");

        let theme = derived(dir.path(), "base = \"bare\"\n[elements.code_block]\nfg = \"red\"\n");
        assert_eq!(theme.style(Element::CodeBlock).bg, None, "the base removed the band and the child did not ask for it back");
        assert_eq!(theme.style(Element::CodeBlock).fg, Some(Color::Red));
    }

    #[test]
    fn an_empty_modifier_list_in_the_base_still_clears_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/plain.toml", "[elements.heading1]\nmodifiers = []\n");

        let theme = derived(dir.path(), "base = \"plain\"\n[elements.heading1]\nfg = \"red\"\n");
        assert_eq!(theme.style(Element::Heading1).add_modifier, Modifier::empty(), "the base cleared bold and the child kept fg");
    }

    #[test]
    fn a_chain_three_files_deep_merges_from_the_bottom_up() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/a.toml", "[palette]\naccent = \"#0000a1\"\nerror = \"#0000a2\"\n");
        write(dir.path(), "themes/b.toml", "base = \"a\"\n[palette]\nerror = \"#0000b2\"\nsuccess = \"#0000b3\"\n");

        let theme = derived(dir.path(), "base = \"b\"\n[palette]\nsuccess = \"#0000c3\"\n");
        assert_eq!(theme.palette.accent, Color::Rgb(0, 0, 0xa1), "the furthest file was lost");
        assert_eq!(theme.palette.error, Color::Rgb(0, 0, 0xb2), "the nearer file should win");
        assert_eq!(theme.palette.success, Color::Rgb(0, 0, 0xc3), "the child should win");
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
        for (name, concrete) in concrete_built_ins() {
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
    fn a_file_that_names_subtle_and_not_cursor_keeps_the_cursor_on_the_band() {
        let inherited = theme("[palette]\nsubtle = \"#101010\"\n");
        assert_eq!(inherited.palette.cursor, Color::Rgb(16, 16, 16), "a theme written before the two slots split moved");
        assert_eq!(inherited.style(Element::CursorLine).bg, Some(Color::Rgb(16, 16, 16)));

        let separate = theme("[palette]\nsubtle = \"#101010\"\ncursor = \"#202020\"\n");
        assert_eq!(separate.palette.cursor, Color::Rgb(32, 32, 32), "a theme that names both must keep them apart");

        let neither = theme("name = \"mine\"\n");
        assert_eq!(neither.palette.cursor, Palette::default().cursor);
    }

    #[test]
    fn a_theme_that_bands_its_code_still_shows_the_cursor_line_over_it() {
        for (name, concrete) in concrete_built_ins() {
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
        assert!(from_source("t", "[palette]\naccent = \"blurple\"", None).is_err());
        assert!(from_source("t", "[palette]\naccent = \"accent\"", None).is_err(), "a slot cannot name a slot");
        assert!(from_source("t", "[elements.link]\nfg = \"blurple\"", None).is_err());
        assert!(from_source("t", "[elements.link]\nmodifiers = [\"blinky\"]", None).is_err());
        assert!(from_source("t", "this is not toml", None).is_err());
    }

    #[test]
    fn the_error_names_the_file_and_the_key() {
        let error = from_source("mine.toml", "[elements.link]\nfg = \"blurple\"", None).expect_err("blurple is not a color");
        let message = error.to_string();
        assert!(message.contains("mine.toml"), "{message}");
        assert!(message.contains("elements.link.fg"), "{message}");
    }

    #[test]
    fn an_error_states_its_cause_once() {
        let error = from_source("mine.toml", "[elements.link]\nfg = \"blurple\"", None).expect_err("blurple is not a color");
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
        assert_eq!(theme.name, DEFAULT);
        assert_eq!(theme.palette, built_in(DEFAULT).palette);

        let (nowhere, _) = resolve(None, None, None).expect("the theme loads");
        assert_eq!(nowhere.palette, built_in(DEFAULT).palette);
        assert_ne!(nowhere.palette, Palette::default(), "the default theme is no longer the merge base");
    }

    #[test]
    fn a_user_file_named_after_the_default_shadows_it_like_any_other() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), &format!("themes/{DEFAULT}.toml"), "[palette]\naccent = \"#000007\"");
        let (theme, _) = resolve(None, None, Some(dir.path())).expect("the theme loads");
        assert_eq!(theme.palette.accent, Color::Rgb(0, 0, 7));
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
    fn a_base_is_resolved_through_the_user_directory_before_the_built_in() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/catppuccin-mocha.toml", "[palette]\naccent = \"#000004\"\n");

        let theme = derived(dir.path(), "base = \"catppuccin-mocha\"\n");
        assert_eq!(theme.palette.accent, Color::Rgb(0, 0, 4), "the built-in won over the reader's own file");
    }

    #[test]
    fn an_unknown_base_is_an_error_that_names_the_alternatives() {
        let error = from_source("child.toml", "base = \"nonesuch\"\n", None).expect_err("there is no such theme");
        let message = error.to_string();
        assert!(message.contains("nonesuch"), "{message}");
        assert!(message.contains("catppuccin-mocha"), "{message}");
    }

    #[test]
    fn a_base_name_cannot_step_outside_the_themes_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "outside.toml", "[palette]\naccent = \"#000005\"\n");

        for name in ["../outside", "..\\outside", "..", ".", "", "/etc/passwd"] {
            let source = format!("base = {name:?}\n");
            let error = from_source("child.toml", &source, Some(dir.path()));
            assert!(error.is_err(), "{name:?} reached a file a base should not address");
        }
    }

    #[test]
    fn a_config_file_reached_by_path_may_name_a_base() {
        let dir = tempfile::tempdir().expect("tempdir");
        let explicit = write(dir.path(), "explicit.toml", "base = \"catppuccin-latte\"\n");

        let (theme, _) = resolve(Some(&explicit), None, Some(dir.path())).expect("the theme loads");
        assert_eq!(theme.palette, built_in("catppuccin-latte").palette);
    }

    #[test]
    fn a_file_that_names_itself_as_its_base_is_an_error_rather_than_a_hang() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/mine.toml", "base = \"mine\"\n");

        let error = resolve(None, Some("mine"), Some(dir.path())).expect_err("a self-reference cannot resolve");
        assert!(matches!(error, ThemeError::Cycle { .. }), "{error}");
    }

    #[test]
    fn two_files_that_name_each_other_are_an_error_rather_than_a_hang() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/one.toml", "base = \"two\"\n");
        write(dir.path(), "themes/two.toml", "base = \"one\"\n");

        let error = resolve(None, Some("one"), Some(dir.path())).expect_err("a loop cannot resolve");
        assert!(matches!(error, ThemeError::Cycle { .. }), "{error}");
    }

    #[test]
    fn a_user_file_named_after_a_built_in_cannot_name_that_built_in_as_its_base() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/handbook.toml", "base = \"handbook\"\n");

        let error = resolve(None, None, Some(dir.path())).expect_err("shadowing makes this a loop");
        assert!(matches!(error, ThemeError::Cycle { .. }), "{error}");
    }

    #[test]
    fn the_cycle_error_names_the_chain_it_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/one.toml", "base = \"two\"\n");
        write(dir.path(), "themes/two.toml", "base = \"one\"\n");

        let error = resolve(None, Some("one"), Some(dir.path())).expect_err("a loop cannot resolve");
        let message = error.to_string();
        assert!(message.contains("one -> two -> one"), "the chain is not in the message: {message}");
        assert!(message.contains("two.toml"), "the file whose base closed the loop is not named: {message}");
    }

    #[test]
    fn a_warning_in_the_base_is_reported_once_and_names_the_base_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/noisy.toml", "colour = \"red\"\n");

        let warnings = derived_warnings(dir.path(), "base = \"noisy\"\n");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("noisy.toml"), "the child was blamed: {warnings:?}");
        assert!(!warnings[0].contains("child.toml"), "the child was blamed: {warnings:?}");
        assert!(warnings[0].contains("colour"), "{warnings:?}");
    }

    #[test]
    fn a_warning_in_each_file_of_a_chain_is_reported_for_that_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/noisy.toml", "colour = \"red\"\n");

        let warnings = derived_warnings(dir.path(), "base = \"noisy\"\nshade = \"blue\"\n");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("child.toml") && warnings[0].contains("shade"), "the child comes first: {warnings:?}");
        assert!(warnings[1].contains("noisy.toml") && warnings[1].contains("colour"), "{warnings:?}");
    }

    #[test]
    fn a_bad_color_in_the_base_is_an_error_even_when_the_child_overrides_that_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/broken.toml", "[palette]\naccent = \"blurple\"\n");

        let error = from_source("child.toml", "base = \"broken\"\n[palette]\naccent = \"red\"\n", Some(dir.path()));
        let error = error.expect_err("a broken base is a broken theme, overridden or not");
        assert!(error.to_string().contains("broken.toml"), "the child was blamed: {error}");
    }

    #[test]
    fn an_error_in_the_child_is_reported_before_an_error_in_the_base() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "themes/broken.toml", "[palette]\naccent = \"blurple\"\n");

        let error = from_source("child.toml", "base = \"broken\"\n[palette]\nerror = \"nonesuch\"\n", Some(dir.path()));
        let error = error.expect_err("both files are broken");
        assert!(error.to_string().contains("child.toml"), "the file the reader just edited should report first: {error}");
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
        assert_eq!(names, ["handbook", "ansi", "kanagawa-dragon", "catppuccin-mocha", "catppuccin-latte", "Apple", "zebra"]);
        assert_eq!(available_in(None), ["handbook", "ansi", "kanagawa-dragon", "catppuccin-mocha", "catppuccin-latte"]);
        assert_eq!(names[0], DEFAULT, "the listing leads with the default");
    }
}
