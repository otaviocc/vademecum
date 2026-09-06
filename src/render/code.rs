//! Syntax highlighting for fenced code blocks.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use ratatui::style::{Color, Modifier, Style};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SyntectStyle, Theme as SyntectTheme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::render::line::{self, StyledSpan};

const FALLBACK: &str = "base16-ocean.dark";

pub type CodeLines = Arc<Vec<Vec<StyledSpan>>>;

pub fn highlight(lang: Option<&str>, text: &str, syntax_theme: Option<&str>) -> CodeLines {
    let syntaxes = syntax_set();
    let syntax = syntax_for(syntaxes, lang, text);
    let (name, theme) = theme_for(syntax_theme);

    let key = Key { syntax: syntax.name.clone(), theme: name.to_owned(), text: hash(text) };
    if let Some(hit) = cached(&key) {
        return hit;
    }

    let lines = Arc::new(if syntax.name == plain_text().name { unpainted(text) } else { paint(syntax, theme, text) });
    store(key, Arc::clone(&lines));
    lines
}

pub fn available() -> Vec<String> {
    theme_set().themes.keys().cloned().collect()
}

fn plain_text() -> &'static SyntaxReference {
    syntax_set().find_syntax_plain_text()
}

fn syntax_for<'a>(syntaxes: &'a SyntaxSet, lang: Option<&str>, text: &str) -> &'a SyntaxReference {
    lang.map(str::trim)
        .filter(|lang| !lang.is_empty())
        .and_then(|lang| syntaxes.find_syntax_by_token(lang))
        .or_else(|| text.lines().next().and_then(|first| syntaxes.find_syntax_by_first_line(first)))
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text())
}

fn theme_for(name: Option<&str>) -> (&'static str, &'static SyntectTheme) {
    let themes = theme_set();

    if let Some(name) = name
        && let Some((name, theme)) = themes.themes.get_key_value(name)
    {
        return (name, theme);
    }
    if let Some(name) = name {
        warn(name);
    }

    let theme = themes.themes.get(FALLBACK).expect("base16-ocean.dark is one of syntect's bundled themes");
    (FALLBACK, theme)
}

fn warn(name: &str) {
    let mut said = said().lock().unwrap_or_else(PoisonError::into_inner);
    if !said.insert(name.to_owned()) {
        return;
    }
    warnings().lock().unwrap_or_else(PoisonError::into_inner).push(format!("no syntax theme named {name:?}; using {FALLBACK}"));
}

fn said() -> &'static Mutex<HashSet<String>> {
    static SAID: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SAID.get_or_init(Mutex::default)
}

fn warnings() -> &'static Mutex<Vec<String>> {
    static WARNINGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    WARNINGS.get_or_init(Mutex::default)
}

pub fn take_warnings() -> Vec<String> {
    std::mem::take(&mut *warnings().lock().unwrap_or_else(PoisonError::into_inner))
}

fn paint(syntax: &SyntaxReference, theme: &SyntectTheme, text: &str) -> Vec<Vec<StyledSpan>> {
    let syntaxes = syntax_set();
    let mut highlighter = HighlightLines::new(syntax, theme);

    let body = body(text);
    LinesWithEndings::from(&body)
        .map(|line| match highlighter.highlight_line(line, syntaxes) {
            Ok(regions) => line::merge(regions.iter().filter_map(|(style, piece)| span(*style, piece))),
            Err(_) => plain(line).into_iter().collect(),
        })
        .collect()
}

fn unpainted(text: &str) -> Vec<Vec<StyledSpan>> {
    LinesWithEndings::from(&body(text)).map(|line| plain(line).into_iter().collect()).collect()
}

fn body(text: &str) -> String {
    let trimmed = text.trim_end_matches('\n');
    if trimmed.is_empty() { String::new() } else { format!("{trimmed}\n") }
}

fn plain(line: &str) -> Option<StyledSpan> {
    let line = line.trim_end_matches(['\n', '\r']);
    (!line.is_empty()).then(|| StyledSpan::new(line, Style::default()))
}

fn span(style: SyntectStyle, piece: &str) -> Option<StyledSpan> {
    let piece = piece.trim_end_matches(['\n', '\r']);
    (!piece.is_empty()).then(|| StyledSpan::new(piece, convert(style)))
}

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
    SYNTAXES.get_or_init(|| {
        syntect::dumps::from_uncompressed_data(include_bytes!(concat!(env!("OUT_DIR"), "/syntaxes.pack")))
            .expect("the pack is written by this crate's own build script")
    })
}

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

fn user_themes() -> Vec<PathBuf> {
    crate::config::config_dir()
        .map(|dir| dir.join("syntax-themes"))
        .and_then(|dir| ThemeSet::discover_theme_paths(dir).ok())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    syntax: String,
    theme: String,
    text: u64,
}

const CAPACITY: usize = 256;

#[derive(Default)]
struct Cache {
    blocks: HashMap<Key, CodeLines>,
    order: VecDeque<Key>,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

fn cached(key: &Key) -> Option<CodeLines> {
    cache().lock().unwrap_or_else(PoisonError::into_inner).blocks.get(key).map(Arc::clone)
}

fn store(key: Key, lines: CodeLines) {
    let mut cache = cache().lock().unwrap_or_else(PoisonError::into_inner);
    if cache.blocks.insert(key.clone(), lines).is_none() {
        cache.order.push_back(key);
    }
    while cache.order.len() > CAPACITY {
        if let Some(oldest) = cache.order.pop_front() {
            cache.blocks.remove(&oldest);
        }
    }
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
        let _guard = exclusively();
        let text = "fn cached() -> bool { true }\n";
        let first = highlight(Some("rust"), text, Some("base16-ocean.dark"));
        let second = highlight(Some("rust"), text, Some("base16-ocean.dark"));
        assert!(Arc::ptr_eq(&first, &second), "the block was highlighted twice");
    }

