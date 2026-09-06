//! The pager's state, and the reducer that is the only way to change it.
//!
//! The document is parsed once. A resize re-runs layout — and only when the
//! *width* changed, because a taller or shorter terminal lays out identically —
//! so scrolling a very long file never touches the parser or the renderer.

use ratatui::layout::Size;

use crate::document::Document;
use crate::markdown::ast::{self, SourceBlock};
use crate::render::layout;
use crate::render::line::RenderedLine;
use crate::theme::Theme;
use crate::ui::input::{Action, Motion};

/// Rows the chrome takes: header, its rule, the statusbar's rule, statusbar.
const CHROME_ROWS: u16 = 4;
/// What the header and statusbar call a document that came from stdin.
const STDIN: &str = "stdin";

/// Everything the pager knows.
pub struct App {
    pub theme: Theme,
    /// Parsed once; a resize re-lays it out but never re-parses it.
    blocks: Vec<SourceBlock>,
    /// The document laid out at `width`. The view borrows this; it is never
    /// cloned per frame.
    pub lines: Vec<RenderedLine>,
    /// `--width`, when the reader pinned one; otherwise the width follows the
    /// terminal.
    width_override: Option<u16>,
    /// The width `lines` was laid out at.
    width: usize,
    area: Size,
    /// Index into `lines`: the reader's position.
    pub cursor: usize,
    /// Index into `lines`: the first visible row.
    pub top: usize,
    pub quit: bool,
    /// Header title: frontmatter title, else the file name, else `stdin`.
    pub title: String,
    /// Statusbar file: the file name, else `stdin`.
    pub file: String,
}

impl App {
    pub fn new(document: Document, theme: Theme, width_override: Option<u16>, area: Size) -> Self {
        let file = document
            .path
            .as_deref()
            .and_then(|path| path.file_name())
            .map_or_else(|| STDIN.to_string(), |name| name.to_string_lossy().into_owned());
        let title = document.title.clone().unwrap_or_else(|| file.clone());

        let blocks = ast::parse(&document.source);
        let width = layout::wrap_width(width_override, Some(area.width));
        let lines = layout::render(&blocks, &theme, width);

        Self { theme, blocks, lines, width_override, width, area, cursor: 0, top: 0, quit: false, title, file }
    }

