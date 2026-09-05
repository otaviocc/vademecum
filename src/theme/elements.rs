//! The element table: every styled thing vademecum draws, and how its default
//! style derives from the palette.

use ratatui::style::{Modifier, Style};

use crate::theme::palette::Palette;

/// Everything that can be styled. Theme files address elements by these names
/// in `snake_case`.
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
    HelpWindow,
}

impl Element {
    /// Every element, in discriminant order.
    pub const ALL: [Element; 37] = [
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
        Element::HelpWindow,
    ];

    /// The element a heading of this level is styled with.
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

/// An element's style before any theme file has its say.
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
        Element::InlineCode => style.fg(palette.warning).bg(palette.subtle),
        Element::CodeBlock => style.fg(palette.foreground).bg(palette.subtle),
        Element::CodeBlockLang => style.fg(palette.muted_text).bg(palette.subtle),
        Element::Quote => style.fg(palette.muted_text).add_modifier(Modifier::ITALIC),
        Element::ListBullet | Element::ListNumber => style.fg(palette.accent),
        Element::TaskDone => style.fg(palette.success),
        Element::TaskTodo => style.fg(palette.muted_text),
        Element::Link => style.fg(palette.highlight).add_modifier(Modifier::UNDERLINED),
        Element::Wikilink => style.fg(palette.notice).add_modifier(Modifier::BOLD),
        Element::LinkFocused => {
            style.fg(palette.selection_foreground).bg(palette.selection_background).add_modifier(Modifier::BOLD)
        }
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
        Element::CursorLine => style.bg(palette.subtle),
        Element::SearchMatch => style.fg(palette.background).bg(palette.warning),
        Element::SearchCurrent => style.fg(palette.background).bg(palette.notice).add_modifier(Modifier::BOLD),
        Element::HelpWindow => style.fg(palette.foreground).bg(palette.background),
    }
}
