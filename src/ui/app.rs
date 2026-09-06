//! The pager's state, and the reducer that is the only way to change it.
//!
//! The document is parsed once. A resize re-runs layout — and only when the
//! *width* changed, because a taller or shorter terminal lays out identically —
//! so scrolling a very long file never touches the parser or the renderer.

use std::path::Path;

use ratatui::layout::Size;

use crate::document::Document;
use crate::markdown::ast::{self, SourceBlock};
use crate::markdown::links::{LinkKind, Links, Target};
use crate::render::layout::{self, Ctx};
use crate::render::line::LinkRef;
use crate::render::line::RenderedLine;
use crate::theme::Theme;
use crate::ui::Options;
use crate::ui::input::{Action, Motion};
use crate::ui::search::{self, Search};

/// Rows the chrome takes: header, its rule, the statusbar's rule, statusbar.
const CHROME_ROWS: u16 = 4;
/// What the header and statusbar call a document that came from stdin.
const STDIN: &str = "stdin";

/// What keys mean right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Browse,
    Search,
    Help,
}

/// The statusbar's transient line. Both are cleared by the next key, which is
/// what makes them transient; an error outranks a notice, because it is the
/// answer to something the reader just asked for and did not get.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Idle,
    Notice(String),
    Error(String),
}

/// A place in the collection, as the history remembers it.
struct Entry {
    document: Document,
    top: usize,
    cursor: usize,
    focus: usize,
}

/// Where the reader is standing, in terms that survive a re-layout.
#[derive(Debug, Clone, Copy)]
struct Place {
    /// The source line under the cursor.
    anchor: usize,
    /// The screen row it sits on, signed: the wheel can put it off either end.
    row: isize,
}

/// Everything the pager knows.
pub struct App {
    pub theme: Theme,
    /// The open document and the vault its links resolve against. Layout needs
    /// both, because whether a link resolves is what decides how it is styled.
    pub links: Links,
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
    pub mode: Mode,
    pub status: Status,
    pub search: Search,
    /// The plain text of every line, which is what search scans. Built the
    /// first time a query is confirmed and dropped on a re-layout, so opening
    /// a document never pays for it.
    plain: Option<Vec<String>>,
    /// Header title: frontmatter title, else the file name, else `stdin`.
    pub title: String,
    /// Statusbar file: the file name, else `stdin`.
    pub file: String,
    /// Which of the cursor line's links `Enter` would follow. A line with one
    /// link focuses it without being asked, which is what 0 means here.
    pub focus: usize,
    /// Whether the error on the statusbar is the last reload's own. A reload
    /// that works disproves it; an error from anywhere else is not its to
    /// clear.
    reload_failed: bool,
    /// Where the reader has been, and where `l` would take them back to.
    back: Vec<Entry>,
    forward: Vec<Entry>,
}

