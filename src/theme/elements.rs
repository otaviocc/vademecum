//! The element table: every styled thing vademecum draws.

use serde::Deserialize;

use ratatui::style::{Color, Modifier, Style};

use crate::theme::color::ColorSpec;
use crate::theme::palette::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Element {
    Paragraph,
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    Emphasis,
    Strong,
    Strikethrough,
    InlineCode,
    CodeBlock,
    CodeBlockLang,
    Quote,
    ListBullet,
    ListNumber,
    TaskDone,
    TaskTodo,
    Link,
    Wikilink,
    LinkFocused,
    LinkUnfocused,
    LinkBroken,
    Image,
    Footnote,
    Html,
    TableHeader,
    TableBorder,
    Hr,
    HeaderTitle,
    Hint,
    Status,
    StatusNotice,
    StatusError,
    CursorLine,
    SearchMatch,
    SearchCurrent,
    Selection,
    HelpWindow,
}

impl Element {
    pub const ALL: [Element; 39] = [
        Element::Paragraph,
        Element::Heading1,
        Element::Heading2,
        Element::Heading3,
        Element::Heading4,
        Element::Heading5,
        Element::Heading6,
        Element::Emphasis,
        Element::Strong,
        Element::Strikethrough,
        Element::InlineCode,
        Element::CodeBlock,
        Element::CodeBlockLang,
        Element::Quote,
        Element::ListBullet,
        Element::ListNumber,
        Element::TaskDone,
        Element::TaskTodo,
        Element::Link,
        Element::Wikilink,
        Element::LinkFocused,
        Element::LinkUnfocused,
        Element::LinkBroken,
        Element::Image,
        Element::Footnote,
        Element::Html,
        Element::TableHeader,
        Element::TableBorder,
        Element::Hr,
        Element::HeaderTitle,
        Element::Hint,
        Element::Status,
        Element::StatusNotice,
        Element::StatusError,
        Element::CursorLine,
        Element::SearchMatch,
        Element::SearchCurrent,
        Element::Selection,
        Element::HelpWindow,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Element::Paragraph => "paragraph",
            Element::Heading1 => "heading1",
            Element::Heading2 => "heading2",
            Element::Heading3 => "heading3",
            Element::Heading4 => "heading4",
            Element::Heading5 => "heading5",
            Element::Heading6 => "heading6",
            Element::Emphasis => "emphasis",
            Element::Strong => "strong",
            Element::Strikethrough => "strikethrough",
            Element::InlineCode => "inline_code",
            Element::CodeBlock => "code_block",
            Element::CodeBlockLang => "code_block_lang",
            Element::Quote => "quote",
            Element::ListBullet => "list_bullet",
            Element::ListNumber => "list_number",
            Element::TaskDone => "task_done",
            Element::TaskTodo => "task_todo",
            Element::Link => "link",
            Element::Wikilink => "wikilink",
            Element::LinkFocused => "link_focused",
            Element::LinkUnfocused => "link_unfocused",
            Element::LinkBroken => "link_broken",
            Element::Image => "image",
            Element::Footnote => "footnote",
            Element::Html => "html",
            Element::TableHeader => "table_header",
            Element::TableBorder => "table_border",
            Element::Hr => "hr",
            Element::HeaderTitle => "header_title",
            Element::Hint => "hint",
            Element::Status => "status",
            Element::StatusNotice => "status_notice",
            Element::StatusError => "status_error",
            Element::CursorLine => "cursor_line",
            Element::SearchMatch => "search_match",
            Element::SearchCurrent => "search_current",
            Element::Selection => "selection",
            Element::HelpWindow => "help_window",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Element::ALL.into_iter().find(|element| element.key() == key)
    }

    pub fn heading(level: u8) -> Self {
        match level {
            1 => Element::Heading1,
            2 => Element::Heading2,
            3 => Element::Heading3,
            4 => Element::Heading4,
            5 => Element::Heading5,
            _ => Element::Heading6,
        }
    }
}

