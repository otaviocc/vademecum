//! Colors as a theme file writes them.
//!
//! A value is an 8-bit index, a hex triplet, `reset`, or one of the sixteen
//! ANSI names. An element's `fg`/`bg` may additionally name a palette slot; a
//! slot may not, since a slot naming a slot is a cycle.

use std::fmt;

use ratatui::style::Color;
use serde::de::{self, Deserialize, Deserializer, Visitor};

use crate::theme::palette::Palette;

/// A color exactly as written, before it is known what it resolves to. TOML
/// spells an index as an integer and everything else as a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorSpec {
    Index(u8),
    Name(String),
}

/// A value that is not any accepted form of color.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value} is not a color: expected {expected}")]
pub struct ColorError {
    value: String,
    expected: &'static str,
}

const PALETTE_FORMS: &str = "an ANSI color name, an index 0-255, #rrggbb, or reset";
const ELEMENT_FORMS: &str = "a palette slot, an ANSI color name, an index 0-255, #rrggbb, or reset";
/// What `bg` accepts and no other key does: the absence of a color.
const NO_COLOR: &str = "none";
const NONE_FORMS: &str = "a color; \"none\" removes a background and is accepted for bg alone";

impl ColorSpec {
    /// Resolve as a `[palette]` value: no slot names, since they are what is
    /// being defined.
    pub fn resolve(&self) -> Result<Color, ColorError> {
        match self {
            ColorSpec::Index(index) => Ok(Color::Indexed(*index)),
            ColorSpec::Name(name) => literal(name).ok_or_else(|| ColorError::new(name, PALETTE_FORMS)),
        }
    }

    /// Resolve as an `[elements.*]` value, where a palette slot is a color.
    /// The slot wins over a literal of the same spelling; none of the fourteen
    /// slot names is also an ANSI name, so there is nothing to shadow.
    pub fn resolve_against(&self, palette: &Palette) -> Result<Color, ColorError> {
        match self {
            ColorSpec::Index(index) => Ok(Color::Indexed(*index)),
            // Reached only where a color is required. A background asks
            // `removes_color` first and never gets here.
            ColorSpec::Name(name) if name == NO_COLOR => Err(ColorError::new(name, NONE_FORMS)),
            ColorSpec::Name(name) => {
                palette.slot(name).or_else(|| literal(name)).ok_or_else(|| ColorError::new(name, ELEMENT_FORMS))
            }
        }
    }

    /// `bg = "none"`: strip the element's background instead of setting one.
    ///
    /// Not the same as `reset`, which is a color — the terminal's own
    /// background, painted *over* whatever is beneath. `none` paints nothing,
    /// which is what lets `ansi` style code by foreground alone and leave the
    /// cursor line's band to show through.
    pub fn removes_color(&self) -> bool {
        matches!(self, ColorSpec::Name(name) if name == NO_COLOR)
    }
}

impl ColorError {
    fn new(value: &str, expected: &'static str) -> Self {
        Self { value: format!("{value:?}"), expected }
    }
}

/// `reset`, an ANSI name, or `#rrggbb`.
fn literal(value: &str) -> Option<Color> {
    if let Some(hex) = value.strip_prefix('#') {
        return rgb(hex);
    }
    Some(match value {
        "reset" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" => Color::Gray,
        "dark_gray" => Color::DarkGray,
        "light_red" => Color::LightRed,
        "light_green" => Color::LightGreen,
        "light_yellow" => Color::LightYellow,
        "light_blue" => Color::LightBlue,
        "light_magenta" => Color::LightMagenta,
        "light_cyan" => Color::LightCyan,
        "white" => Color::White,
        _ => return None,
    })
}

/// Exactly six hex digits. Three-digit shorthand is not accepted: guessing
/// which of two readings a file meant is worse than saying it is unreadable.
fn rgb(hex: &str) -> Option<Color> {
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
    Some(Color::Rgb(channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

impl<'de> Deserialize<'de> for ColorSpec {
    /// Hand-written rather than `#[serde(untagged)]`: these values arrive
    /// through a flattened map, whose buffered deserializer does not give an
    /// untagged enum the integer type it was written with.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ColorSpecVisitor)
    }
}

struct ColorSpecVisitor;

impl Visitor<'_> for ColorSpecVisitor {
    type Value = ColorSpec;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a color name, #rrggbb, or an index 0-255")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(ColorSpec::Name(value.to_owned()))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        u8::try_from(value).map(ColorSpec::Index).map_err(|_| E::custom(format!("{value} is not an index 0-255")))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        u8::try_from(value).map(ColorSpec::Index).map_err(|_| E::custom(format!("{value} is not an index 0-255")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &str) -> ColorSpec {
        ColorSpec::Name(value.to_owned())
    }

    #[test]
    fn ansi_names_resolve() {
        assert_eq!(name("blue").resolve(), Ok(Color::Blue));
        assert_eq!(name("dark_gray").resolve(), Ok(Color::DarkGray));
        assert_eq!(name("light_magenta").resolve(), Ok(Color::LightMagenta));
        assert_eq!(name("reset").resolve(), Ok(Color::Reset));
    }

    #[test]
    fn hex_resolves_to_truecolor() {
        assert_eq!(name("#89b4fa").resolve(), Ok(Color::Rgb(0x89, 0xB4, 0xFA)));
        assert_eq!(name("#89B4FA").resolve(), Ok(Color::Rgb(0x89, 0xB4, 0xFA)));
    }

    #[test]
    fn an_index_resolves_to_itself() {
        assert_eq!(ColorSpec::Index(208).resolve(), Ok(Color::Indexed(208)));
    }

    #[test]
    fn a_slot_is_a_color_only_for_an_element() {
        let palette = Palette::default();
        assert_eq!(name("accent").resolve_against(&palette), Ok(Color::Cyan));
        assert!(name("accent").resolve().is_err(), "a palette slot must not name another slot");
    }

    #[test]
    fn an_element_still_takes_a_literal() {
        let palette = Palette::default();
        assert_eq!(name("#ff0000").resolve_against(&palette), Ok(Color::Rgb(0xFF, 0, 0)));
        assert_eq!(name("light_blue").resolve_against(&palette), Ok(Color::LightBlue));
    }

    #[test]
    fn unreadable_values_are_errors_rather_than_guesses() {
        for value in ["", "#abc", "#12345g", "#1234567", "blurple", "Blue", "dark gray", "208"] {
            assert!(name(value).resolve().is_err(), "{value:?} resolved to a color");
        }
    }

    #[test]
    fn the_error_says_what_was_expected() {
        let error = name("blurple").resolve().expect_err("blurple is not a color");
        let message = error.to_string();
        assert!(message.contains("blurple"), "{message}");
        assert!(message.contains("#rrggbb"), "{message}");
    }

    #[test]
    fn toml_spells_a_color_as_a_string_or_an_integer() {
        #[derive(serde::Deserialize)]
        struct Holder {
            fg: ColorSpec,
        }

        let string: Holder = toml::from_str(r#"fg = "accent""#).expect("a string is a color");
        assert_eq!(string.fg, name("accent"));

        let integer: Holder = toml::from_str("fg = 208").expect("an integer is a color");
        assert_eq!(integer.fg, ColorSpec::Index(208));

        assert!(toml::from_str::<Holder>("fg = 300").is_err(), "300 is not an 8-bit index");
        assert!(toml::from_str::<Holder>("fg = -1").is_err(), "-1 is not an 8-bit index");
    }
}
