//! Themes: a palette of semantic colors, and the element styles derived from
//! it.
//!
//! The built-in `ansi` is the ground truth: it is what a reader gets without
//! asking, and the defaults every partial theme file merges over. `loader`
//! reads the files; everything else here is the table they merge into.

pub mod color;
pub mod elements;
pub mod loader;
pub mod palette;

use ratatui::style::Style;

pub use elements::Element;
pub use palette::Palette;

/// A palette with every element style already resolved against it.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub palette: Palette,
    /// Indexed by `Element as usize`; resolved once, read on every line.
    styles: Vec<Style>,
    /// The theme's own `name`, for the record; the file it came from when it
    /// does not name itself.
    #[allow(dead_code, reason = "shown by the UI in milestone 4")]
    pub name: String,
    /// The syntect `.tmTheme` code blocks are highlighted with.
    pub syntax_theme: Option<String>,
}

impl Theme {
    pub fn new(palette: Palette) -> Self {
        let styles = Element::ALL.iter().map(|element| elements::default_style(*element, &palette)).collect();
        Self { palette, styles, name: String::from("ansi"), syntax_theme: None }
    }

    pub fn style(&self, element: Element) -> Style {
        self.styles[element as usize]
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new(Palette::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    #[test]
    fn every_element_is_listed_once_in_discriminant_order() {
        for (index, element) in Element::ALL.iter().enumerate() {
            assert_eq!(*element as usize, index, "{element:?} is out of order in Element::ALL");
        }
    }

    #[test]
    fn styles_are_looked_up_by_element() {
        let theme = Theme::default();
        assert_eq!(theme.style(Element::Heading1).fg, Some(theme.palette.accent));
        assert!(theme.style(Element::Heading1).add_modifier.contains(Modifier::BOLD));
        // Code carries no background by default; a theme whose `subtle` is a
        // real tint rather than ANSI bright black asks for one back. The cursor
        // line is the slot's one default user.
        assert_eq!(theme.style(Element::InlineCode).bg, None);
        assert_eq!(theme.style(Element::CursorLine).bg, Some(theme.palette.subtle));
    }

    #[test]
    fn a_repalette_moves_every_element_with_it() {
        let palette = Palette { accent: Color::Red, ..Palette::default() };
        let theme = Theme::new(palette);
        assert_eq!(theme.style(Element::Heading1).fg, Some(Color::Red));
        assert_eq!(theme.style(Element::Heading2).fg, Some(Color::Red));
    }

    #[test]
    fn headings_map_to_their_level() {
        assert_eq!(Element::heading(1), Element::Heading1);
        assert_eq!(Element::heading(4), Element::Heading4);
        assert_eq!(Element::heading(6), Element::Heading6);
    }

    #[test]
    fn emphasis_and_strong_carry_no_color_of_their_own() {
        let theme = Theme::default();
        assert_eq!(theme.style(Element::Emphasis).fg, None);
        assert_eq!(theme.style(Element::Strong).fg, None);
    }
}
