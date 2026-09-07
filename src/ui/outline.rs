//! The document's headings, and the cursor moving over them.

use std::collections::HashMap;

use crate::markdown::ast::{Block, SourceBlock, plain_text};
use crate::render::line::RenderedLine;
use crate::ui::input::Motion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outline {
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub top: usize,
}

pub fn build(blocks: &[SourceBlock], lines: &[RenderedLine]) -> Vec<Entry> {
    let mut at: HashMap<&str, usize> = HashMap::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(anchor) = line.anchor.as_deref() {
            at.entry(anchor).or_insert(index);
        }
    }

    let mut entries = Vec::new();
    collect(blocks, &at, &mut entries);
    entries
}

fn collect(blocks: &[SourceBlock], at: &HashMap<&str, usize>, entries: &mut Vec<Entry>) {
    for block in blocks {
        match &block.block {
            Block::Heading { level, inlines, anchor } => {
                if let Some(&line) = at.get(anchor.as_str()) {
                    entries.push(Entry { level: *level, text: plain_text(inlines), line });
                }
            }
            Block::Quote(blocks) | Block::FootnoteDef { blocks, .. } => collect(blocks, at, entries),
            Block::List { items, .. } => {
                for item in items {
                    collect(&item.blocks, at, entries);
                }
            }
            _ => {}
        }
    }
}

impl Outline {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    pub fn open_at(&mut self, line: usize, height: usize) {
        self.selected = self.entries.iter().rposition(|entry| entry.line <= line).unwrap_or(0);
        self.reveal(height);
    }

    pub fn move_by(&mut self, motion: Motion, height: usize) {
        let last = self.last();
        let height = height.max(1) as isize;
        self.selected = match motion {
            Motion::Line(delta) => self.step(delta),
            Motion::HalfPage(delta) => self.step(delta * (height / 2).max(1)),
            Motion::Page(delta) => self.step(delta * height),
            Motion::Top => 0,
            Motion::Bottom => last,
        };
        self.reveal(height as usize);
    }

    pub fn scroll(&mut self, delta: isize, height: usize) {
        let ceiling = self.last().saturating_sub(height.max(1) - 1);
        self.top = self.top.saturating_add_signed(delta).min(ceiling);
        self.snap(height);
    }

    pub fn pick(&mut self, row: usize) -> Option<&Entry> {
        let index = self.top.checked_add(row)?;
        if index > self.last() {
            return None;
        }
        self.selected = index;
        self.entries.get(index)
    }

    fn step(&self, delta: isize) -> usize {
        self.selected.saturating_add_signed(delta).min(self.last())
    }

    fn last(&self) -> usize {
        self.entries.len().saturating_sub(1)
    }

    fn reveal(&mut self, height: usize) {
        let height = height.max(1);
        self.top = self.top.min(self.selected);
        self.top = self.top.max(self.selected.saturating_sub(height - 1));
    }

