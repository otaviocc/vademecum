//! The document's frontmatter, and the cursor moving over it.

use unicode_width::UnicodeWidthStr;

use crate::document::{Document, Property};
use crate::ui::input::Motion;
use crate::ui::listing;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Properties {
    pub rows: Vec<Property>,
    pub selected: usize,
    pub top: usize,
}

impl Properties {
    pub fn of(document: &Document) -> Self {
        Self { rows: document.properties.clone(), selected: 0, top: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn selected(&self) -> Option<&Property> {
        self.rows.get(self.selected)
    }

    pub fn key_width(&self) -> usize {
        self.rows.iter().map(|row| row.key.width()).max().unwrap_or(0)
    }

    pub fn move_by(&mut self, motion: Motion, height: usize) {
        self.selected = listing::target(motion, self.selected, self.last(), height);
        self.top = listing::revealed(self.top, self.selected, height);
    }

    pub fn scroll(&mut self, delta: isize, height: usize) {
        self.top = listing::scrolled(self.top, delta, self.last(), height);
        self.selected = listing::snapped(self.selected, self.top, self.last(), height);
    }

    pub fn pick(&mut self, row: usize) -> Option<&Property> {
        self.selected = listing::picked(self.top, row, self.last())?;
        self.rows.get(self.selected)
    }

    fn last(&self) -> usize {
        self.rows.len().saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(count: usize) -> Properties {
        let rows = (1..=count).map(|n| Property { key: format!("key{n}"), value: format!("value {n}") }).collect();
        Properties { rows, selected: 0, top: 0 }
    }

    #[test]
    fn a_document_without_frontmatter_has_an_empty_sheet() {
        let document = Document::new(None, std::path::PathBuf::new(), String::from("# Heading\n"));
        assert!(Properties::of(&document).is_empty());
    }

    #[test]
    fn a_sheet_reads_the_documents_properties_in_order() {
        let document = Document::new(None, std::path::PathBuf::new(), String::from("---\na: 1\nb: 2\n---\nbody\n"));
        let sheet = Properties::of(&document);
        assert_eq!(sheet.rows.len(), 2);
        assert_eq!(sheet.selected().map(|row| row.key.as_str()), Some("a"));
    }

    #[test]
    fn the_key_column_is_the_widest_key() {
        assert_eq!(sheet(3).key_width(), 4, "key1");
        assert_eq!(Properties::default().key_width(), 0);
    }

    #[test]
    fn moving_walks_the_rows_and_stops_at_the_ends() {
        let mut sheet = sheet(5);
        sheet.move_by(Motion::Line(2), 10);
        assert_eq!(sheet.selected, 2);
        sheet.move_by(Motion::Bottom, 10);
        assert_eq!(sheet.selected, 4);
        sheet.move_by(Motion::Line(1), 10);
        assert_eq!(sheet.selected, 4, "the last row is the end");
        sheet.move_by(Motion::Top, 10);
        assert_eq!(sheet.selected, 0);
    }

    #[test]
    fn a_sheet_longer_than_its_box_scrolls_to_keep_the_selection_visible() {
        let mut sheet = sheet(20);
        sheet.move_by(Motion::Bottom, 5);
        assert_eq!(sheet.selected, 19);
        assert_eq!(sheet.top, 15, "the box followed the selection");
    }

    #[test]
    fn the_wheel_scrolls_the_box_and_pulls_the_selection_along() {
        let mut sheet = sheet(20);
        sheet.scroll(6, 5);
        assert_eq!(sheet.top, 6);
        assert_eq!(sheet.selected, 6, "snapped onto the first visible row");
    }

    #[test]
    fn a_click_past_the_last_row_picks_nothing_and_moves_nothing() {
        let mut sheet = sheet(3);
        assert_eq!(sheet.pick(1).map(|row| row.key.as_str()), Some("key2"));
        assert_eq!(sheet.pick(9), None);
        assert_eq!(sheet.selected, 1, "the selection is left where it was");
    }

    #[test]
    fn an_empty_sheet_answers_every_motion_without_panicking() {
        let mut sheet = Properties::default();
        sheet.move_by(Motion::Bottom, 5);
        sheet.scroll(3, 5);
        assert_eq!((sheet.selected, sheet.top), (0, 0));
        assert_eq!(sheet.selected(), None);
        assert_eq!(sheet.pick(0), None);
    }
}
