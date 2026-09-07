//! The contract shared by the TUI and the stdout writer.

use std::ops::Range;
use std::path::PathBuf;

use ratatui::style::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
    pub inset: usize,
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

    pub fn link_at(&self, column: usize) -> Option<usize> {
        let mut x = 0;
        let span = self.spans.iter().position(|span| {
            let width = span.width();
            let hit = width > 0 && (x..x + width).contains(&column);
            x += width;
            hit
        })?;
        self.links.iter().position(|link| link.span_range.contains(&span))
    }

    pub fn byte_at(&self, column: usize) -> usize {
        let mut x = 0;
        let mut byte = 0;
        for span in &self.spans {
            for character in span.text.chars() {
                if x >= column {
                    return byte;
                }
                x += UnicodeWidthChar::width(character).unwrap_or(0);
                byte += character.len_utf8();
            }
        }
        byte
    }

    pub fn prefix(&mut self, span: StyledSpan) {
        if self.inset > 0 {
            self.inset += span.width();
        }
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
            ..RenderedLine::default()
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
    fn a_column_finds_the_link_it_lands_inside() {
        let line = line();
        assert_eq!(line.link_at(6), Some(0));
        assert_eq!(line.link_at(10), Some(0));
    }

    #[test]
    fn a_column_outside_every_link_finds_none() {
        let line = line();
        assert_eq!(line.link_at(0), None, "before the link");
        assert_eq!(line.link_at(5), None, "the space between");
        assert_eq!(line.link_at(11), None, "past the end of the line");
        assert_eq!(line.link_at(usize::MAX), None);
    }

    #[test]
    fn a_column_finds_the_byte_the_text_starts_at() {
        let line = line();
        assert_eq!(line.byte_at(0), 0);
        assert_eq!(line.byte_at(6), 6);
        assert_eq!(&line.text()[line.byte_at(6)..], "world");
    }

    #[test]
    fn a_column_past_the_end_of_the_line_finds_its_last_byte() {
        let line = line();
        assert_eq!(line.byte_at(11), 11);
        assert_eq!(line.byte_at(usize::MAX), 11);
        assert_eq!(RenderedLine::blank().byte_at(4), 0);
    }

    #[test]
    fn a_column_inside_a_wide_glyph_finds_the_boundary_after_it() {
        let mut line = RenderedLine::default();
        line.push(StyledSpan::new("日本語", Style::default()));

        assert_eq!(line.byte_at(0), 0);
        assert_eq!(line.byte_at(2), 3, "the second glyph starts at column 2");
        assert_eq!(line.byte_at(3), 6, "a column inside a glyph does not split it");
    }

    #[test]
    fn a_column_picks_the_second_of_two_links_on_one_line() {
        let mut line = RenderedLine::default();
        for (text, target) in [("one", Some("a")), (" and ", None), ("two", Some("b"))] {
            let index = line.spans.len();
            line.push(StyledSpan::new(text, Style::default()));
            if let Some(target) = target {
                line.links.push(LinkRef {
                    span_range: index..index + 1,
                    kind: LinkKind::Wiki { target: target.into(), fragment: None },
                    resolved: None,
                });
            }
        }

        assert_eq!(line.link_at(1), Some(0));
        assert_eq!(line.link_at(4), None);
        assert_eq!(line.link_at(9), Some(1));
    }

    #[test]
    fn a_column_counts_display_width_rather_than_bytes() {
        let mut line = RenderedLine::default();
        line.push(StyledSpan::new("日本語", Style::default()));
        line.push(StyledSpan::new("link", Style::default()));
        line.links.push(LinkRef {
            span_range: 1..2,
            kind: LinkKind::Wiki { target: "x".into(), fragment: None },
            resolved: None,
        });

        assert_eq!(line.link_at(5), None, "still inside the three wide characters");
        assert_eq!(line.link_at(6), Some(0));
    }

    #[test]
    fn an_empty_span_swallows_no_column() {
        let mut line = RenderedLine::default();
        line.push(StyledSpan::new("", Style::default()));
        line.push(StyledSpan::new("link", Style::default()));
        line.links.push(LinkRef {
            span_range: 1..2,
            kind: LinkKind::Wiki { target: "x".into(), fragment: None },
            resolved: None,
        });

        assert_eq!(line.link_at(0), Some(0));
    }

    #[test]
    fn prefixing_shifts_the_link_ranges() {
        let mut line = line();
        line.prefix(StyledSpan::new("┃ ", Style::default()));
        assert_eq!(line.text(), "┃ hello world");
        assert_eq!(line.links[0].span_range, 2..3);
    }

    #[test]
    fn prefixing_pushes_the_inset_along_with_the_text() {
        let mut padded = line();
        padded.inset = 1;
        padded.prefix(StyledSpan::new("┃ ", Style::default()));
        assert_eq!(padded.inset, 3, "the inset no longer points at the text it was measuring");
        let text = padded.text();
        assert_eq!(&text[padded.byte_at(padded.inset)..], "ello world");
    }

    #[test]
    fn prefixing_a_line_without_an_inset_leaves_it_without_one() {
        let mut plain = line();
        plain.prefix(StyledSpan::new("┃ ", Style::default()));
        assert_eq!(plain.inset, 0, "a line with no chrome of its own gained some");
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