    /// The reducer. Every state change in the pager comes through here.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Quit => self.quit = true,
            Action::Move(motion) => self.move_cursor(motion),
            Action::Scroll(delta) => self.scroll(delta),
            Action::Resize(area) => self.resize(area),
        }
        self.clamp();
    }

    /// Rows the document itself gets. Always at least one, so a terminal too
    /// short for the chrome still shows a line rather than dividing by zero.
    pub fn viewport_height(&self) -> usize {
        usize::from(self.area.height.saturating_sub(CHROME_ROWS)).max(1)
    }

    /// The cursor line's position through the document, so the first line reads
    /// 0% and the last 100%.
    pub fn percent(&self) -> usize {
        match self.lines.len() {
            0 | 1 => 100,
            len => self.cursor * 100 / (len - 1),
        }
    }

    fn move_cursor(&mut self, motion: Motion) {
        let height = self.viewport_height();
        let last = self.lines.len().saturating_sub(1);
        match motion {
            Motion::Line(delta) => self.cursor = offset(self.cursor, delta),
            // A page keeps one line of context; a half page is half the view.
            Motion::HalfPage(delta) => self.jump(delta * (height / 2).max(1) as isize),
            Motion::Page(delta) => self.jump(delta * height.saturating_sub(1).max(1) as isize),
            Motion::Top => {
                self.cursor = 0;
                self.top = 0;
            }
            Motion::Bottom => {
                self.cursor = last;
                self.top = last;
            }
        }
    }

    /// Move cursor and viewport together, which is what a page key does: the
    /// text under the cursor moves by exactly one page.
    fn jump(&mut self, delta: isize) {
        self.cursor = offset(self.cursor, delta);
        self.top = offset(self.top, delta);
    }

    /// The wheel moves the viewport; the cursor is pulled to the nearest
    /// visible line rather than travelling with it.
    fn scroll(&mut self, delta: isize) {
        let height = self.viewport_height();
        self.top = offset(self.top, delta);
        self.cursor = self.cursor.clamp(self.top, self.top + height - 1);
    }

    fn resize(&mut self, area: Size) {
        self.area = area;
        let width = layout::wrap_width(self.width_override, Some(area.width));
        if width != self.width {
            self.width = width;
            self.relayout();
        }
    }

    /// Lay the cached blocks out again, and put the reader back where they
    /// were: on the same source line, at the same height on the screen.
    fn relayout(&mut self) {
        let anchor = self.anchor();
        let row = self.cursor - self.top;

        self.lines = layout::render(&self.blocks, &self.theme, self.width);

        self.cursor = self
            .lines
            .iter()
            .position(|line| line.source_line == anchor)
            .or_else(|| self.lines.iter().position(|line| !line.is_blank() && line.source_line >= anchor))
            .unwrap_or(self.cursor);
        self.top = self.cursor.saturating_sub(row);
    }

    /// The source line the cursor is on. Blank lines belong to no source line
    /// in particular and carry 0, so the search walks back to the last line
    /// that does know where it came from.
    fn anchor(&self) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        self.lines[..=self.cursor.min(self.lines.len() - 1)]
            .iter()
            .rev()
            .find(|line| !line.is_blank())
            .map_or(0, |line| line.source_line)
    }

    /// The invariants, in one place: the cursor is on a line that exists, and
    /// the viewport contains it without scrolling past the end.
    fn clamp(&mut self) {
        let height = self.viewport_height();
        let last = self.lines.len().saturating_sub(1);

        self.cursor = self.cursor.min(last);
        self.top = self.top.min(last.saturating_sub(height.saturating_sub(1)));
        self.top = self.top.min(self.cursor);
        self.top = self.top.max(self.cursor.saturating_sub(height - 1));
    }
}

