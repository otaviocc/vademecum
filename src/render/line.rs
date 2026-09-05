//! The contract shared by the TUI and the stdout writer.
//!
//! Everything after layout works on `RenderedLine`s: the TUI paints their
//! spans into the ratatui buffer, the stdout writer serializes them to ANSI,
//! search scans their text, and link navigation reads their `links`.

use std::ops::Range;
use std::path::PathBuf;

use ratatui::style::Style;
use unicode_width::UnicodeWidthStr;

use crate::markdown::links::LinkKind;

/// A run of text sharing one style.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

impl StyledSpan {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self { text: text.into(), style }
    }

    /// Display columns, not bytes or chars.
    pub fn width(&self) -> usize {
        self.text.width()
    }
}

/// A link occupying a range of spans on one line. A link broken across a wrap
/// becomes one `LinkRef` per line.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRef {
    pub span_range: Range<usize>,
    pub kind: LinkKind,
    /// The file a Local or Wiki link points at, once resolution has run.
    pub resolved: Option<PathBuf>,
}

/// One line of laid-out output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RenderedLine {
    pub spans: Vec<StyledSpan>,
    pub links: Vec<LinkRef>,
    /// Set on the line a heading renders to, so fragments can jump to it.
    pub anchor: Option<String>,
    /// The 1-based line of the source this came from.
    pub source_line: usize,
}

impl RenderedLine {
    /// A blank line, belonging to no source line in particular.
    pub fn blank() -> Self {
        Self::default()
    }

    pub fn is_blank(&self) -> bool {
        self.spans.iter().all(|span| span.text.is_empty())
    }

    /// The line as plain text — what search scans and what `--color never`
    /// writes.
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    /// Display columns.
    pub fn width(&self) -> usize {
        self.spans.iter().map(StyledSpan::width).sum()
    }

    /// Add a span to the end of the line.
    pub fn push(&mut self, span: StyledSpan) {
        self.spans.push(span);
    }

    /// Append another line's spans and links, rebasing its link ranges.
    pub fn append(&mut self, other: RenderedLine) {
        let offset = self.spans.len();
        self.links.extend(
            other
                .links
                .into_iter()
                .map(|link| LinkRef { span_range: link.span_range.start + offset..link.span_range.end + offset, ..link }),
        );
        self.spans.extend(other.spans);
    }

    /// Put a span in front of the line, shifting the link ranges that follow.
    pub fn prefix(&mut self, span: StyledSpan) {
        self.spans.insert(0, span);
        for link in &mut self.links {
            link.span_range = link.span_range.start + 1..link.span_range.end + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> RenderedLine {
        RenderedLine {
            spans: vec![StyledSpan::new("hello ", Style::default()), StyledSpan::new("world", Style::default())],
            links: vec![LinkRef {
                span_range: 1..2,
                kind: LinkKind::Wiki { target: "world".into(), fragment: None },
                resolved: None,
            }],
            anchor: None,
            source_line: 7,
        }
    }

    #[test]
    fn text_concatenates_the_spans() {
        assert_eq!(line().text(), "hello world");
    }

    #[test]
    fn width_counts_display_columns() {
        assert_eq!(line().width(), 11);
        assert_eq!(StyledSpan::new("日本語", Style::default()).width(), 6);
    }

    #[test]
    fn prefixing_shifts_the_link_ranges() {
        let mut line = line();
        line.prefix(StyledSpan::new("┃ ", Style::default()));
        assert_eq!(line.text(), "┃ hello world");
        assert_eq!(line.links[0].span_range, 2..3);
    }

    #[test]
    fn appending_rebases_the_link_ranges() {
        let mut target = RenderedLine::default();
        target.push(StyledSpan::new("| ", Style::default()));
        target.append(line());
        assert_eq!(target.text(), "| hello world");
        assert_eq!(target.links[0].span_range, 2..3);
    }

    #[test]
    fn a_blank_line_is_blank() {
        assert!(RenderedLine::blank().is_blank());
        assert!(!line().is_blank());
    }
}
