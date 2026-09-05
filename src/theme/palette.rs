//! The semantic color slots every element style derives from.

use ratatui::style::Color;

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

impl Default for Palette {
    /// `default-plus`, the built-in every partial theme file falls back to.
    fn default() -> Self {
        Self {
            background: Color::Rgb(0x1E, 0x1E, 0x1E),
            foreground: Color::Rgb(0xFF, 0xFF, 0xFF),
            muted: Color::Rgb(0x4D, 0x4D, 0x4D),
            muted_text: Color::Rgb(0x8E, 0x8E, 0x8E),
            subtle: Color::Rgb(0x2A, 0x2A, 0x2A),
            selection_background: Color::Rgb(0x54, 0x55, 0x4A),
            selection_foreground: Color::Rgb(0xFF, 0xFF, 0xFF),
            error: Color::Rgb(0xFC, 0x46, 0x51),
            success: Color::Rgb(0x2E, 0xA8, 0x5B),
            warning: Color::Rgb(0xFF, 0xE7, 0x6D),
            accent: Color::Rgb(0x56, 0xD0, 0xB3),
            chrome: Color::Rgb(0x56, 0xD0, 0xB3),
            highlight: Color::Rgb(0x35, 0xB0, 0xD8),
            notice: Color::Rgb(0xF2, 0x24, 0x8C),
        }
    }
}