pub fn default_style(element: Element, palette: &Palette) -> Style {
    let style = Style::default();
    match element {
        Element::Paragraph => style.fg(palette.foreground),
        Element::Heading1 | Element::Heading2 => style.fg(palette.accent).add_modifier(Modifier::BOLD),
        Element::Heading3 => style.fg(palette.highlight).add_modifier(Modifier::BOLD),
        Element::Heading4 | Element::Heading5 | Element::Heading6 => style.fg(palette.highlight),
        Element::Emphasis => style.add_modifier(Modifier::ITALIC),
        Element::Strong => style.add_modifier(Modifier::BOLD),
        Element::Strikethrough => style.fg(palette.muted_text).add_modifier(Modifier::CROSSED_OUT),
        Element::InlineCode => style.fg(palette.warning),
        Element::CodeBlock => style.fg(palette.foreground),
        Element::CodeBlockLang => style.fg(palette.muted_text),
        Element::Quote => style.fg(palette.muted_text).add_modifier(Modifier::ITALIC),
        Element::ListBullet | Element::ListNumber => style.fg(palette.accent),
        Element::TaskDone => style.fg(palette.success),
        Element::TaskTodo => style.fg(palette.muted_text),
        Element::Link => style.fg(palette.highlight).add_modifier(Modifier::UNDERLINED),
        Element::Wikilink => style.fg(palette.notice).add_modifier(Modifier::BOLD),
        Element::LinkFocused => {
            style.fg(palette.selection_foreground).bg(palette.selection_background).add_modifier(Modifier::BOLD)
        }
        Element::LinkUnfocused => style.add_modifier(Modifier::UNDERLINED),
        Element::LinkBroken => style.fg(palette.error).add_modifier(Modifier::CROSSED_OUT),
        Element::Image => style.fg(palette.muted_text).add_modifier(Modifier::ITALIC),
        Element::Footnote => style.fg(palette.muted_text),
        Element::Html => style.fg(palette.muted_text).add_modifier(Modifier::DIM),
        Element::TableHeader => style.fg(palette.foreground).add_modifier(Modifier::BOLD),
        Element::TableBorder | Element::Hr => style.fg(palette.muted),
        Element::HeaderTitle => style.fg(palette.chrome).add_modifier(Modifier::BOLD),
        Element::Hint => style.fg(palette.muted_text),
        Element::Status => style.fg(palette.foreground),
        Element::StatusNotice => style.fg(palette.notice),
        Element::StatusError => style.fg(palette.error).add_modifier(Modifier::BOLD),
        Element::CursorLine => style.bg(palette.cursor),
        Element::SearchMatch => style.fg(Color::Black).bg(palette.warning),
        Element::SearchCurrent => style.fg(Color::Black).bg(palette.notice).add_modifier(Modifier::BOLD),
        Element::Selection => style.fg(palette.selection_foreground).bg(palette.selection_background),
        Element::HelpWindow => style.fg(palette.foreground).bg(palette.background),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ElementFile {
    pub fg: Option<ColorSpec>,
    pub bg: Option<ColorSpec>,
    pub modifiers: Option<Vec<String>>,
    #[serde(flatten)]
    pub unknown: std::collections::BTreeMap<String, toml::Value>,
}

impl ElementFile {
    pub fn merge(self, base: Self) -> Self {
        let mut unknown = base.unknown;
        unknown.extend(self.unknown);
        Self { fg: self.fg.or(base.fg), bg: self.bg.or(base.bg), modifiers: self.modifiers.or(base.modifiers), unknown }
    }
}

pub fn modifier(name: &str) -> Option<Modifier> {
    Some(match name {
        "bold" => Modifier::BOLD,
        "italic" => Modifier::ITALIC,
        "underline" => Modifier::UNDERLINED,
        "dim" => Modifier::DIM,
        "reversed" => Modifier::REVERSED,
        "crossed_out" => Modifier::CROSSED_OUT,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_element_field_merges_independently_of_the_others() {
        let base: ElementFile = toml::from_str("fg = \"red\"\nmodifiers = [\"bold\"]\n").expect("a base element");
        let child: ElementFile = toml::from_str("bg = \"blue\"\n").expect("a child element");

        let merged = child.merge(base);
        assert_eq!(merged.fg, Some(ColorSpec::Name(String::from("red"))), "the base's foreground was dropped");
        assert_eq!(merged.bg, Some(ColorSpec::Name(String::from("blue"))));
        assert_eq!(merged.modifiers.as_deref(), Some(["bold".to_string()].as_slice()), "the base's modifiers were dropped");

        let base: ElementFile = toml::from_str("fg = \"red\"\n").expect("a base element");
        let child: ElementFile = toml::from_str("fg = \"green\"\nmodifiers = []\n").expect("a child element");
        let merged = child.merge(base);
        assert_eq!(merged.fg, Some(ColorSpec::Name(String::from("green"))), "the child does not win its own field");
        assert_eq!(merged.modifiers, Some(Vec::new()), "an empty list must survive as a way to clear them");
    }

    #[test]
    fn every_element_has_a_key_that_finds_it_again() {
        for element in Element::ALL {
            assert_eq!(Element::from_key(element.key()), Some(element), "{element:?} is not addressable by its key");
        }
    }

    #[test]
    fn keys_are_the_snake_case_names_the_readme_documents() {
        assert_eq!(Element::InlineCode.key(), "inline_code");
        assert_eq!(Element::Heading6.key(), "heading6");
        assert_eq!(Element::from_key("code_block_lang"), Some(Element::CodeBlockLang));
        assert_eq!(Element::from_key("headings"), None);
    }

    #[test]
    fn the_documented_modifiers_are_the_ones_accepted() {
        assert_eq!(modifier("bold"), Some(Modifier::BOLD));
        assert_eq!(modifier("underline"), Some(Modifier::UNDERLINED));
        assert_eq!(modifier("crossed_out"), Some(Modifier::CROSSED_OUT));
        assert_eq!(modifier("blinky"), None);
        assert_eq!(modifier("BOLD"), None);
    }
}
