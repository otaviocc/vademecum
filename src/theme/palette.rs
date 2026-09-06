//! The semantic color slots every element style derives from.

use ratatui::style::Color;
use serde::Deserialize;

use crate::theme::color::ColorSpec;

/// Semantic colors. Element styles never name a color directly; they name a
/// slot, so a theme file can restyle the whole document by changing this table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub background: Color,
    pub foreground: Color,
    /// Rules, borders, table lines.
    pub muted: Color,
    /// Hints, footers, HTML passthrough.
    pub muted_text: Color,
    /// Cursor line, code block background.
    pub subtle: Color,
    pub selection_background: Color,
    pub selection_foreground: Color,
    pub error: Color,
    pub success: Color,
    pub warning: Color,
    /// Headings, popup borders.
    pub accent: Color,
    /// Header title.
    pub chrome: Color,
    /// Links.
    pub highlight: Color,
    /// Wikilinks, transient status.
    pub notice: Color,
}

impl Palette {
    /// The slot a theme file names, for an element override like `fg =
    /// "accent"`. `None` when the name is not a slot at all.
    pub fn slot(&self, name: &str) -> Option<Color> {
        Some(match name {
            "background" => self.background,
            "foreground" => self.foreground,
            "muted" => self.muted,
            "muted_text" => self.muted_text,
            "subtle" => self.subtle,
            "selection_background" => self.selection_background,
            "selection_foreground" => self.selection_foreground,
            "error" => self.error,
            "success" => self.success,
            "warning" => self.warning,
            "accent" => self.accent,
            "chrome" => self.chrome,
            "highlight" => self.highlight,
            "notice" => self.notice,
            _ => return None,
        })
    }
}

impl Default for Palette {
    /// `ansi`, the built-in every partial theme file falls back to. Nothing
    /// here asserts a color the terminal has not already chosen, except
    /// `subtle`: it is the background of the cursor line and of code blocks,
    /// and `Reset` would make both of those invisible.
    fn default() -> Self {
        Self {
            background: Color::Reset,
            foreground: Color::Reset,
            muted: Color::DarkGray,
            muted_text: Color::Gray,
            subtle: Color::DarkGray,
            selection_background: Color::Blue,
            selection_foreground: Color::White,
            error: Color::Red,
            success: Color::Green,
            warning: Color::Yellow,
            accent: Color::Cyan,
            chrome: Color::Cyan,
            highlight: Color::Blue,
            notice: Color::Magenta,
        }
    }
}

/// A `[palette]` table as a theme file writes it: every slot optional, so a
/// file that names one color inherits the other thirteen.
#[derive(Debug, Default, Deserialize)]
pub struct PaletteFile {
    pub background: Option<ColorSpec>,
    pub foreground: Option<ColorSpec>,
    pub muted: Option<ColorSpec>,
    pub muted_text: Option<ColorSpec>,
    pub subtle: Option<ColorSpec>,
    pub selection_background: Option<ColorSpec>,
    pub selection_foreground: Option<ColorSpec>,
    pub error: Option<ColorSpec>,
    pub success: Option<ColorSpec>,
    pub warning: Option<ColorSpec>,
    pub accent: Option<ColorSpec>,
    pub chrome: Option<ColorSpec>,
    pub highlight: Option<ColorSpec>,
    pub notice: Option<ColorSpec>,
    /// Anything else the file wrote here. Warned about, never fatal.
    #[serde(flatten)]
    pub unknown: std::collections::BTreeMap<String, toml::Value>,
}

impl PaletteFile {
    /// Every slot this file sets, paired with the name it was written under so
    /// a bad value can be reported by name.
    pub fn slots(&self) -> [(&'static str, Option<&ColorSpec>); 14] {
        [
            ("background", self.background.as_ref()),
            ("foreground", self.foreground.as_ref()),
            ("muted", self.muted.as_ref()),
            ("muted_text", self.muted_text.as_ref()),
            ("subtle", self.subtle.as_ref()),
            ("selection_background", self.selection_background.as_ref()),
            ("selection_foreground", self.selection_foreground.as_ref()),
            ("error", self.error.as_ref()),
            ("success", self.success.as_ref()),
            ("warning", self.warning.as_ref()),
            ("accent", self.accent.as_ref()),
            ("chrome", self.chrome.as_ref()),
            ("highlight", self.highlight.as_ref()),
            ("notice", self.notice.as_ref()),
        ]
    }

    /// Write a resolved color into the slot of that name.
    pub fn set(palette: &mut Palette, name: &str, color: Color) {
        match name {
            "background" => palette.background = color,
            "foreground" => palette.foreground = color,
            "muted" => palette.muted = color,
            "muted_text" => palette.muted_text = color,
            "subtle" => palette.subtle = color,
            "selection_background" => palette.selection_background = color,
            "selection_foreground" => palette.selection_foreground = color,
            "error" => palette.error = color,
            "success" => palette.success = color,
            "warning" => palette.warning = color,
            "accent" => palette.accent = color,
            "chrome" => palette.chrome = color,
            "highlight" => palette.highlight = color,
            "notice" => palette.notice = color,
            // `slots()` is the only caller, and it only ever names its own keys.
            other => debug_assert!(false, "{other} is not a palette slot"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_asserts_nothing_the_terminal_has_not_chosen() {
        let palette = Palette::default();
        assert_eq!(palette.background, Color::Reset);
        assert_eq!(palette.foreground, Color::Reset);
    }

    #[test]
    fn slots_are_addressable_by_name() {
        let palette = Palette::default();
        assert_eq!(palette.slot("accent"), Some(Color::Cyan));
        assert_eq!(palette.slot("selection_foreground"), Some(Color::White));
        assert_eq!(palette.slot("nonesuch"), None);
    }

    #[test]
    fn every_slot_reads_back_what_was_written_to_it() {
        let empty = PaletteFile::default();
        for (name, _) in empty.slots() {
            let mut palette = Palette::default();
            PaletteFile::set(&mut palette, name, Color::Indexed(208));
            assert_eq!(palette.slot(name), Some(Color::Indexed(208)), "{name} did not survive the round trip");
        }
    }
}
