//! Syntax highlighting for fenced code blocks.
//!
//! syntect owns two expensive things — the syntax set and the theme set — so
//! both are built once and leaked into a `OnceLock`. Highlighting itself is
//! expensive too: the `fancy-regex` engine is slower than Oniguruma, and a
//! block would otherwise be re-highlighted on every scroll and every resize.
//! So a block is highlighted once and cached, and what comes back is shared
//! rather than copied.
//!
//! What this module returns carries foreground colors and font styles only.
//! The background belongs to the vademecum theme (`code_block.bg`), which is
//! why the caller patches these styles over the block style rather than the
//! other way round: a `.tmTheme` must never repaint the block.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use ratatui::style::{Color, Modifier, Style};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SyntectStyle, Theme as SyntectTheme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::render::line::{self, StyledSpan};

/// What a theme naming an unknown `.tmTheme` falls back to.
const FALLBACK: &str = "base16-ocean.dark";

/// One code block, highlighted: the spans of each of its lines, in order.
/// Shared because the cache hands the same block out again and again.
pub type CodeLines = Arc<Vec<Vec<StyledSpan>>>;

/// Highlight a fenced block, from the cache when it has been seen before.
///
/// `lang` is the fence tag and `syntax_theme` the theme's `.tmTheme` name;
/// neither has to name anything real, and a block whose language cannot be
/// identified is still returned, as plain text.
pub fn highlight(lang: Option<&str>, text: &str, syntax_theme: Option<&str>) -> CodeLines {
    let syntaxes = syntax_set();
    let syntax = syntax_for(syntaxes, lang, text);
    let (name, theme) = theme_for(syntax_theme);

    let key = Key { syntax: syntax.name.clone(), theme: name.to_owned(), text: hash(text) };
    if let Some(hit) = cached(&key) {
        return hit;
    }

    // A block whose language could not be identified is left alone rather than
    // painted plain-text-black: with no tokens to tell apart, the theme's own
    // `code_block` foreground is the better answer, and it is the only one the
    // `ansi` built-in can give without asserting a color of its own.
    let lines = Arc::new(if syntax.name == plain_text().name { unpainted(text) } else { paint(syntax, theme, text) });
    store(key, Arc::clone(&lines));
    lines
}

/// The names `--list-syntax-themes` prints: bundled and user themes together,
/// alphabetically, which is the order syntect already keeps them in.
pub fn available() -> Vec<String> {
    theme_set().themes.keys().cloned().collect()
}

fn plain_text() -> &'static SyntaxReference {
    syntax_set().find_syntax_plain_text()
}

/// The fence tag, else what the first line says about itself (a shebang), else
/// plain text.
fn syntax_for<'a>(syntaxes: &'a SyntaxSet, lang: Option<&str>, text: &str) -> &'a SyntaxReference {
    lang.map(str::trim)
        .filter(|lang| !lang.is_empty())
        .and_then(|lang| syntaxes.find_syntax_by_token(lang))
        .or_else(|| text.lines().next().and_then(|first| syntaxes.find_syntax_by_first_line(first)))
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text())
}

/// The named theme, or the fallback with one warning. A `.tmTheme` that is not
/// installed is a theme file asking for something this machine does not have —
/// worth saying out loud, but not worth refusing to render over.
fn theme_for(name: Option<&str>) -> (&'static str, &'static SyntectTheme) {
    let themes = theme_set();

    if let Some(name) = name
        && let Some((name, theme)) = themes.themes.get_key_value(name)
    {
        return (name, theme);
    }
    if let Some(name) = name {
        warn_once(name);
    }

    let theme = themes.themes.get(FALLBACK).expect("base16-ocean.dark is one of syntect's bundled themes");
    (FALLBACK, theme)
}

fn warn_once(name: &str) {
    static WARNED: OnceLock<()> = OnceLock::new();
    WARNED.get_or_init(|| eprintln!("vademecum: no syntax theme named {name:?}; using {FALLBACK}"));
}