    fn snap(&mut self, height: usize) {
        let height = height.max(1);
        self.selected = self.selected.clamp(self.top, (self.top + height - 1).min(self.last()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use crate::document::Document;
    use crate::markdown::ast;
    use crate::markdown::links::Links;
    use crate::render::layout::{self, Ctx};
    use crate::theme::Theme;

    fn rendered(source: &str) -> (Vec<SourceBlock>, Vec<RenderedLine>) {
        let blocks = ast::parse(source);
        let theme = Theme::default();
        let links = Links::new(Document::new(None, PathBuf::new(), source.to_string()), None);
        let lines = layout::render(&blocks, &Ctx::new(&theme, &links), 40);
        (blocks, lines)
    }

    fn outline_of(source: &str) -> Vec<Entry> {
        let (blocks, lines) = rendered(source);
        build(&blocks, &lines)
    }

    fn listing(levels: &[u8]) -> Outline {
        let entries = levels
            .iter()
            .enumerate()
            .map(|(index, level)| Entry { level: *level, text: format!("h{index}"), line: index * 3 })
            .collect();
        Outline { entries, selected: 0, top: 0 }
    }

    #[test]
    fn every_heading_is_found_with_its_level_and_its_rendered_line() {
        let entries = outline_of("# One\n\ntext\n\n## Two\n\n### Three\n");
        assert_eq!(entries.iter().map(|entry| entry.level).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(entries.iter().map(|entry| entry.text.as_str()).collect::<Vec<_>>(), vec!["One", "Two", "Three"]);
        assert!(entries[0].line < entries[1].line && entries[1].line < entries[2].line);
    }

    #[test]
    fn a_heading_line_is_the_line_the_reader_would_land_on() {
        let (blocks, lines) = rendered("# One\n\ntext\n\n## Two\n");

        for entry in build(&blocks, &lines) {
            assert!(lines[entry.line].text().contains(&entry.text), "{entry:?}");
        }
    }

    #[test]
    fn a_headings_emphasis_is_flattened_to_its_text() {
        assert_eq!(outline_of("# *One* and `two`\n")[0].text, "One and two");
    }

    #[test]
    fn headings_nested_in_a_quote_or_a_list_are_found_too() {
        let entries = outline_of("# Top\n\n> ## Quoted\n\n- ### Listed\n");
        assert_eq!(entries.iter().map(|entry| entry.text.as_str()).collect::<Vec<_>>(), vec!["Top", "Quoted", "Listed"]);
    }

    #[test]
    fn a_document_with_no_headings_has_an_empty_outline() {
        assert!(outline_of("just a paragraph\n").is_empty());
        assert!(Outline::default().is_empty());
    }

    #[test]
    fn repeated_headings_keep_one_entry_each() {
        let entries = outline_of("# Same\n\n# Same\n");
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].line, entries[1].line);
    }

    #[test]
    fn the_selection_stops_at_both_ends() {
        let mut outline = listing(&[1, 2, 2, 1]);

        outline.move_by(Motion::Line(-1), 4);
        assert_eq!(outline.selected, 0);

        outline.move_by(Motion::Bottom, 4);
        assert_eq!(outline.selected, 3);

        outline.move_by(Motion::Line(1), 4);
        assert_eq!(outline.selected, 3);

        outline.move_by(Motion::Top, 4);
        assert_eq!(outline.selected, 0);
    }

    #[test]
    fn a_page_is_the_height_of_the_popup() {
        let mut outline = listing(&[1; 20]);

        outline.move_by(Motion::Page(1), 5);
        assert_eq!(outline.selected, 5);

        outline.move_by(Motion::HalfPage(1), 5);
        assert_eq!(outline.selected, 7);
    }

    #[test]
    fn the_view_follows_the_selection() {
        let mut outline = listing(&[1; 20]);

        outline.move_by(Motion::Line(6), 4);
        assert_eq!(outline.selected, 6);
        assert_eq!(outline.top, 3, "the selection is the last visible row");

        outline.move_by(Motion::Top, 4);
        assert_eq!(outline.top, 0);
    }

    #[test]
    fn the_selection_follows_the_view_when_it_is_scrolled() {
        let mut outline = listing(&[1; 20]);

        outline.scroll(6, 4);
        assert_eq!(outline.top, 6);
        assert_eq!(outline.selected, 6, "snapped onto the first visible row");

        outline.scroll(-2, 4);
        assert_eq!(outline.top, 4);
        assert_eq!(outline.selected, 6, "still visible, so left alone");
    }

    #[test]
    fn scrolling_stops_with_the_last_entry_in_view() {
        let mut outline = listing(&[1; 8]);

        outline.scroll(100, 4);
        assert_eq!(outline.top, 4, "the last entry is on the last row");
        assert_eq!(outline.selected, 4, "snapped onto the nearest visible row");

        outline.scroll(-100, 4);
        assert_eq!(outline.top, 0);
    }

    #[test]
    fn opening_selects_the_section_the_reader_is_in() {
        let mut outline = listing(&[1, 1, 1]);

        outline.open_at(0, 4);
        assert_eq!(outline.selected, 0);

        outline.open_at(4, 4);
        assert_eq!(outline.selected, 1, "line 4 is past the heading on line 3");

        outline.open_at(100, 4);
        assert_eq!(outline.selected, 2);
    }

    #[test]
    fn opening_above_the_first_heading_selects_it_rather_than_nothing() {
        let mut outline = Outline { entries: vec![Entry { level: 1, text: "h".into(), line: 5 }], selected: 0, top: 0 };
        outline.open_at(0, 4);
        assert_eq!(outline.selected, 0);
    }

    #[test]
    fn opening_scrolls_the_view_to_the_selection() {
        let mut outline = listing(&[1; 20]);
        outline.open_at(100, 4);
        assert_eq!(outline.selected, 19);
        assert_eq!(outline.top, 16);
    }

    #[test]
    fn picking_a_row_selects_what_is_on_it_and_nothing_past_the_end() {
        let mut outline = listing(&[1; 6]);
        outline.scroll(2, 4);

        assert_eq!(outline.pick(1).map(|entry| entry.line), Some(9));
        assert_eq!(outline.selected, 3);

        assert_eq!(outline.pick(4), None, "row 6 is past the last entry");
        assert_eq!(outline.selected, 3, "and leaves the selection alone");
    }
}
