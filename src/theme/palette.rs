//! The semantic color slots every element style derives from.

use ratatui::style::Color;
use serde::Deserialize;

use crate::theme::color::ColorSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
    pub muted_text: Color,
    pub subtle: Color,
    pub cursor: Color,
    pub selection_background: Color,
    pub selection_foreground: Color,
    pub error: Color,
    pub success: Color,
    pub warning: Color,
    pub accent: Color,
    pub chrome: Color,
    pub highlight: Color,
    pub notice: Color,
}

impl Palette {
    pub fn slot(&self, name: &str) -> Option<Color> {
        Some(match name {
            "background" => self.background,
            "foreground" => self.foreground,
            "muted" => self.muted,
            "muted_text" => self.muted_text,
            "subtle" => self.subtle,
            "cursor" => self.cursor,
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
    fn default() -> Self {
        Self {
            background: Color::Reset,
            foreground: Color::Reset,
            muted: Color::DarkGray,
            muted_text: Color::Gray,
            subtle: Color::DarkGray,
            cursor: Color::DarkGray,
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

#[derive(Debug, Default, Deserialize)]
pub struct PaletteFile {
    pub background: Option<ColorSpec>,
    pub foreground: Option<ColorSpec>,
    pub muted: Option<ColorSpec>,
    pub muted_text: Option<ColorSpec>,
    pub subtle: Option<ColorSpec>,
    pub cursor: Option<ColorSpec>,
    pub selection_background: Option<ColorSpec>,
    pub selection_foreground: Option<ColorSpec>,
    pub error: Option<ColorSpec>,
    pub success: Option<ColorSpec>,
    pub warning: Option<ColorSpec>,
    pub accent: Option<ColorSpec>,
    pub chrome: Option<ColorSpec>,
    pub highlight: Option<ColorSpec>,
    pub notice: Option<ColorSpec>,
    #[serde(flatten)]
    pub unknown: std::collections::BTreeMap<String, toml::Value>,
}

impl PaletteFile {
    pub fn slots(&self) -> [(&'static str, Option<&ColorSpec>); 15] {
        [
            ("background", self.background.as_ref()),
            ("foreground", self.foreground.as_ref()),
            ("muted", self.muted.as_ref()),
            ("muted_text", self.muted_text.as_ref()),
            ("subtle", self.subtle.as_ref()),
            ("cursor", self.cursor.as_ref()),
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

    pub fn merge(self, base: Self) -> Self {
        let mut unknown = base.unknown;
        unknown.extend(self.unknown);
        Self {
            background: self.background.or(base.background),
            foreground: self.foreground.or(base.foreground),
            muted: self.muted.or(base.muted),
            muted_text: self.muted_text.or(base.muted_text),
            subtle: self.subtle.or(base.subtle),
            cursor: self.cursor.or(base.cursor),
            selection_background: self.selection_background.or(base.selection_background),
            selection_foreground: self.selection_foreground.or(base.selection_foreground),
            error: self.error.or(base.error),
            success: self.success.or(base.success),
            warning: self.warning.or(base.warning),
            accent: self.accent.or(base.accent),
            chrome: self.chrome.or(base.chrome),
            highlight: self.highlight.or(base.highlight),
            notice: self.notice.or(base.notice),
            unknown,
        }
    }

    pub fn set(palette: &mut Palette, name: &str, color: Color) {
        match name {
            "background" => palette.background = color,
            "foreground" => palette.foreground = color,
            "muted" => palette.muted = color,
            "muted_text" => palette.muted_text = color,
            "subtle" => palette.subtle = color,
            "cursor" => palette.cursor = color,
            "selection_background" => palette.selection_background = color,
            "selection_foreground" => palette.selection_foreground = color,
            "error" => palette.error = color,
            "success" => palette.success = color,
            "warning" => palette.warning = color,
            "accent" => palette.accent = color,
            "chrome" => palette.chrome = color,
            "highlight" => palette.highlight = color,
            "notice" => palette.notice = color,
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
    fn every_slot_merges_from_the_child_and_no_other_slot_moves() {
        let names: Vec<&str> = PaletteFile::default().slots().iter().map(|(name, _)| *name).collect();
        let every = names.iter().map(|name| format!("{name} = 1\n")).collect::<String>();

        for name in &names {
            let base: PaletteFile = toml::from_str(&every).expect("a palette naming every slot");
            let child: PaletteFile = toml::from_str(&format!("{name} = 208\n")).expect("a palette naming one slot");

            let mut palette = Palette::default();
            for (slot, spec) in child.merge(base).slots() {
                let color = spec.expect("the merge kept every slot").resolve().expect("a color");
                PaletteFile::set(&mut palette, slot, color);
            }

            assert_eq!(palette.slot(name), Some(Color::Indexed(208)), "{name} did not come from the child");
            for other in names.iter().filter(|other| *other != name) {
                assert_eq!(palette.slot(other), Some(Color::Indexed(1)), "{other} moved when {name} was merged");
            }
        }
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
