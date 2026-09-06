//! The contract shared by the TUI and the stdout writer.

use std::ops::Range;
use std::path::PathBuf;

use ratatui::style::Style;
use unicode_width::UnicodeWidthStr;

use crate::markdown::links::LinkKind;

#[derive(Debug, Clone, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

impl StyledSpan {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self { text: text.into(), style }
    }

    pub fn width(&self) -> usize {
        self.text.width()
    }
}

pub fn merge(spans: impl IntoIterator<Item = StyledSpan>) -> Vec<StyledSpan> {
    spans.into_iter().filter(|span| !span.text.is_empty()).fold(Vec::new(), |mut merged: Vec<StyledSpan>, span| {
        match merged.last_mut() {
            Some(last) if last.style == span.style => last.text.push_str(&span.text),
            _ => merged.push(span),
        }
        merged
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkRef {
    pub span_range: Range<usize>,
    pub kind: LinkKind,
    pub resolved: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RenderedLine {
    pub spans: Vec<StyledSpan>,
    pub links: Vec<LinkRef>,
    pub anchor: Option<String>,
    pub source_line: usize,
}

impl RenderedLine {
    pub fn blank() -> Self {
        Self::default()
    }

    pub fn is_blank(&self) -> bool {
        self.spans.iter().all(|span| span.text.is_empty())
    }

    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(StyledSpan::width).sum()
    }

    pub fn push(&mut self, span: StyledSpan) {
        self.spans.push(span);
    }

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
    fn merging_joins_only_neighbours_of_one_style() {
        let bold = Style::default().add_modifier(ratatui::style::Modifier::BOLD);
        let spans = vec![
            StyledSpan::new("a", Style::default()),
            StyledSpan::new("", Style::default()),
            StyledSpan::new("b", Style::default()),
            StyledSpan::new("c", bold),
            StyledSpan::new("d", Style::default()),
        ];
        let merged = merge(spans);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].text, "ab");
        assert_eq!(merged[1].text, "c");
        assert_eq!(merged[2].text, "d");
    }

    #[test]
    fn a_blank_line_is_blank() {
        assert!(RenderedLine::blank().is_blank());
        assert!(!line().is_blank());
    }
}