/// syntect line by syntect line. The set is loaded with newlines, so each line
/// is handed over with its own, and the newline is dropped again on the way
/// out: a `RenderedLine` is a line already.
fn paint(syntax: &SyntaxReference, theme: &SyntectTheme, text: &str) -> Vec<Vec<StyledSpan>> {
    let syntaxes = syntax_set();
    let mut highlighter = HighlightLines::new(syntax, theme);

    let body = body(text);
    LinesWithEndings::from(&body)
        .map(|line| match highlighter.highlight_line(line, syntaxes) {
            Ok(regions) => line::merge(regions.iter().filter_map(|(style, piece)| span(*style, piece))),
            // A syntax that trips the regex engine loses its colors, not its
            // text: the block still has to render.
            Err(_) => plain(line).into_iter().collect(),
        })
        .collect()
}

/// Every line as one span of its own. What an unidentified language gets.
fn unpainted(text: &str) -> Vec<Vec<StyledSpan>> {
    LinesWithEndings::from(&body(text)).map(|line| plain(line).into_iter().collect()).collect()
}

/// The block's text with exactly one trailing newline, and none at all when
/// there is nothing to highlight. The syntaxes are loaded with newlines, so a
/// last line handed over without one can end in the wrong context — a line
/// comment that never closes, say — and take a color the same line would not
/// take in the middle of the block.
fn body(text: &str) -> String {
    let trimmed = text.trim_end_matches('\n');
    if trimmed.is_empty() { String::new() } else { format!("{trimmed}\n") }
}

/// A piece of a line with no color of its own, so the block style shows
/// through unchanged.
fn plain(line: &str) -> Option<StyledSpan> {
    let line = line.trim_end_matches(['\n', '\r']);
    (!line.is_empty()).then(|| StyledSpan::new(line, Style::default()))
}

fn span(style: SyntectStyle, piece: &str) -> Option<StyledSpan> {
    let piece = piece.trim_end_matches(['\n', '\r']);
    (!piece.is_empty()).then(|| StyledSpan::new(piece, convert(style)))
}

/// syntect `Style` → ours: the foreground and the three font styles a terminal
/// can show. The background is deliberately dropped.
fn convert(style: SyntectStyle) -> Style {
    let foreground = style.foreground;
    let mut converted = Style::default().fg(Color::Rgb(foreground.r, foreground.g, foreground.b));

    for (font_style, modifier) in
        [(FontStyle::BOLD, Modifier::BOLD), (FontStyle::ITALIC, Modifier::ITALIC), (FontStyle::UNDERLINE, Modifier::UNDERLINED)]
    {
        if style.font_style.contains(font_style) {
            converted = converted.add_modifier(modifier);
        }
    }
    converted
}

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// The bundled themes plus whatever `.tmTheme` files the reader has put in
/// `<config>/syntax-themes/`. No such directory is not a fault, and neither is
/// a file in it that syntect cannot read: it is skipped and the rest still
/// load. `ThemeSet::add_from_folder` cannot do that — it stops at the first
/// unreadable file and drops every theme after it — so the folder is walked
/// here instead.
fn theme_set() -> &'static ThemeSet {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    THEMES.get_or_init(|| {
        let mut themes = ThemeSet::load_defaults();
        for path in user_themes() {
            let Ok(theme) = ThemeSet::get_theme(&path) else { continue };
            let Some(name) = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()) else { continue };
            themes.themes.insert(name, theme);
        }
        themes
    })
}

/// Every `.tmTheme` in `<config>/syntax-themes/`, in the order syntect
/// discovers them.
fn user_themes() -> Vec<PathBuf> {
    crate::config::config_dir()
        .map(|dir| dir.join("syntax-themes"))
        .and_then(|dir| ThemeSet::discover_theme_paths(dir).ok())
        .unwrap_or_default()
}

/// A block is identified by the syntax that read it, the theme that colored
/// it, and its text — the three things its spans depend on.
#[derive(Debug, PartialEq, Eq, Hash)]
struct Key {
    syntax: String,
    theme: String,
    text: u64,
}

fn cache() -> &'static Mutex<HashMap<Key, CodeLines>> {
    static CACHE: OnceLock<Mutex<HashMap<Key, CodeLines>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// A poisoned cache is a cache, not a reason to stop rendering: whatever
/// panicked left highlighted spans behind, and they are still correct.
fn cached(key: &Key) -> Option<CodeLines> {
    cache().lock().unwrap_or_else(PoisonError::into_inner).get(key).map(Arc::clone)
}