impl App {
    pub fn new(document: Document, theme: Theme, options: &Options, area: Size) -> Self {
        let width_override = options.width;
        let (title, file) = names(&document);

        let blocks = ast::parse(&document.source);
        let links = Links::new(document, options.root.as_deref());
        let width = layout::wrap_width(width_override, Some(area.width));
        let lines = layout::render(&blocks, &Ctx::new(&theme, &links), width);

        Self {
            theme,
            links,
            blocks,
            lines,
            width_override,
            width,
            area,
            cursor: 0,
            top: 0,
            quit: false,
            mode: Mode::default(),
            status: Status::default(),
            search: Search::default(),
            plain: None,
            title,
            file,
            focus: 0,
            reload_failed: false,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    /// The reducer. Every state change in the pager comes through here.
    pub fn apply(&mut self, action: Action) {
        // A notice lasts until the next key. Neither a resize nor a reload is
        // a key: dragging the window, or someone else saving the file, should
        // not swallow what the pager just said — least of all an error the
        // reader has not read yet.
        if !matches!(action, Action::Resize(_) | Action::Reload) {
            self.status = Status::Idle;
            // The reader has acted, so whatever the last reload said is gone
            // from the screen and is no longer anyone's to clear.
            self.reload_failed = false;
        }

        // Neither a resize nor a reload is on this list: `rerender` puts the
        // cursor back at the height it was already at, off-screen included, and
        // revealing it would pull a scrolled-away viewport back.
        let moves_cursor = matches!(
            action,
            Action::Move(_) | Action::SearchConfirm | Action::SearchStep { .. } | Action::Follow | Action::History { .. }
        );

        match action {
            Action::Quit => self.quit = true,
            Action::Move(motion) => self.move_cursor(motion),
            Action::Scroll(delta) => self.scroll(delta),
            Action::Resize(area) => self.resize(area),
            Action::Reload => self.reload(),
            Action::ToggleHelp => self.mode = if self.mode == Mode::Help { Mode::Browse } else { Mode::Help },
            Action::Dismiss => self.search.clear(),
            Action::SearchStart => {
                self.mode = Mode::Search;
                self.search.input.clear();
            }
            Action::SearchType(character) => self.search.input.push(character),
            Action::SearchErase => {
                self.search.input.pop();
            }
            Action::SearchCancel => {
                self.mode = Mode::Browse;
                self.search.input.clear();
            }
            Action::SearchConfirm => self.confirm_search(),
            Action::SearchStep { forward } => self.step_search(forward),
            Action::Focus { forward } => self.cycle_focus(forward),
            Action::Follow => self.follow(),
            Action::OpenExternal => self.open_external(),
            Action::History { forward } => self.travel(forward),
        }

        self.bound();
        // Only an action that moves the cursor drags the viewport after it.
        // The wheel is the reason: it leaves the cursor where the reader put
        // it, off-screen if need be, and revealing it here would undo the
        // scroll on the same tick.
        if moves_cursor {
            self.reveal();
        }
    }

    /// The link `Enter` would follow: the focused one on the cursor line.
    fn focused(&self) -> Option<&LinkRef> {
        self.lines.get(self.cursor).and_then(|line| line.links.get(self.focus))
    }

    /// `Tab` / `Shift-Tab`. A line with one link needs neither: focus starts at
    /// the first one and there is nowhere else to go.
    fn cycle_focus(&mut self, forward: bool) {
        let count = self.lines.get(self.cursor).map_or(0, |line| line.links.len());
        if count == 0 {
            return;
        }
        self.focus = match forward {
            true => (self.focus + 1) % count,
            false => (self.focus + count - 1) % count,
        };
    }

    /// `Enter`. An External link is left to `o`, as the README pairs them.
    fn follow(&mut self) {
        let Some(kind) = self.focused().map(|link| link.kind.clone()) else { return };
        match self.links.resolve(&kind) {
            // Missing or ambiguous: the reader asked to go somewhere and did
            // not, so they are told why rather than left wondering.
            Err(error) => self.status = Status::Error(error.to_string()),
            Ok(Target::External) => {}
            // Only a jump that goes somewhere is history. A fragment naming
            // no heading would otherwise push a dead entry and, worse, clear
            // the way forward.
            Ok(Target::SameDocument) => {
                if let Some(line) = self.anchor_line(kind.fragment()) {
                    self.remember();
                    self.go_to(line);
                    // Nothing was re-laid out, but the reader has moved, and
                    // `n` resumes from where they are.
                    self.resume_search();
                }
            }
            Ok(Target::File(path)) => self.open(&path, kind.fragment()),
        }
    }

    /// `o`. Nothing to do on a Local or Wiki link: opening files in an editor
    /// is out of scope.
    fn open_external(&mut self) {
        let Some(LinkKind::External(url)) = self.focused().map(|link| link.kind.clone()) else { return };
        if let Err(error) = open::that_detached(url.as_str()) {
            self.status = Status::Error(format!("{url}: {error}"));
        }
    }

    /// Load another document and show it. A file that cannot be read is a
    /// statusbar error, never an exit: the reader is already inside the pager.
    fn open(&mut self, path: &Path, fragment: Option<&str>) {
        let document = match Document::load(path) {
            Ok(document) => document,
            Err(error) => {
                self.status = Status::Error(format!("{error:#}"));
                return;
            }
        };

        self.remember();
        self.show(document);
        self.jump_to(fragment);
        self.resume_search();
        self.status = Status::Notice(format!("Opened {}", self.file));
    }

    /// Push where the reader is onto the back stack. Going somewhere new is
    /// what makes the forward stack stale, so it is dropped here.
    fn remember(&mut self) {
        self.back.push(self.here());
        self.forward.clear();
    }

    fn here(&self) -> Entry {
        Entry { document: self.links.document.clone(), top: self.top, cursor: self.cursor, focus: self.focus }
    }

    /// `h` / `l`. The two stacks are symmetric: whichever one is being popped,
    /// the other one gets where the reader was standing.
    fn travel(&mut self, forward: bool) {
        let Some(entry) = (if forward { self.forward.pop() } else { self.back.pop() }) else { return };
        let here = self.here();
        if forward {
            self.back.push(here)
        } else {
            self.forward.push(here)
        }

        self.show(entry.document);
        self.cursor = entry.cursor;
        self.top = entry.top;
        self.focus = entry.focus;
        self.resume_search();
        self.status = Status::Notice(format!("Opened {}", self.file));
    }

    /// Make `document` the one on screen, at the top of it.
    fn show(&mut self, document: Document) {
        (self.title, self.file) = names(&document);
        self.blocks = ast::parse(&document.source);
        self.links.open(document);
        self.lines = layout::render(&self.blocks, &Ctx::new(&self.theme, &self.links), self.width);

        self.cursor = 0;
        self.top = 0;
        self.focus = 0;

        // The lines are another document's, so the cached text and the offsets
        // into it are stale. A standing query follows the reader across, but
        // only once the caller has placed them: `resume_search`.
        self.plain = None;
    }

    /// The line a `#fragment` names, if any line answers to it.
    fn anchor_line(&self, fragment: Option<&str>) -> Option<usize> {
        // Decoded first: an editor that writes `my%20note.md` writes
        // `#A%20Heading` beside it, and the raw form slugs to `a20heading`,
        // which matches no heading and silently opens the document at its top.
        let slug = ast::slug(&crate::markdown::links::decode_fragment(fragment?));
        self.lines.iter().position(|line| line.anchor.as_deref() == Some(slug.as_str()))
    }

    /// Put the heading a `#fragment` names at the top of the view. One that
    /// matches no heading leaves the reader where the document opened, which
    /// is the top of it.
    fn jump_to(&mut self, fragment: Option<&str>) {
        if let Some(line) = self.anchor_line(fragment) {
            self.go_to(line);
        }
    }

    fn go_to(&mut self, line: usize) {
        self.cursor = line;
        self.top = line;
    }

    /// Match a standing query against the lines as they are now. Called once
    /// the reader has been put where they belong, never before: the search
    /// resumes from the cursor, so doing it first would resume from the top of
    /// a document nobody is looking at.
    fn resume_search(&mut self) {
        if !self.search.query.is_empty() {
            self.rematch();
        }
    }

    /// `Enter` on a query: match once, over the whole document, and take the
    /// reader to the first hit at or after where they are.
    fn confirm_search(&mut self) {
        self.mode = Mode::Browse;
        let query = std::mem::take(&mut self.search.input);
        if query.is_empty() {
            // Confirming nothing is not a search. A stray `/` then `Enter`
            // leaves a standing query and its highlights where they were.
            return;
        }

        self.search.query = query;
        let found = self.rematch();
        self.go_to_match(found);
    }

    /// `n` / `N`.
    fn step_search(&mut self, forward: bool) {
        if self.search.query.is_empty() {
            self.status = Status::Notice(String::from("nothing to search for yet"));
            return;
        }
        let found = self.search.step(forward);
        self.go_to_match(found);
    }

    fn go_to_match(&mut self, found: Option<search::Match>) {
        match found {
            // `clamp` scrolls the viewport to it; that is its whole job.
            Some(found) => self.cursor = found.line,
            None => self.status = Status::Notice(format!("no matches for \"{}\"", self.search.query)),
        }
    }

    /// Match the standing query against the lines as they are now, and take
    /// the first hit at or after the cursor. Both the confirm and the re-layout
    /// need exactly this.
    fn rematch(&mut self) -> Option<search::Match> {
        // Built on first use, so opening a document never pays for it, and
        // borrowed field-wise so the query can be read alongside it.
        let plain = self.plain.get_or_insert_with(|| self.lines.iter().map(RenderedLine::text).collect());
        self.search.matches = search::find(plain, &self.search.query);
        self.search.current = None;
        self.search.seek_from(self.cursor)
    }

    /// The document on screen, when it came from a file. The event loop needs
    /// it to keep the watch pointed at what the reader is reading.
    pub fn path(&self) -> Option<&Path> {
        self.links.document.path.as_deref()
    }

    /// A failure the reducer cannot see, because it happened to the event loop
    /// rather than to the pager: the watch losing its footing when the reader
    /// navigates. Transient like any other error — the next key clears it.
    pub fn report(&mut self, error: &str) {
        self.status = Status::Error(error.to_string());
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
        // The reader has left the line the focus was on, so it is not their
        // link any more.
        self.focus = 0;
        let height = self.viewport_height();
        let last = self.lines.len().saturating_sub(1);

        // A motion key is a statement of position, so it starts from what the
        // reader can see: the wheel may have left the cursor off-screen, and
        // pressing `j` there means "carry on reading here", not "go back".
        self.cursor = self.cursor.clamp(self.top, self.top + height - 1).min(last);

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

    /// The wheel moves the viewport and nothing else. The cursor keeps its
    /// document line and may scroll off the screen, so scrolling away and back
    /// puts the reader exactly where they were — with the focused link intact.
    fn scroll(&mut self, delta: isize) {
        self.top = offset(self.top, delta);
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
        let place = self.place();
        self.rerender(place);
    }

    /// The file changed under the reader. Re-read it, re-parse it, and lay it
    /// out again at the place they were standing — the same promise a resize
    /// makes, for the same reason.
    ///
    /// It reuses neither `show` nor `remember`: the first is navigation and
    /// resets the reader to the top of the document, and a reload is not
    /// history — an entry would clear the way forward and leave `h` stepping
    /// onto a stale copy of the file already on screen.
    fn reload(&mut self) {
        // A document that came from a pipe has nothing to re-read.
        let Some(path) = self.links.document.path.clone() else { return };
        let document = match Document::load(&path) {
            // Deleted, or unreadable, or caught mid-rename. None of those is a
            // reason to take the text away from the reader: the error is said
            // and the document stays, until the next write reloads it.
            Err(error) => {
                self.status = Status::Error(format!("{error:#}"));
                self.reload_failed = true;
                return;
            }
            Ok(document) => document,
        };

        let place = self.place();
        (self.title, self.file) = names(&document);
        self.blocks = ast::parse(&document.source);
        self.links.open(document);
        self.rerender(place);
        // A notice, and only over a statusbar that is not already answering
        // the reader. An error is what they asked for and did not get, and
        // someone else saving the file is no reason to take it off the screen
        // before they have acted on it — the same ordering `Status` documents.
        // The one error this may overwrite is the last reload's own, which
        // having just read the file it has disproved.
        if self.reload_failed || !matches!(self.status, Status::Error(_)) {
            self.status = Status::Notice(format!("Reloaded {}", self.file));
        }
        self.reload_failed = false;
    }

    /// Where the reader is standing, in terms that survive a re-layout: the
    /// source line under the cursor, and the screen row it sits on.
    fn place(&self) -> Place {
        // The row is signed, because the wheel can leave the cursor above or
        // below the viewport and neither a rewrap nor a reload should quietly
        // pull it back into it.
        Place { anchor: self.anchor(), row: self.cursor as isize - self.top as isize }
    }

    /// Lay the blocks out at the current width and put the reader back at
    /// `place`. The blocks may be the ones that were already there — a resize —
    /// or freshly parsed from a file that changed under the reader.
    fn rerender(&mut self, place: Place) {
        self.lines = layout::render(&self.blocks, &Ctx::new(&self.theme, &self.links), self.width);

        self.cursor = self
            .lines
            .iter()
            .position(|line| line.source_line == place.anchor)
            .or_else(|| self.lines.iter().position(|line| !line.is_blank() && line.source_line >= place.anchor))
            .unwrap_or(self.cursor);
        self.top = offset(self.cursor, -place.row);
        // The line the cursor lands on is a different line, with its own links:
        // a narrower width can leave the old index past the end of them, and
        // `Enter` with nothing focused does nothing at all.
        self.focus = 0;

        // The lines are new, so the cached text and the offsets into it are
        // stale. A live query is matched again against the new layout.
        self.plain = None;
        self.resume_search();
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

    /// The invariants that hold after *every* action: the cursor is on a line
    /// that exists, and the viewport does not run past the end of the document.
    fn bound(&mut self) {
        let height = self.viewport_height();
        let last = self.lines.len().saturating_sub(1);

        self.cursor = self.cursor.min(last);
        self.top = self.top.min(last.saturating_sub(height.saturating_sub(1)));
    }

    /// Scroll the viewport the least it can to contain the cursor. Only for
    /// actions that moved the cursor; see `apply`.
    fn reveal(&mut self) {
        let height = self.viewport_height();

        self.top = self.top.min(self.cursor);
        self.top = self.top.max(self.cursor.saturating_sub(height - 1));
    }
}

/// A document's header title and statusbar name: the frontmatter title, else
/// the file name, else `stdin` for a document that came from a pipe.
fn names(document: &Document) -> (String, String) {
    let file = document
        .path
        .as_deref()
        .and_then(|path| path.file_name())
        .map_or_else(|| STDIN.to_string(), |name| name.to_string_lossy().into_owned());
    let title = document.title.clone().unwrap_or_else(|| file.clone());
    (title, file)
}

/// Signed movement over an index, saturating at both ends.
fn offset(index: usize, delta: isize) -> usize {
    if delta >= 0 { index.saturating_add(delta as usize) } else { index.saturating_sub(delta.unsigned_abs()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

    /// A document of `count` one-line paragraphs, which lay out one line each
    /// with a blank line between them.
    fn numbered(count: usize) -> String {
        (1..=count).map(|n| format!("line {n}\n\n")).collect()
    }

    fn app(source: &str, height: u16) -> App {
        let document = Document::new(Some(PathBuf::from("notes/x.md")), PathBuf::from("notes"), source.to_string());
        App::new(document, Theme::default(), &Options { width: Some(40), ..Options::default() }, Size::new(60, height))
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
    fn scrolling_away_and_back_puts_the_reader_exactly_where_they_were() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Line(1)));
        let cursor = app.cursor;

        // Three screenfuls out, well past anything the viewport can show.
        for _ in 0..10 {
            app.apply(Action::Scroll(3));
        }
        assert_eq!(app.top, 30);
        assert_eq!(app.cursor, cursor, "the wheel moves the viewport and nothing else");

        for _ in 0..10 {
            app.apply(Action::Scroll(-3));
        }
        assert_eq!(app.top, 0);
        assert_eq!(app.cursor, cursor, "so coming back restores the place, rather than guessing at it");
    }

    #[test]
    fn the_wheel_leaves_the_cursor_alone_at_both_ends_of_the_document() {
        let mut app = paged();
        let (top, cursor) = (app.top, app.cursor);
        for _ in 0..5 {
            app.apply(Action::Scroll(-3));
        }
        assert_eq!((app.top, app.cursor), (top, cursor), "the view is already at the head");

        app.apply(Action::Move(Motion::Bottom));
        // Park the reader at the top of the last screenful.
        app.cursor = app.top;
        let (top, cursor) = (app.top, app.cursor);
        for _ in 0..5 {
            app.apply(Action::Scroll(3));
        }
        assert_eq!(app.top, top, "the view is already at the foot");
        assert_eq!(app.cursor, cursor, "and the cursor was never the wheel's to move");
    }

    #[test]
    fn a_motion_key_starts_from_the_line_the_reader_can_see() {
        let mut app = paged();
        app.apply(Action::Scroll(30));
        assert_eq!((app.top, app.cursor), (30, 0), "the cursor is off the top of the screen");

        app.apply(Action::Move(Motion::Line(1)));
        assert_eq!(app.cursor, 31, "snapped to the first visible line, then moved");
        assert_eq!(app.top, 30, "and the viewport the reader chose is left alone");

        // The other way round: the cursor below the viewport, scrolled back up.
        app.apply(Action::Scroll(-30));
        assert_eq!((app.top, app.cursor), (0, 31));
        app.apply(Action::Move(Motion::Line(-1)));
        assert_eq!(app.cursor, 8, "snapped to the last visible line of a ten-row viewport, then moved");
        assert_eq!(app.top, 0);
    }

    /// A document on disk, so the reload path has a real file to re-read. The
    /// directory is returned with it: dropping it would take the file away.
    fn on_disk(source: &str) -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("note.md");
        std::fs::write(&path, source).expect("write");
        let document = Document::load(&path).expect("load");
        let app = App::new(document, Theme::default(), &Options { width: Some(40), ..Options::default() }, Size::new(60, 14));
        (dir, app)
    }

    fn rewrite(dir: &tempfile::TempDir, source: &str) {
        std::fs::write(dir.path().join("note.md"), source).expect("write");
    }

    #[test]
    fn a_reload_keeps_the_reader_on_the_line_they_were_reading() {
        let (dir, mut app) = on_disk(&numbered(40));
        app.apply(Action::Move(Motion::Page(1)));
        // Blank lines belong to no source line and the anchor walks back off
        // them; park on real text so the assertion is about the reload.
        while app.lines[app.cursor].is_blank() {
            app.apply(Action::Move(Motion::Line(1)));
        }
        let (source_line, row, lines) = (app.lines[app.cursor].source_line, app.cursor - app.top, app.lines.len());

        // Appended, not prepended: the anchor is a source line number, so
        // adding text above the reader legitimately moves them.
        rewrite(&dir, &format!("{}{}", numbered(40), numbered(10)));
        app.apply(Action::Reload);

        assert!(app.lines.len() > lines, "the file on disk was not re-read");
        assert_eq!(app.lines[app.cursor].source_line, source_line, "the reader lost the line they were on");
        assert_eq!(app.cursor - app.top, row, "and the height on the screen it was at");
    }

    /// The reason a reload goes through `apply` rather than being called
    /// directly: `bound` runs there, and without it a file that shrank leaves
    /// the cursor and the viewport pointing past the end of the document.
    #[test]
    fn a_reload_that_shortens_the_file_keeps_the_cursor_on_a_line_that_exists() {
        let (dir, mut app) = on_disk(&numbered(40));
        app.apply(Action::Move(Motion::Bottom));

        rewrite(&dir, "just the one paragraph now\n");
        app.apply(Action::Reload);

        assert!(app.cursor < app.lines.len(), "cursor {} is past the {} lines left", app.cursor, app.lines.len());
        assert!(app.top <= app.cursor, "the viewport starts after the cursor");
    }

    #[test]
    fn a_reload_says_so_and_a_file_that_has_gone_says_why() {
        let (dir, mut app) = on_disk(&numbered(10));
        rewrite(&dir, &numbered(12));
        app.apply(Action::Reload);
        assert_eq!(app.status, Status::Notice(String::from("Reloaded note.md")));

        let lines = app.lines.len();
        std::fs::remove_file(dir.path().join("note.md")).expect("remove");
        app.apply(Action::Reload);
        assert!(matches!(app.status, Status::Error(_)), "a file that has gone said nothing: {:?}", app.status);
        assert_eq!(app.lines.len(), lines, "the reader lost the document as well as the file");
    }

    /// A reload that works disproves the last one's complaint. Without this the
    /// statusbar goes on saying the file cannot be read while the reader is
    /// looking at its new contents, and every later save reloads in silence.
    #[test]
    fn a_reload_that_works_clears_the_failure_that_came_before_it() {
        let (dir, mut app) = on_disk(&numbered(10));
        std::fs::remove_file(dir.path().join("note.md")).expect("remove");
        app.apply(Action::Reload);
        assert!(matches!(app.status, Status::Error(_)), "the failure was not reported");

        rewrite(&dir, &numbered(12));
        app.apply(Action::Reload);
        assert_eq!(app.status, Status::Notice(String::from("Reloaded note.md")));
    }

    #[test]
    fn a_reload_does_not_wipe_an_error_the_reader_has_not_read() {
        let (dir, mut app) = on_disk("[[missing]] link\n");
        app.apply(Action::Follow);
        let Status::Error(error) = app.status.clone() else { panic!("expected an error, got {:?}", app.status) };

        rewrite(&dir, "[[missing]] link\n\nand more\n");
        app.apply(Action::Reload);
        assert_eq!(app.status, Status::Error(error), "someone else's save swallowed the reader's answer");
    }

    #[test]
    fn a_reload_matches_a_standing_query_against_the_text_that_is_there_now() {
        let (dir, mut app) = on_disk("needle once\n\nfiller\n");
        search_for(&mut app, "needle");
        let before = app.search.matches.len();

        rewrite(&dir, "needle once\n\nfiller\n\nneedle twice\n");
        app.apply(Action::Reload);

        assert!(app.search.matches.len() > before, "the new text was not searched");
        for found in &app.search.matches {
            let text = app.lines[found.line].text();
            assert_eq!(&text[found.start..found.end], "needle", "the offsets index the new lines");
        }
    }

    #[test]
    fn a_piped_document_has_nothing_to_reload() {
        let piped = Document::new(None, PathBuf::from("."), numbered(10));
        let mut app = App::new(piped, Theme::default(), &Options::default(), Size::new(60, 14));
        app.apply(Action::Reload);
        assert_eq!(app.status, Status::Idle, "there is no file behind a pipe to have failed");
    }

    #[test]
    fn the_open_document_names_the_file_the_watch_should_follow() {
        let mut app = vault();
        assert_eq!(app.path(), Some(Path::new("tests/fixtures/vault/index.md")));

        focus_link(&mut app, "note");
        app.apply(Action::Follow);
        assert_eq!(app.path(), Some(Path::new("tests/fixtures/vault/note.md")), "the watch has somewhere new to follow");
    }

    #[test]
    fn a_narrower_terminal_keeps_the_cursor_on_the_same_source_line() {
        // Paragraphs long enough that a narrower width rewraps them.
        let source =
            (1..=20).map(|n| format!("paragraph {n} with enough words in it to wrap twice over\n\n")).collect::<String>();
        let document = Document::new(Some(PathBuf::from("x.md")), PathBuf::from("."), source.clone());
        let mut app = App::new(document, Theme::default(), &Options { width: None, ..Options::default() }, Size::new(70, 14));

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
        let app = App::new(piped, Theme::default(), &Options { width: None, ..Options::default() }, Size::new(60, 14));
        assert_eq!(app.title, STDIN);
        assert_eq!(app.file, STDIN);
    }

    /// Type a query and confirm it.
    fn search_for(app: &mut App, query: &str) {
        app.apply(Action::SearchStart);
        for character in query.chars() {
            app.apply(Action::SearchType(character));
        }
        app.apply(Action::SearchConfirm);
    }

    #[test]
    fn typing_a_query_stays_in_search_mode_until_it_is_confirmed() {
        let mut app = paged();
        app.apply(Action::SearchStart);
        assert_eq!(app.mode, Mode::Search);

        app.apply(Action::SearchType('l'));
        app.apply(Action::SearchType('x'));
        app.apply(Action::SearchErase);
        assert_eq!(app.search.input, "l");
        assert_eq!(app.mode, Mode::Search);

        app.apply(Action::SearchConfirm);
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.search.query, "l");
    }

    #[test]
    fn confirming_takes_the_reader_to_the_first_match_at_or_after_the_cursor() {
        let mut app = paged();
        search_for(&mut app, "line 7");
        assert_eq!(app.lines[app.cursor].text().trim(), "line 7");
        assert!(app.top <= app.cursor && app.cursor < app.top + app.viewport_height());
    }

    #[test]
    fn a_search_wraps_when_there_is_nothing_below_the_cursor() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Bottom));
        // The only match is well above the cursor, so it can only be found by
        // wrapping.
        search_for(&mut app, "line 9");
        assert_eq!(app.lines[app.cursor].text().trim(), "line 9");
    }

    #[test]
    fn stepping_cycles_through_the_matches_both_ways() {
        let mut app = paged();
        search_for(&mut app, "line 1");
        let first = app.cursor;

        app.apply(Action::SearchStep { forward: true });
        assert_ne!(app.cursor, first);
        app.apply(Action::SearchStep { forward: false });
        assert_eq!(app.cursor, first);
    }

    #[test]
    fn a_fruitless_search_says_so_and_leaves_the_reader_alone() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Line(1)));
        let before = app.cursor;

        search_for(&mut app, "nothing here");
        assert_eq!(app.cursor, before);
        assert_eq!(app.status, Status::Notice(String::from("no matches for \"nothing here\"")));
    }

    #[test]
    fn a_notice_is_cleared_by_the_next_key_but_survives_a_resize() {
        let mut app = paged();
        search_for(&mut app, "nothing here");

        app.apply(Action::Resize(Size::new(60, 20)));
        assert!(matches!(app.status, Status::Notice(_)), "a drag of the window is not a keypress");

        app.apply(Action::Move(Motion::Line(1)));
        assert_eq!(app.status, Status::Idle);
    }

    #[test]
    fn escape_while_typing_keeps_the_query_that_was_already_standing() {
        let mut app = paged();
        search_for(&mut app, "line 1");
        let matches = app.search.matches.len();

        app.apply(Action::SearchStart);
        app.apply(Action::SearchType('z'));
        app.apply(Action::SearchCancel);

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.search.query, "line 1");
        assert_eq!(app.search.matches.len(), matches);
    }

    #[test]
    fn escape_while_browsing_drops_the_highlight_without_moving_the_reader() {
        let mut app = paged();
        search_for(&mut app, "line 7");
        let before = app.cursor;

        app.apply(Action::Dismiss);
        assert!(app.search.query.is_empty());
        assert!(app.search.matches.is_empty());
        assert_eq!(app.cursor, before);
    }

    #[test]
    fn a_rewrap_matches_the_query_against_the_new_layout() {
        let source = (1..=20).map(|n| format!("paragraph {n} with a needle in it somewhere\n\n")).collect::<String>();
        let document = Document::new(Some(PathBuf::from("x.md")), PathBuf::from("."), source);
        let mut app = App::new(document, Theme::default(), &Options { width: None, ..Options::default() }, Size::new(70, 14));
        search_for(&mut app, "needle");
        let before = app.search.matches.len();

        app.apply(Action::Resize(Size::new(30, 14)));
        assert_eq!(app.search.matches.len(), before, "every needle is still found");
        for found in &app.search.matches {
            let text = app.lines[found.line].text();
            assert_eq!(&text[found.start..found.end], "needle", "the offsets index the new lines");
        }
    }

    #[test]
    fn n_without_a_query_says_there_is_nothing_to_repeat() {
        let mut app = paged();
        app.apply(Action::SearchStep { forward: true });
        assert!(matches!(app.status, Status::Notice(_)));
    }

    #[test]
    fn the_help_overlay_toggles() {
        let mut app = paged();
        app.apply(Action::ToggleHelp);
        assert_eq!(app.mode, Mode::Help);

        app.apply(Action::ToggleHelp);
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn the_document_does_not_move_behind_the_overlay() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Page(1)));
        app.apply(Action::ToggleHelp);
        let (top, cursor) = (app.top, app.cursor);

        // `input` is what refuses the keys, so this is the reducer's half of
        // the promise: whatever the overlay lets through must not move anyone.
        for key in [KeyCode::Char('j'), KeyCode::Char('G'), KeyCode::Char('/')] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert_eq!(crate::ui::input::action(&event, app.mode), None, "{key:?}");
        }
        let wheel =
            Event::Mouse(MouseEvent { kind: MouseEventKind::ScrollDown, column: 0, row: 0, modifiers: KeyModifiers::NONE });
        assert_eq!(crate::ui::input::action(&wheel, app.mode), None);

        assert_eq!((app.top, app.cursor), (top, cursor));
    }

    #[test]
    fn confirming_an_empty_query_leaves_a_standing_one_alone() {
        let mut app = paged();
        search_for(&mut app, "line 1");
        let (query, matches, cursor) = (app.search.query.clone(), app.search.matches.len(), app.cursor);

        app.apply(Action::SearchStart);
        app.apply(Action::SearchConfirm);

        assert_eq!(app.search.query, query);
        assert_eq!(app.search.matches.len(), matches);
        assert_eq!(app.cursor, cursor);
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn quitting_is_the_only_thing_that_ends_the_loop() {
        let mut app = paged();
        assert!(!app.quit);
        app.apply(Action::Quit);
        assert!(app.quit);
    }

    /// The vault fixture, open at its index: the only document in the tree with
    /// one of every kind of link in it.
    fn vault() -> App {
        let document = Document::load(std::path::Path::new("tests/fixtures/vault/index.md")).expect("the fixture is there");
        App::new(document, Theme::default(), &Options { width: Some(78), ..Options::default() }, Size::new(80, 24))
    }

    /// A document of vademecum's own inside the vault, for the tests that need
    /// several links on one line. It never has to exist on disk: resolution
    /// reads its `base_dir`, which does.
    fn inside_vault(source: &str) -> App {
        let document = Document::new(
            Some(PathBuf::from("tests/fixtures/vault/scratch.md")),
            PathBuf::from("tests/fixtures/vault"),
            source.to_string(),
        );
        App::new(document, Theme::default(), &Options { width: Some(78), ..Options::default() }, Size::new(80, 24))
    }

    /// Park the cursor on the link written as `destination`, and focus it.
    fn focus_link(app: &mut App, destination: &str) {
        for index in 0..app.lines.len() {
            if let Some(focus) = app.lines[index].links.iter().position(|link| link.kind.destination() == destination) {
                app.cursor = index;
                app.focus = focus;
                return;
            }
        }
        panic!("no link to {destination:?} in the fixture");
    }

    /// The document on screen, which is the only way to tell two files of the
    /// same name apart.
    fn open_path(app: &App) -> PathBuf {
        app.links.document.path.clone().expect("the fixture came from a file")
    }

    #[test]
    fn a_line_with_one_link_focuses_it_without_being_asked() {
        let mut app = vault();
        focus_link(&mut app, "missing");
        assert_eq!(app.focus, 0);
        assert_eq!(app.focused().expect("a focused link").kind.destination(), "missing");
    }

    #[test]
    fn tab_cycles_the_focus_and_wraps_at_both_ends() {
        let mut app = inside_vault("[[note]], [[missing]] and [[dup]] on one line\n");
        let line = app.lines.iter().position(|line| line.links.len() > 1).expect("a line with several links");
        app.cursor = line;
        let count = app.lines[line].links.len();
        assert_eq!(count, 3);

        app.apply(Action::Focus { forward: true });
        assert_eq!(app.focus, 1);
        for _ in 1..count {
            app.apply(Action::Focus { forward: true });
        }
        assert_eq!(app.focus, 0, "forward wraps");

        app.apply(Action::Focus { forward: false });
        assert_eq!(app.focus, count - 1, "and so does backward");
    }

    #[test]
    fn moving_the_cursor_takes_the_focus_with_it() {
        let mut app = inside_vault("[[note]] and [[missing]] on one line\n\nanother paragraph\n");
        let line = app.lines.iter().position(|line| line.links.len() > 1).expect("a line with several links");
        app.cursor = line;
        app.apply(Action::Focus { forward: true });
        assert_eq!(app.focus, 1);

        app.apply(Action::Move(Motion::Line(1)));
        assert_eq!(app.focus, 0, "the link on the line the reader left is not the one they meant");
    }

    #[test]
    fn tab_on_a_line_with_no_links_does_nothing() {
        let mut app = vault();
        app.cursor = app.lines.iter().position(|line| line.links.is_empty()).expect("a line with no links");
        app.apply(Action::Focus { forward: true });
        assert_eq!(app.focus, 0);
    }

    #[test]
    fn enter_follows_a_wikilink_and_says_where_it_went() {
        let mut app = vault();
        focus_link(&mut app, "note");
        app.apply(Action::Follow);

        assert_eq!(app.file, "note.md");
        assert_eq!(open_path(&app), PathBuf::from("tests/fixtures/vault/note.md"));
        assert_eq!(app.status, Status::Notice(String::from("Opened note.md")));
        assert_eq!((app.cursor, app.top, app.focus), (0, 0, 0), "a new document opens at the top");
    }

    #[test]
    fn a_relative_link_is_followed_too() {
        let mut app = vault();
        focus_link(&mut app, "nested/note.md");
        app.apply(Action::Follow);
        assert_eq!(open_path(&app), PathBuf::from("tests/fixtures/vault/nested/note.md"));
    }

    #[test]
    fn history_goes_back_to_the_line_it_left_from_and_forward_again() {
        let mut app = vault();
        focus_link(&mut app, "note");
        let (cursor, focus) = (app.cursor, app.focus);

        app.apply(Action::Follow);
        app.apply(Action::History { forward: false });
        assert_eq!(app.file, "index.md");
        assert_eq!((app.cursor, app.focus), (cursor, focus), "back restores the position, focus included");

        app.apply(Action::History { forward: true });
        assert_eq!(open_path(&app), PathBuf::from("tests/fixtures/vault/note.md"));
    }

    #[test]
    fn going_somewhere_new_drops_the_way_forward() {
        let mut app = vault();
        focus_link(&mut app, "note");
        app.apply(Action::Follow);
        app.apply(Action::History { forward: false });

        focus_link(&mut app, "nested/note.md");
        app.apply(Action::Follow);
        app.apply(Action::History { forward: true });
        assert_eq!(
            open_path(&app),
            PathBuf::from("tests/fixtures/vault/nested/note.md"),
            "forward is the nested note now, not the one that was abandoned"
        );
    }

    #[test]
    fn history_at_either_end_is_not_an_error() {
        let mut app = vault();
        app.apply(Action::History { forward: false });
        app.apply(Action::History { forward: true });
        assert_eq!(app.file, "index.md");
        assert_eq!(app.status, Status::Idle);
    }

    #[test]
    fn a_broken_link_says_why_and_leaves_the_reader_where_they_are() {
        let mut app = vault();
        focus_link(&mut app, "missing");
        let (file, cursor) = (app.file.clone(), app.cursor);

        app.apply(Action::Follow);
        assert_eq!(app.status, Status::Error(String::from("missing: not found")));
        assert_eq!((app.file, app.cursor), (file, cursor));
    }

    #[test]
    fn an_ambiguous_link_names_the_candidates_it_could_not_choose_between() {
        let mut app = vault();
        focus_link(&mut app, "dup");
        app.apply(Action::Follow);

        let Status::Error(error) = &app.status else { panic!("expected an error, got {:?}", app.status) };
        assert!(error.starts_with("dup: ambiguous ("), "{error}");
        assert!(error.contains("a/dup.md") || error.contains("a\\dup.md"), "{error}");
        assert!(error.contains("b/dup.md") || error.contains("b\\dup.md"), "{error}");
    }

    #[test]
    fn a_fragment_puts_its_heading_at_the_top_of_the_view() {
        let mut app = vault();
        focus_link(&mut app, "note#A Heading");
        app.apply(Action::Follow);

        assert_eq!(app.file, "note.md");
        assert_eq!(app.lines[app.cursor].anchor.as_deref(), Some("a-heading"), "the cursor is on the heading it named");
    }

    #[test]
    fn a_fragment_on_this_page_moves_without_loading_anything() {
        let mut app = vault();
        focus_link(&mut app, "#index");
        app.cursor = app.lines.len() - 1;
        app.apply(Action::Follow);

        assert_eq!(app.file, "index.md");
        assert_eq!(app.lines[app.cursor].anchor.as_deref(), Some("index"));

        // And it is history, so the reader can undo the jump.
        app.apply(Action::History { forward: false });
        assert_eq!(app.cursor, app.lines.len() - 1);
    }

    /// The end-to-end half of the same thing: the link opens the right file and
    /// has to land on the right heading in it.
    #[test]
    fn a_percent_encoded_fragment_jumps_to_the_heading_it_names() {
        let mut app = vault();
        focus_link(&mut app, "nested/note.md#a-heading");
        app.apply(Action::Follow);
        let plain = app.cursor;

        let mut app = inside_vault("[h](nested/note.md#a%2Dheading)\n");
        focus_link(&mut app, "nested/note.md#a%2Dheading");
        app.apply(Action::Follow);
        assert_eq!(app.file, "note.md");
        assert_eq!(app.cursor, plain, "the encoded fragment landed somewhere else than the plain one");
    }

    #[test]
    fn a_fragment_that_names_no_heading_opens_the_document_at_the_top() {
        let mut app = vault();
        focus_link(&mut app, "nested/note.md#a-heading");
        app.apply(Action::Follow);
        let found = app.cursor;

        let mut app = vault();
        focus_link(&mut app, "nested/note.md");
        app.apply(Action::Follow);
        assert_ne!(found, app.cursor, "the fragment is what moved the cursor");
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn enter_and_o_each_leave_the_other_kind_of_link_alone() {
        let mut app = vault();
        focus_link(&mut app, "https://example.com");
        app.apply(Action::Follow);
        assert_eq!(app.status, Status::Idle, "Enter does not follow the web; `o` does");
        assert_eq!(app.file, "index.md");

        // And `o` on a wikilink does nothing rather than launching anything.
        focus_link(&mut app, "note");
        app.apply(Action::OpenExternal);
        assert_eq!(app.status, Status::Idle);
        assert_eq!(app.file, "index.md");
    }

    #[test]
    fn following_nothing_is_not_an_error() {
        let mut app = vault();
        app.cursor = app.lines.iter().position(|line| line.links.is_empty()).expect("a line with no links");
        app.apply(Action::Follow);
        app.apply(Action::OpenExternal);
        assert_eq!(app.status, Status::Idle);
    }

    #[test]
    fn a_standing_query_follows_the_reader_into_the_next_document() {
        let mut app = vault();
        search_for(&mut app, "heading");
        focus_link(&mut app, "note");
        app.apply(Action::Follow);

        assert_eq!(app.search.query, "heading");
        assert!(!app.search.matches.is_empty(), "the query is matched against the document now on screen");
        for found in &app.search.matches {
            assert!(found.line < app.lines.len(), "the offsets index the new document");
        }
    }

    #[test]
    fn a_wheel_notch_that_does_not_move_the_cursor_keeps_the_focus() {
        let mut app = inside_vault("[[note]] and [[missing]] on one line\n\n".to_string().repeat(30).as_str());
        let line = app.lines.iter().position(|line| line.links.len() > 1).expect("a line with several links");
        app.cursor = line;
        app.apply(Action::Focus { forward: true });
        assert_eq!(app.focus, 1);

        // The cursor is in the middle of the view, so a notch does not touch it.
        app.apply(Action::Move(Motion::HalfPage(1)));
        app.cursor = app.top + app.viewport_height() / 2;
        app.apply(Action::Focus { forward: true });
        let (cursor, focus) = (app.cursor, app.focus);

        app.apply(Action::Scroll(1));
        assert_eq!((app.cursor, app.focus), (cursor, focus), "the reader's link survives a scroll past it");
    }

    #[test]
    fn scrolling_the_focused_link_off_the_screen_and_back_keeps_it_focused() {
        let mut app = inside_vault("[[note]] and [[missing]]\n\n".to_string().repeat(30).as_str());
        let line = app.lines.iter().position(|line| line.links.len() > 1).expect("a line with several links");
        app.cursor = line;
        app.apply(Action::Focus { forward: true });
        assert_eq!(app.focus, 1);

        // Far enough that the cursor cannot stay on the screen.
        let away = app.viewport_height() as isize * 2;
        app.apply(Action::Scroll(away));
        assert_eq!((app.cursor, app.focus), (line, 1), "the wheel moved the view, not the reader");

        app.apply(Action::Scroll(-away));
        assert_eq!((app.cursor, app.focus), (line, 1), "and back again, with the chosen link still chosen");
    }

    #[test]
    fn a_fragment_that_names_no_heading_does_not_spend_a_history_entry() {
        let mut app = vault();
        focus_link(&mut app, "note");
        app.apply(Action::Follow);
        app.apply(Action::History { forward: false });
        assert_eq!(app.file, "index.md");

        // A bare fragment matching nothing: it must not clear the way forward.
        let mut app = inside_vault("[nowhere](#no-such-heading)\n");
        focus_link(&mut app, "#no-such-heading");
        let cursor = app.cursor;
        app.apply(Action::Follow);
        assert_eq!(app.cursor, cursor, "nothing to jump to");
        app.apply(Action::History { forward: false });
        assert_eq!(app.cursor, cursor, "and nothing was pushed to come back from");
    }

    #[test]
    fn a_bare_fragment_wikilink_is_the_document_already_open() {
        let mut app = inside_vault("# Scratch\n\n## A Heading\n\nGo to [[#A Heading]].\n");
        focus_link(&mut app, "#A Heading");
        app.apply(Action::Follow);

        assert_eq!(app.file, "scratch.md", "it never left");
        assert_eq!(app.lines[app.cursor].anchor.as_deref(), Some("a-heading"));
    }

    #[test]
    fn a_query_resumes_from_where_the_reader_lands_not_from_the_top() {
        // Two matches, one above the fragment and one below it. Landing on the
        // heading has to make the one below it current.
        let source = "# Top\n\nneedle above\n\n## A Heading\n\nneedle below\n";
        let mut app = inside_vault(&format!("{source}\nGo to [[#A Heading]].\n"));
        search_for(&mut app, "needle");
        focus_link(&mut app, "#A Heading");
        app.apply(Action::Follow);

        let current = app.search.current.and_then(|index| app.search.matches.get(index)).expect("a current match");
        assert!(current.line >= app.cursor, "the search resumed from the heading, not from the top");
    }

    #[test]
    fn a_rewrap_does_not_leave_the_focus_pointing_past_the_line_it_is_on() {
        // Wide enough that both links share a line, narrow enough afterwards
        // that they do not.
        let document = Document::new(
            Some(PathBuf::from("tests/fixtures/vault/scratch.md")),
            PathBuf::from("tests/fixtures/vault"),
            "[[note]] and then some words and then [[missing]]\n".to_string(),
        );
        let mut app = App::new(document, Theme::default(), &Options::default(), Size::new(80, 24));
        let line = app.lines.iter().position(|line| line.links.len() > 1).expect("both links on one line");
        app.cursor = line;
        app.apply(Action::Focus { forward: true });
        assert_eq!(app.focus, 1);

        app.apply(Action::Resize(Size::new(20, 24)));
        assert!(
            app.focus < app.lines[app.cursor].links.len().max(1),
            "focus {} is past the {} link(s) on the line",
            app.focus,
            app.lines[app.cursor].links.len()
        );
        assert!(app.focused().is_some(), "Enter still has something to follow");
    }
}
