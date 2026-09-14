//! The document's shape, scaled down to a strip beside the body.

use ratatui::style::Color;
use unicode_width::UnicodeWidthChar;

use crate::render::line::RenderedLine;

pub const CELLS: u16 = 12;
pub const GAP: u16 = 2;
pub const FLOOR: u16 = 40;

const SHADES: [char; 4] = ['░', '▒', '▓', '█'];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cell {
    pub shade: Option<char>,
    pub color: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scale {
    lines: usize,
    rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Map {
    pub scale: Scale,
    pub rows: Vec<Vec<Cell>>,
}

impl Scale {
    pub fn new(lines: usize, rows: usize) -> Self {
        Self { lines, rows: rows.min(lines) }
    }

    pub fn rows(self) -> usize {
        self.rows
    }

    pub fn row_of(self, line: usize) -> usize {
        match self.lines {
            0 => 0,
            lines => (line * self.rows / lines).min(self.rows.saturating_sub(1)),
        }
    }

    pub fn line_at(self, row: usize) -> usize {
        match self.rows {
            0 => 0,
            rows => (row * self.lines).div_ceil(rows).min(self.lines.saturating_sub(1)),
        }
    }

    fn bucket(self, row: usize) -> std::ops::Range<usize> {
        match row + 1 >= self.rows {
            true => self.line_at(row)..self.lines,
            false => self.line_at(row)..self.line_at(row + 1),
        }
    }
}

pub fn reserved(columns: u16, open: bool) -> u16 {
    if open && columns >= FLOOR { CELLS + GAP } else { 0 }
}

pub fn build(lines: &[RenderedLine], width: usize, cells: usize, rows: usize) -> Map {
    let scale = Scale::new(lines.len(), rows);
    if cells == 0 || scale.rows == 0 {
        return Map { scale, rows: Vec::new() };
    }

    let slice = width.div_ceil(cells).max(1);
    let rows = (0..scale.rows)
        .map(|row| {
            let bucket = scale.bucket(row);
            let capacity = bucket.len() * slice;
            let mut ink = vec![0usize; cells];
            let mut tally: Vec<Vec<(Color, usize)>> = vec![Vec::new(); cells];
            for line in &lines[bucket] {
                gather(line, slice, &mut ink, &mut tally);
            }
            ink.iter().zip(&tally).map(|(ink, tally)| cell(*ink, tally, capacity)).collect()
        })
        .collect();

    Map { scale, rows }
}

fn gather(line: &RenderedLine, slice: usize, ink: &mut [usize], tally: &mut [Vec<(Color, usize)>]) {
    let mut column = 0;
    for styled in &line.spans {
        for character in styled.text.chars() {
            let cells = UnicodeWidthChar::width(character).unwrap_or(0);
            if !character.is_whitespace() {
                for column in column..column + cells {
                    let Some(bucket) = ink.get_mut(column / slice) else { continue };
                    *bucket += 1;
                    if let Some(color) = styled.style.fg {
                        count(&mut tally[column / slice], color);
                    }
                }
            }
            column += cells;
        }
    }
}

fn count(tally: &mut Vec<(Color, usize)>, color: Color) {
    match tally.iter_mut().find(|(seen, _)| *seen == color) {
        Some((_, count)) => *count += 1,
        None => tally.push((color, 1)),
    }
}

fn cell(ink: usize, tally: &[(Color, usize)], capacity: usize) -> Cell {
    if ink == 0 || capacity == 0 {
        return Cell::default();
    }
    let step = (ink * SHADES.len()).div_ceil(capacity).clamp(1, SHADES.len());
    let color = tally.iter().fold(None, |best: Option<(Color, usize)>, (color, count)| match best {
        Some((_, most)) if most >= *count => best,
        _ => Some((*color, *count)),
    });
    Cell { shade: Some(SHADES[step - 1]), color: color.map(|(color, _)| color) }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Style;

    use super::*;
    use crate::render::line::StyledSpan;

    fn line(text: &str) -> RenderedLine {
        let mut line = RenderedLine::default();
        line.push(StyledSpan::new(text, Style::default().fg(Color::Red)));
        line
    }

    fn body(count: usize) -> Vec<RenderedLine> {
        (0..count).map(|_| line("full")).collect()
    }

    #[test]
    fn a_closed_or_cramped_strip_reserves_nothing() {
        assert_eq!(reserved(100, false), 0);
        assert_eq!(reserved(FLOOR - 1, true), 0);
        assert_eq!(reserved(0, true), 0);
        assert_eq!(reserved(FLOOR, true), CELLS + GAP);
    }

    #[test]
    fn a_document_shorter_than_the_strip_maps_one_line_to_one_row() {
        let scale = Scale::new(10, 40);
        assert_eq!(scale.rows(), 10, "a short document does not stretch over the whole strip");
        for line in 0..10 {
            assert_eq!(scale.row_of(line), line);
            assert_eq!(scale.line_at(line), line);
        }
    }

    #[test]
    fn a_longer_document_fills_every_row_of_the_strip() {
        for lines in [17, 33, 39, 100, 997] {
            for rows in [1, 12, 16, 40] {
                let scale = Scale::new(lines, rows);
                assert_eq!(scale.rows(), rows.min(lines), "{lines} lines over {rows} rows underfilled");
                assert_eq!(scale.line_at(0), 0, "the strip does not start at the first line");
                assert_eq!(scale.row_of(lines - 1), scale.rows() - 1, "the last line is not on the last row");
            }
        }
    }

    #[test]
    fn every_row_round_trips_back_to_itself() {
        for lines in [17, 39, 100, 997] {
            let scale = Scale::new(lines, 33);
            for row in 0..scale.rows() {
                assert_eq!(scale.row_of(scale.line_at(row)), row, "{lines} lines, row {row}");
            }
        }
    }

    #[test]
    fn the_buckets_cover_every_line_exactly_once() {
        for lines in [7, 17, 39, 100, 997] {
            let scale = Scale::new(lines, 16);
            let mut next = 0;
            for row in 0..scale.rows() {
                let bucket = scale.bucket(row);
                assert_eq!(bucket.start, next, "{lines} lines: row {row} does not carry on from the last");
                assert!(!bucket.is_empty(), "{lines} lines: row {row} covers nothing");
                next = bucket.end;
            }
            assert_eq!(next, lines, "{lines} lines: the last bucket stops short");
        }
    }

    #[test]
    fn the_last_row_is_shaded_against_the_lines_it_actually_holds() {
        for count in 12..=17 {
            let map = build(&vec![line("xxxx"); count], 4, 1, 4);
            let shades: Vec<Option<char>> = map.rows.iter().map(|row| row[0].shade).collect();
            assert_eq!(shades, vec![Some('█'); 4], "{count} solid lines did not shade evenly");
        }
    }

    #[test]
    fn a_degenerate_strip_or_document_builds_an_empty_map_rather_than_panicking() {
        assert!(build(&body(10), 80, 0, 20).rows.is_empty(), "no cells");
        assert!(build(&body(10), 80, 12, 0).rows.is_empty(), "no rows");
        assert!(build(&[], 80, 12, 20).rows.is_empty(), "no document");
        assert!(build(&body(10), 0, 12, 20).rows.len() == 10, "a zero width still draws a row per line");
        let empty = Scale::new(0, 20);
        assert_eq!((empty.rows(), empty.row_of(5), empty.line_at(5)), (0, 0, 0), "the scale never divides by zero");
    }

    #[test]
    fn a_blank_line_leaves_its_row_unshaded() {
        let map = build(&[RenderedLine::blank()], 80, 12, 20);
        assert_eq!(map.rows.len(), 1);
        assert!(map.rows[0].iter().all(|cell| cell.shade.is_none()), "{:?}", map.rows[0]);
    }

    #[test]
    fn a_line_of_solid_text_shades_the_cells_it_covers_and_no_others() {
        let map = build(&[line(&"x".repeat(12))], 24, 12, 20);
        let shaded: Vec<Option<char>> = map.rows[0].iter().map(|cell| cell.shade).collect();
        assert_eq!(shaded[..6], [Some('█'); 6], "a two-column slice fully covered");
        assert_eq!(shaded[6..], [None; 6], "past the end of the text");
    }

    #[test]
    fn a_sparser_bucket_shades_lighter_than_a_denser_one() {
        let sparse = build(&[line("x"), RenderedLine::blank(), RenderedLine::blank(), RenderedLine::blank()], 4, 1, 1);
        let dense = build(&[line("xxxx"), line("xxxx"), line("xxxx"), line("xxxx")], 4, 1, 1);
        assert_eq!(sparse.rows[0][0].shade, Some('░'));
        assert_eq!(dense.rows[0][0].shade, Some('█'));
    }

    #[test]
    fn a_cell_takes_the_colour_that_covers_most_of_it() {
        let mut mixed = RenderedLine::default();
        mixed.push(StyledSpan::new("aaa", Style::default().fg(Color::Blue)));
        mixed.push(StyledSpan::new("b", Style::default().fg(Color::Red)));

        let map = build(&[mixed], 4, 1, 1);
        assert_eq!(map.rows[0][0].color, Some(Color::Blue));
    }

    #[test]
    fn an_uncoloured_span_leaves_the_cell_to_the_theme() {
        let mut plain = RenderedLine::default();
        plain.push(StyledSpan::new("text", Style::default()));

        let map = build(&[plain], 4, 1, 1);
        assert_eq!(map.rows[0][0].shade, Some('█'));
        assert_eq!(map.rows[0][0].color, None);
    }

    #[test]
    fn a_wide_glyph_inks_both_of_the_columns_it_sits_in() {
        let mut wide = RenderedLine::default();
        wide.push(StyledSpan::new("日", Style::default().fg(Color::Red)));

        let map = build(&[wide], 2, 2, 1);
        assert_eq!(map.rows[0][0].shade, Some('█'));
        assert_eq!(map.rows[0][1].shade, Some('█'), "the second half of the glyph inks too");
    }

    #[test]
    fn the_strip_never_holds_more_rows_than_it_was_given() {
        let map = build(&body(1000), 80, 12, 24);
        assert!(map.rows.len() <= 24, "{} rows", map.rows.len());
        assert_eq!(map.rows[0].len(), 12);
    }
}