/// Signed movement over an index, saturating at both ends.
fn offset(index: usize, delta: isize) -> usize {
    if delta >= 0 { index.saturating_add(delta as usize) } else { index.saturating_sub(delta.unsigned_abs()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A document of `count` one-line paragraphs, which lay out one line each
    /// with a blank line between them.
    fn numbered(count: usize) -> String {
        (1..=count).map(|n| format!("line {n}\n\n")).collect()
    }

    fn app(source: &str, height: u16) -> App {
        let document = Document::new(Some(PathBuf::from("notes/x.md")), PathBuf::from("notes"), source.to_string());
        App::new(document, Theme::default(), Some(40), Size::new(60, height))
    }

    /// Ten rows of chrome-free viewport.
    fn paged() -> App {
        app(&numbered(40), 14)
    }

    #[test]
    fn the_viewport_is_the_terminal_less_the_chrome() {
        assert_eq!(paged().viewport_height(), 10);
        // A terminal with no room left still gets a line to draw on.
        assert_eq!(app(&numbered(3), 2).viewport_height(), 1);
    }

    #[test]
    fn the_cursor_stops_at_both_ends() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Line(-1)));
        assert_eq!(app.cursor, 0);

        app.apply(Action::Move(Motion::Bottom));
        let last = app.lines.len() - 1;
        assert_eq!(app.cursor, last);
        app.apply(Action::Move(Motion::Line(1)));
        assert_eq!(app.cursor, last);
    }

    #[test]
    fn the_viewport_follows_the_cursor_only_when_it_would_leave() {
        let mut app = paged();
        for _ in 0..9 {
            app.apply(Action::Move(Motion::Line(1)));
        }
        assert_eq!((app.cursor, app.top), (9, 0), "the cursor crosses the view before it scrolls");

        app.apply(Action::Move(Motion::Line(1)));
        assert_eq!((app.cursor, app.top), (10, 1));
    }

    #[test]
    fn a_page_keeps_one_line_of_context_and_a_half_page_is_half_the_view() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Page(1)));
        assert_eq!((app.cursor, app.top), (9, 9));

        let mut app = paged();
        app.apply(Action::Move(Motion::HalfPage(1)));
        assert_eq!((app.cursor, app.top), (5, 5));
    }

    #[test]
    fn the_bottom_shows_the_end_of_the_document_rather_than_a_blank_screen() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Bottom));
        let last = app.lines.len() - 1;
        assert_eq!(app.cursor, last);
        assert_eq!(app.top, last - (app.viewport_height() - 1));
    }

    #[test]
    fn the_wheel_moves_the_view_and_pulls_the_cursor_along_only_at_the_edge() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Line(1)));
        app.apply(Action::Scroll(3));
        assert_eq!(app.top, 3);
        assert_eq!(app.cursor, 3, "the cursor was above the new view and is pulled to its top");

        app.apply(Action::Scroll(-3));
        assert_eq!(app.top, 0);
        assert_eq!(app.cursor, 3, "still visible, so it stays where the reader left it");
    }

    #[test]
    fn a_narrower_terminal_keeps_the_cursor_on_the_same_source_line() {
        // Paragraphs long enough that a narrower width rewraps them.
        let source =
            (1..=20).map(|n| format!("paragraph {n} with enough words in it to wrap twice over\n\n")).collect::<String>();
        let document = Document::new(Some(PathBuf::from("x.md")), PathBuf::from("."), source.clone());
        let mut app = App::new(document, Theme::default(), None, Size::new(70, 14));

        app.apply(Action::Move(Motion::Page(1)));
        // Blank lines belong to no source line, and the anchor walks back off
        // them; park on real text so the assertion is about the rewrap.
        while app.lines[app.cursor].is_blank() {
            app.apply(Action::Move(Motion::Line(1)));
        }
        let before = app.lines[app.cursor].source_line;
        assert!(before > 1, "the test needs a cursor well into the document");

        app.apply(Action::Resize(Size::new(30, 14)));
        assert_eq!(app.lines[app.cursor].source_line, before);
    }

    #[test]
    fn a_taller_terminal_does_not_lay_the_document_out_again() {
        let mut app = paged();
        let width = app.width;
        let count = app.lines.len();

        app.apply(Action::Resize(Size::new(60, 30)));
        assert_eq!(app.width, width);
        assert_eq!(app.lines.len(), count);
        assert_eq!(app.viewport_height(), 26);
    }

    #[test]
    fn a_pinned_width_ignores_the_terminal() {
        let mut app = paged();
        app.apply(Action::Resize(Size::new(200, 14)));
        assert_eq!(app.width, 40);
    }

    #[test]
    fn the_percentage_runs_from_nought_to_a_hundred() {
        let mut app = paged();
        assert_eq!(app.percent(), 0);
        app.apply(Action::Move(Motion::Bottom));
        assert_eq!(app.percent(), 100);
    }

    #[test]
    fn a_one_line_document_is_read_in_full() {
        assert_eq!(app("hi\n", 14).percent(), 100);
    }

    #[test]
    fn the_title_falls_back_from_frontmatter_to_the_file_name_to_stdin() {
        assert_eq!(app("hi\n", 14).title, "x.md");
        assert_eq!(app("---\ntitle: Notes\n---\nhi\n", 14).title, "Notes");

        let piped = Document::new(None, PathBuf::from("."), "hi\n".to_string());
        let app = App::new(piped, Theme::default(), None, Size::new(60, 14));
        assert_eq!(app.title, STDIN);
        assert_eq!(app.file, STDIN);
    }

    #[test]
    fn quitting_is_the_only_thing_that_ends_the_loop() {
        let mut app = paged();
        assert!(!app.quit);
        app.apply(Action::Quit);
        assert!(app.quit);
    }
}