fn store(key: Key, lines: CodeLines) {
    cache().lock().unwrap_or_else(PoisonError::into_inner).insert(key, lines);
}

fn hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn foregrounds(lines: &[Vec<StyledSpan>]) -> Vec<Color> {
        lines.iter().flatten().filter_map(|span| span.style.fg).collect()
    }

    fn distinct(colors: &[Color]) -> usize {
        let mut seen: Vec<Color> = Vec::new();
        for color in colors {
            if !seen.contains(color) {
                seen.push(*color);
            }
        }
        seen.len()
    }

    #[test]
    fn a_rust_fence_gets_more_than_one_color() {
        let lines = highlight(Some("rust"), "fn main() {\n    println!(\"hi\");\n}\n", Some("base16-ocean.dark"));
        assert_eq!(lines.len(), 3, "one entry per code line: {lines:?}");
        assert!(distinct(&foregrounds(&lines)) > 1, "the keyword and the string are the same color: {lines:?}");
    }

    #[test]
    fn an_unknown_language_gets_one_style() {
        let lines = highlight(Some("notalanguage"), "fn main() {}\nlet x = 1;\n", Some("base16-ocean.dark"));
        assert!(foregrounds(&lines).is_empty(), "an unhighlighted block took a color from the .tmTheme: {lines:?}");
        assert!(lines.iter().all(|line| line.len() == 1), "an unhighlighted line was split up: {lines:?}");
    }

    #[test]
    fn neighbouring_tokens_of_one_style_become_one_span() {
        let lines = highlight(Some("rust"), "let value = 1;\n", Some("base16-ocean.dark"));
        let styles: Vec<Style> = lines[0].iter().map(|span| span.style).collect();
        assert!(styles.windows(2).all(|pair| pair[0] != pair[1]), "adjacent spans share a style: {lines:?}");
    }

    #[test]
    fn a_shebang_names_the_language_when_the_fence_does_not() {
        let syntaxes = syntax_set();
        let syntax = syntax_for(syntaxes, None, "#!/usr/bin/env python3\nprint(1)\n");
        assert_eq!(syntax.name, "Python");
    }

    #[test]
    fn an_untagged_block_without_a_shebang_is_plain_text() {
        let syntaxes = syntax_set();
        assert_eq!(syntax_for(syntaxes, None, "just some prose\n").name, "Plain Text");
        assert_eq!(syntax_for(syntaxes, Some(""), "just some prose\n").name, "Plain Text");
    }

    #[test]
    fn the_same_block_twice_is_the_same_allocation() {
        let text = "fn cached() -> bool { true }\n";
        let first = highlight(Some("rust"), text, Some("base16-ocean.dark"));
        let second = highlight(Some("rust"), text, Some("base16-ocean.dark"));
        assert!(Arc::ptr_eq(&first, &second), "the block was highlighted twice");
    }

    #[test]
    fn the_syntax_theme_is_part_of_the_key() {
        let text = "fn themed() {}\n";
        let dark = highlight(Some("rust"), text, Some("base16-ocean.dark"));
        let light = highlight(Some("rust"), text, Some("base16-ocean.light"));
        assert!(!Arc::ptr_eq(&dark, &light));
        assert_ne!(foregrounds(&dark), foregrounds(&light), "two themes painted the same colors");
    }

    #[test]
    fn an_unknown_theme_falls_back_rather_than_failing() {
        let (name, _) = theme_for(Some("no-such-theme"));
        assert_eq!(name, FALLBACK);
        let (name, _) = theme_for(None);
        assert_eq!(name, FALLBACK);
    }

    #[test]
    fn blank_lines_keep_their_place() {
        let lines = highlight(Some("rust"), "let a = 1;\n\nlet b = 2;\n", Some("base16-ocean.dark"));
        assert_eq!(lines.len(), 3);
        assert!(lines[1].is_empty(), "the blank line carries spans: {:?}", lines[1]);
    }

    #[test]
    fn the_bundled_themes_are_listed() {
        let names = available();
        assert!(names.contains(&String::from(FALLBACK)), "{names:?}");
        assert!(names.contains(&String::from("InspiredGitHub")), "{names:?}");
    }
}