    #[test]
    fn the_syntax_theme_is_part_of_the_key() {
        let _guard = exclusively();
        let text = "fn themed() {}\n";
        let dark = highlight(Some("rust"), text, Some("base16-ocean.dark"));
        let light = highlight(Some("rust"), text, Some("base16-ocean.light"));
        assert!(!Arc::ptr_eq(&dark, &light));
        assert_ne!(foregrounds(&dark), foregrounds(&light), "two themes painted the same colors");
    }

    #[test]
    fn an_unknown_theme_falls_back_rather_than_failing() {
        let _guard = exclusively();
        forget_what_was_said();
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

    fn exclusively() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(Mutex::default).lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn forget_what_was_said() {
        said().lock().unwrap_or_else(PoisonError::into_inner).clear();
        let _ = take_warnings();
    }

    #[test]
    fn the_cache_stops_growing_at_its_bound() {
        let _guard = exclusively();
        for n in 0..CAPACITY + 50 {
            highlight(Some("rust"), &format!("let x{n} = {n};\n"), None);
        }
        let cache = cache().lock().unwrap_or_else(PoisonError::into_inner);
        assert!(cache.blocks.len() <= CAPACITY, "{} blocks kept", cache.blocks.len());
        assert_eq!(cache.blocks.len(), cache.order.len(), "the map and the queue drifted apart");
    }

    #[test]
    fn nothing_of_one_document_is_evicted_while_it_is_being_read() {
        let _guard = exclusively();
        let document: Vec<String> = (0..24).map(|n| format!("fn doc{n}() {{}}\n")).collect();
        let first: Vec<_> = document.iter().map(|text| highlight(Some("rust"), text, None)).collect();

        for (text, before) in document.iter().zip(&first) {
            let after = highlight(Some("rust"), text, None);
            assert!(Arc::ptr_eq(before, &after), "a block of the open document was evicted before it was re-read");
        }
    }

    #[test]
    fn every_distinct_bad_theme_name_is_reported_once() {
        let _guard = exclusively();
        forget_what_was_said();

        theme_for(Some("no-such-theme"));
        theme_for(Some("no-such-theme"));
        theme_for(Some("another-missing-theme"));

        let warnings = take_warnings();
        assert_eq!(warnings.len(), 2, "expected one per distinct name, got {warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("no-such-theme")));
        assert!(warnings.iter().any(|warning| warning.contains("another-missing-theme")));
        assert!(warnings.iter().all(|warning| warning.contains(FALLBACK)), "the fallback is not named");
    }

    #[test]
    fn taking_the_warnings_empties_them() {
        let _guard = exclusively();
        forget_what_was_said();
        theme_for(Some("gone-missing"));
        assert!(!take_warnings().is_empty());
        assert!(take_warnings().is_empty(), "a warning survived being collected");
    }

    #[test]
    fn a_name_already_reported_is_not_reported_again_after_collection() {
        let _guard = exclusively();
        forget_what_was_said();

        theme_for(Some("only-once"));
        assert_eq!(take_warnings().len(), 1);

        for _ in 0..5 {
            theme_for(Some("only-once"));
        }
        assert!(take_warnings().is_empty(), "the same name was reported twice");
    }

    #[test]
    fn a_theme_that_exists_says_nothing() {
        let _guard = exclusively();
        forget_what_was_said();
        theme_for(Some(FALLBACK));
        theme_for(None);
        assert!(take_warnings().is_empty());
    }

    #[test]
    fn every_bundled_language_is_highlighted() {
        let samples = [
            ("swift", "import Foundation\nclass Greeter {\n    func greet() -> Bool { return true }\n}\n"),
            ("kotlin", "import java.util.Date\nclass Greeter {\n    fun greet(): Boolean { return true }\n}\n"),
            ("toml", "[package]\nedition = 2024\n"),
            ("typescript", "import fs from 'node:fs';\nclass Greeter {\n    greet(): boolean { return true }\n}\n"),
        ];
        for (lang, code) in samples {
            let lines = highlight(Some(lang), code, None);
            assert!(distinct(&foregrounds(&lines)) > 1, "{lang} came out in one color, so it was not highlighted");
        }
    }

    #[test]
    fn the_bundled_languages_answer_to_their_usual_fence_tags() {
        let syntaxes = syntax_set();
        for (tag, expected) in
            [("swift", "Swift"), ("kotlin", "Kotlin"), ("toml", "TOML"), ("ts", "TypeScript"), ("typescript", "TypeScript")]
        {
            let syntax = syntax_for(syntaxes, Some(tag), "");
            assert_eq!(syntax.name, expected, "the tag {tag:?} found {:?}", syntax.name);
        }
    }

    #[test]
    fn the_bundled_themes_are_listed() {
        let names = available();
        assert!(names.contains(&String::from(FALLBACK)), "{names:?}");
        assert!(names.contains(&String::from("InspiredGitHub")), "{names:?}");
    }
}
