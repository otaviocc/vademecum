//! The pager's state, and the reducer that is the only way to change it.

use std::ops::Range;
use std::path::Path;

use ratatui::layout::{Rect, Size};

use crate::document::Document;
use crate::markdown::ast::{self, SourceBlock};
use crate::markdown::links::{LinkKind, Links, Target};
use crate::render::layout::{self, Ctx};
use crate::render::line::LinkRef;
use crate::render::line::RenderedLine;
use crate::theme::Theme;
use crate::ui::Options;
use crate::ui::input::{Action, Motion};
use crate::ui::outline::{self, Outline};
use crate::ui::properties::Properties;
use crate::ui::search::{self, Search};
use crate::ui::view;

pub(crate) const CHROME_ROWS: u16 = 4;
pub(crate) const CONTENT_TOP: u16 = 2;
const STDIN: &str = "stdin";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Browse,
    Search,
    Toc,
    Properties,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Idle,
    Notice(String),
    Error(String),
}

struct Entry {
    document: Document,
    top: usize,
    cursor: usize,
    focus: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Point {
    line: usize,
    byte: usize,
    next: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Selection {
    anchor: Point,
    head: Point,
    dragged: bool,
}

impl Selection {
    fn span(&self) -> (Point, Point) {
        match self.anchor <= self.head {
            true => (self.anchor, self.head),
            false => (self.head, self.anchor),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Place {
    anchor: usize,
    row: isize,
}

pub struct App {
    pub theme: Theme,
    pub links: Links,
    blocks: Vec<SourceBlock>,
    pub lines: Vec<RenderedLine>,
    width_override: Option<u16>,
    width: usize,
    area: Size,
    pub cursor: usize,
    pub top: usize,
    pub quit: bool,
    pub mode: Mode,
    pub status: Status,
    pub search: Search,
    pub outline: Outline,
    pub properties: Properties,
    plain: Option<Vec<String>>,
    pub title: String,
    pub file: String,
    pub focus: usize,
    reload_failed: bool,
    selection: Option<Selection>,
    copied: Option<String>,
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
        let outline = Outline { entries: outline::build(&blocks, &lines), selected: 0, top: 0 };
        let properties = Properties::of(&links.document);
        let status = complaint().unwrap_or_default();

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
            status,
            search: Search::default(),
            outline,
            properties,
            plain: None,
            title,
            file,
            focus: 0,
            reload_failed: false,
            selection: None,
            copied: None,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    pub fn apply(&mut self, action: Action) {
        if !matches!(action, Action::Resize(_) | Action::Reload) {
            self.status = Status::Idle;
            self.reload_failed = false;
        }

        if !matches!(action, Action::SelectStart { .. } | Action::SelectExtend { .. } | Action::SelectEnd { .. }) {
            self.selection = None;
        }

        let moves_cursor = matches!(
            action,
            Action::Move(_)
                | Action::SearchConfirm
                | Action::SearchStep { .. }
                | Action::Follow
                | Action::History { .. }
                | Action::TocSelect
                | Action::TocClick { .. }
        );

        match action {
            Action::Quit => self.quit = true,
            Action::Move(motion) => self.move_cursor(motion),
            Action::Scroll(delta) => self.scroll(delta),
            Action::SelectStart { column, row } => self.select_start(column, row),
            Action::SelectExtend { column, row } => self.select_extend(column, row),
            Action::SelectEnd { column, row } => self.select_end(column, row),
            Action::Resize(area) => self.resize(area),
            Action::Reload => self.reload(),
            Action::ToggleHelp => self.mode = if self.mode == Mode::Help { Mode::Browse } else { Mode::Help },
            Action::ToggleToc => self.toggle_toc(),
            Action::ToggleProperties => self.toggle_properties(),
            Action::PropertiesMove(motion) => self.properties.move_by(motion, self.properties_height()),
            Action::PropertiesScroll(delta) => self.properties.scroll(delta, self.properties_height()),
            Action::PropertiesClick { row } => self.pick_property(row),
            Action::PropertiesYank => self.yank_property(),
            Action::TocMove(motion) => self.outline.move_by(motion, self.toc_height()),
            Action::TocScroll(delta) => self.outline.scroll(delta, self.toc_height()),
            Action::TocSelect => self.pick_heading(None),
            Action::TocClick { row } => self.pick_heading(Some(row)),
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
            Action::Yank => self.yank(),
            Action::YankLink => self.yank_link(),
        }

        self.bound();
        match action {
            Action::Scroll(_) => self.snap(),
            Action::Resize(_) => self.reveal(),
            _ if moves_cursor => self.reveal(),
            _ => {}
        }
    }

    fn focused(&self) -> Option<&LinkRef> {
        self.lines.get(self.cursor).and_then(|line| line.links.get(self.focus))
    }

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

    fn follow(&mut self) {
        let Some(kind) = self.focused().map(|link| link.kind.clone()) else { return };
        match self.links.resolve(&kind) {
            Err(error) => self.status = Status::Error(error.to_string()),
            Ok(Target::External) => {}
            Ok(Target::SameDocument) => {
                if let Some(line) = self.anchor_line(kind.fragment()) {
                    self.remember();
                    self.go_to(line);
                    self.resume_search();
                }
            }
            Ok(Target::File(path)) => self.open(&path, kind.fragment()),
        }
    }

    fn open_external(&mut self) {
        let Some(LinkKind::External(url)) = self.focused().map(|link| link.kind.clone()) else { return };
        if let Err(error) = open::that_detached(url.as_str()) {
            self.status = Status::Error(format!("{url}: {error}"));
        }
    }

    fn yank(&mut self) {
        let Some(line) = self.lines.get(self.cursor) else { return };
        let text = line.text();
        let text = text[line.byte_at(line.inset)..].trim_end().to_string();
        self.copy(text, String::from("Copied the line"));
    }

    fn yank_link(&mut self) {
        let Some(kind) = self.focused().map(|link| link.kind.clone()) else {
            self.status = Status::Notice(String::from("no link on this line"));
            return;
        };
        let destination = kind.destination();
        let notice = format!("Copied {destination}");
        self.copy(destination, notice);
    }

    fn copy(&mut self, text: String, notice: String) {
        if text.is_empty() {
            return;
        }
        self.copied = Some(text);
        self.status = Status::Notice(notice);
    }

    pub fn take_copy(&mut self) -> Option<String> {
        self.copied.take()
    }

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
        self.opened();
    }

    fn remember(&mut self) {
        self.back.push(self.here());
        self.forward.clear();
    }

    fn here(&self) -> Entry {
        Entry { document: self.links.document.clone(), top: self.top, cursor: self.cursor, focus: self.focus }
    }

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
        self.opened();
    }

    fn opened(&mut self) {
        if !matches!(self.status, Status::Error(_)) {
            self.status = Status::Notice(format!("Opened {}", self.file));
        }
    }

    fn show(&mut self, document: Document) {
        (self.title, self.file) = names(&document);
        self.blocks = ast::parse(&document.source);
        self.links.open(document);
        self.lines = layout::render(&self.blocks, &Ctx::new(&self.theme, &self.links), self.width);
        self.reoutline();
        if let Some(complaint) = complaint() {
            self.status = complaint;
        }

        self.cursor = 0;
        self.top = 0;
        self.focus = 0;

        self.plain = None;
        self.selection = None;
    }

    fn anchor_line(&self, fragment: Option<&str>) -> Option<usize> {
        let slug = ast::slug(&crate::markdown::links::decode_fragment(fragment?));
        self.lines.iter().position(|line| line.anchor.as_deref() == Some(slug.as_str()))
    }

    fn jump_to(&mut self, fragment: Option<&str>) {
        if let Some(line) = self.anchor_line(fragment) {
            self.go_to(line);
        }
    }

    fn reoutline(&mut self) {
        self.outline.entries = outline::build(&self.blocks, &self.lines);
        self.outline.selected = 0;
        self.outline.top = 0;
        self.properties = Properties::of(&self.links.document);
    }

    fn toc_view(&self) -> Rect {
        let area = Rect::new(0, 0, self.area.width, self.area.height);
        view::toc_inner(area, self.outline.entries.len())
    }

    pub fn toc_height(&self) -> usize {
        usize::from(self.toc_view().height).max(1)
    }

    fn toc_row(&self, row: u16) -> Option<usize> {
        let inner = self.toc_view();
        (row >= inner.y && row < inner.bottom()).then(|| usize::from(row - inner.y))
    }

    fn properties_view(&self) -> Rect {
        let area = Rect::new(0, 0, self.area.width, self.area.height);
        view::properties_inner(area, self.properties.rows.len())
    }

    pub fn properties_height(&self) -> usize {
        usize::from(self.properties_view().height).max(1)
    }

    fn properties_row(&self, row: u16) -> Option<usize> {
        let inner = self.properties_view();
        (row >= inner.y && row < inner.bottom()).then(|| usize::from(row - inner.y))
    }

    fn toggle_properties(&mut self) {
        if self.mode == Mode::Properties {
            self.mode = Mode::Browse;
            return;
        }
        if self.properties.is_empty() {
            self.status = Status::Notice(String::from("no properties in this document"));
            return;
        }
        self.mode = Mode::Properties;
    }

    fn pick_property(&mut self, row: u16) {
        let Some(row) = self.properties_row(row) else {
            self.mode = Mode::Browse;
            return;
        };
        self.properties.pick(row);
    }

    fn yank_property(&mut self) {
        let Some(row) = self.properties.selected() else { return };
        let (key, value) = (row.key.clone(), row.value.clone());
        let notice = match key.is_empty() {
            true => String::from("Copied the line"),
            false => format!("Copied {key}"),
        };
        self.copy(value, notice);
    }

    fn toggle_toc(&mut self) {
        if self.mode == Mode::Toc {
            self.mode = Mode::Browse;
            return;
        }
        if self.outline.is_empty() {
            self.status = Status::Notice(String::from("no headings in this document"));
            return;
        }
        self.mode = Mode::Toc;
        self.outline.open_at(self.cursor, self.toc_height());
    }

    fn pick_heading(&mut self, row: Option<u16>) {
        self.mode = Mode::Browse;
        let line = match row {
            Some(row) => self.toc_row(row).and_then(|row| self.outline.pick(row)).map(|entry| entry.line),
            None => self.outline.selected().map(|entry| entry.line),
        };
        let Some(line) = line else { return };

        self.remember();
        self.go_to(line);
        self.resume_search();
    }

    fn go_to(&mut self, line: usize) {
        self.cursor = line;
        self.top = line;
    }

    fn resume_search(&mut self) {
        if !self.search.query.is_empty() {
            self.rematch();
        }
    }

    fn confirm_search(&mut self) {
        self.mode = Mode::Browse;
        let query = std::mem::take(&mut self.search.input);
        if query.is_empty() {
            return;
        }

        self.search.query = query;
        let found = self.rematch();
        self.go_to_match(found);
    }

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
            Some(found) => self.cursor = found.line,
            None => self.status = Status::Notice(format!("no matches for \"{}\"", self.search.query)),
        }
    }

    fn rematch(&mut self) -> Option<search::Match> {
        let plain = self.plain.get_or_insert_with(|| self.lines.iter().map(RenderedLine::text).collect());
        self.search.matches = search::find(plain, &self.search.query);
        self.search.current = None;
        self.search.seek_from(self.cursor)
    }

    pub fn path(&self) -> Option<&Path> {
        self.links.document.path.as_deref()
    }

    pub fn report(&mut self, error: &str) {
        self.status = Status::Error(error.to_string());
    }

    pub fn viewport_height(&self) -> usize {
        usize::from(self.area.height.saturating_sub(CHROME_ROWS)).max(1)
    }

    pub fn link_progress(&self) -> Option<(usize, usize)> {
        let count = self.lines.get(self.cursor).map_or(0, |line| line.links.len());
        (count > 0).then_some((self.focus + 1, count))
    }

    pub fn percent(&self) -> usize {
        match self.lines.len() {
            0 | 1 => 100,
            len => self.cursor * 100 / (len - 1),
        }
    }

    fn move_cursor(&mut self, motion: Motion) {
        self.focus = 0;
        let height = self.viewport_height();
        let last = self.lines.len().saturating_sub(1);

        match motion {
            Motion::Line(delta) => self.cursor = offset(self.cursor, delta),
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

    fn jump(&mut self, delta: isize) {
        self.cursor = offset(self.cursor, delta);
        self.top = offset(self.top, delta);
    }

    fn scroll(&mut self, delta: isize) {
        self.top = offset(self.top, delta);
    }

    fn clicked_line(&self, row: u16) -> Option<usize> {
        let row = usize::from(row.checked_sub(CONTENT_TOP)?);
        if row >= self.viewport_height() {
            return None;
        }
        let line = self.top + row;
        (line < self.lines.len()).then_some(line)
    }

    fn point(&self, column: u16, row: u16) -> Option<Point> {
        let line = self.clicked_line(row)?;
        let column = usize::from(column).max(layout::GUTTER);
        Some(Point { line, byte: self.lines[line].byte_at(column), next: self.lines[line].byte_at(column + 1) })
    }

    fn select_start(&mut self, column: u16, row: u16) {
        self.selection = self.point(column, row).map(|point| Selection { anchor: point, head: point, dragged: false });
    }

    fn select_extend(&mut self, column: u16, row: u16) {
        let Some(point) = self.point(column, row) else { return };
        let Some(selection) = self.selection.as_mut() else { return };
        selection.head = point;
        selection.dragged |= selection.head != selection.anchor;
    }

    fn select_end(&mut self, column: u16, row: u16) {
        self.select_extend(column, row);
        match self.selection.filter(|selection| selection.dragged) {
            Some(_) => self.copy_selection(),
            None => {
                self.selection = None;
                self.click(column, row);
            }
        }
    }

    pub fn selected(&self, index: usize) -> Option<Range<usize>> {
        let selection = self.selection.filter(|selection| selection.dragged)?;
        let (from, to) = selection.span();
        if !(from.line..=to.line).contains(&index) {
            return None;
        }

        let line = self.lines.get(index)?;
        let end = line.text().len();
        let start = if index == from.line { from.byte } else { line.byte_at(line.inset) };
        let stop = if index == to.line { to.next } else { end };
        (start < stop).then(|| start..stop.min(end))
    }

    fn copy_selection(&mut self) {
        let Some(selection) = self.selection.filter(|selection| selection.dragged) else { return };
        let (from, to) = selection.span();

        let mut lines = Vec::new();
        for index in from.line..=to.line {
            let text = self.lines[index].text();
            let taken = match self.selected(index) {
                Some(range) => text[range].trim_end().to_string(),
                None => String::new(),
            };
            lines.push(taken);
        }
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }

        let count = lines.len();
        let notice = match count {
            1 => String::from("Copied the line"),
            _ => format!("Copied {count} lines"),
        };
        self.copy(lines.join("\n"), notice);
    }

    fn click(&mut self, column: u16, row: u16) {
        let Some(line) = self.clicked_line(row) else { return };
        let Some(index) = self.lines[line].link_at(usize::from(column)) else { return };

        self.cursor = line;
        self.focus = index;
        match self.focused().map(|link| &link.kind) {
            Some(LinkKind::External(_)) => self.open_external(),
            Some(_) => self.follow(),
            None => {}
        }
    }

    fn resize(&mut self, area: Size) {
        self.area = area;
        let width = layout::wrap_width(self.width_override, Some(area.width));
        if width != self.width {
            self.width = width;
            self.relayout();
        }
    }

    fn relayout(&mut self) {
        let place = self.place();
        self.rerender(place);
    }

    fn reload(&mut self) {
        let Some(path) = self.links.document.path.clone() else { return };
        let document = match Document::load(&path) {
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
        if self.reload_failed || !matches!(self.status, Status::Error(_)) {
            self.status = Status::Notice(format!("Reloaded {}", self.file));
        }
        self.reload_failed = false;
    }

    fn place(&self) -> Place {
        Place { anchor: self.anchor(), row: self.cursor as isize - self.top as isize }
    }

    fn rerender(&mut self, place: Place) {
        self.lines = layout::render(&self.blocks, &Ctx::new(&self.theme, &self.links), self.width);
        self.reoutline();
        if let Some(complaint) = complaint() {
            self.status = complaint;
        }

        self.cursor = self
            .lines
            .iter()
            .position(|line| line.source_line == place.anchor)
            .or_else(|| self.lines.iter().position(|line| !line.is_blank() && line.source_line >= place.anchor))
            .unwrap_or(self.cursor);
        self.top = offset(self.cursor, -place.row);
        self.focus = 0;

        self.plain = None;
        self.selection = None;
        self.resume_search();
    }

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

    fn bound(&mut self) {
        let height = self.viewport_height();
        let last = self.lines.len().saturating_sub(1);

        self.cursor = self.cursor.min(last);
        self.top = self.top.min(last.saturating_sub(height.saturating_sub(1)));
    }

    fn reveal(&mut self) {
        let height = self.viewport_height();

        self.top = self.top.min(self.cursor);
        self.top = self.top.max(self.cursor.saturating_sub(height - 1));
    }

    fn snap(&mut self) {
        let height = self.viewport_height();
        let last = self.lines.len().saturating_sub(1);

        let snapped = self.cursor.clamp(self.top, (self.top + height - 1).min(last));
        if snapped != self.cursor {
            self.cursor = snapped;
            self.focus = 0;
        }
    }
}

fn complaint() -> Option<Status> {
    let warnings = crate::render::code::take_warnings();
    (!warnings.is_empty()).then(|| Status::Error(warnings.join("; ")))
}

fn names(document: &Document) -> (String, String) {
    let file = document
        .path
        .as_deref()
        .and_then(|path| path.file_name())
        .map_or_else(|| STDIN.to_string(), |name| name.to_string_lossy().into_owned());
    let title = document.title.clone().unwrap_or_else(|| file.clone());
    (title, file)
}

fn offset(index: usize, delta: isize) -> usize {
    if delta >= 0 { index.saturating_add(delta as usize) } else { index.saturating_sub(delta.unsigned_abs()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

    fn numbered(count: usize) -> String {
        (1..=count).map(|n| format!("line {n}\n\n")).collect()
    }

    fn click(app: &mut App, column: u16, row: u16) {
        app.apply(Action::SelectStart { column, row });
        app.apply(Action::SelectEnd { column, row });
    }

    fn drag(app: &mut App, from: (u16, u16), to: (u16, u16)) {
        app.apply(Action::SelectStart { column: from.0, row: from.1 });
        app.apply(Action::SelectExtend { column: to.0, row: to.1 });
        app.apply(Action::SelectEnd { column: to.0, row: to.1 });
    }

    fn app(source: &str, height: u16) -> App {
        let document = Document::new(Some(PathBuf::from("notes/x.md")), PathBuf::from("notes"), source.to_string());
        App::new(document, Theme::default(), &Options { width: Some(40), ..Options::default() }, Size::new(60, height))
    }

    fn noted(height: u16) -> App {
        let source = format!("---\ntitle: Notes\ntags: [inbox]\ncreated: today\n---\n\n# Heading\n\n{}", numbered(40));
        app(&source, height)
    }

    #[test]
    fn the_properties_window_toggles_and_leaves_the_document_alone() {
        let mut app = noted(20);
        let lines = app.lines.clone();
        let (cursor, top) = (app.cursor, app.top);

        app.apply(Action::ToggleProperties);
        assert_eq!(app.mode, Mode::Properties);
        assert_eq!(app.properties.rows.len(), 3);
        assert_eq!(app.lines, lines, "the document is not re-laid-out");
        assert_eq!((app.cursor, app.top), (cursor, top), "and the reading position does not move");

        app.apply(Action::ToggleProperties);
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn a_document_without_properties_says_so_rather_than_opening_an_empty_window() {
        let mut app = app("# Heading\n\nbody\n", 20);

        app.apply(Action::ToggleProperties);

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.status, Status::Notice(String::from("no properties in this document")));
    }

    #[test]
    fn the_window_walks_its_rows_and_yanks_the_selected_value() {
        let mut app = noted(20);
        app.apply(Action::ToggleProperties);
        app.apply(Action::PropertiesMove(Motion::Line(1)));

        app.apply(Action::PropertiesYank);

        assert_eq!(app.take_copy().as_deref(), Some("inbox"));
        assert_eq!(app.status, Status::Notice(String::from("Copied tags")));
    }

    #[test]
    fn yanking_a_verbatim_row_copies_the_line_it_could_not_read() {
        let mut app = app("---\nthis is not a pair\n---\n\nbody\n", 20);
        app.apply(Action::ToggleProperties);

        app.apply(Action::PropertiesYank);

        assert_eq!(app.take_copy().as_deref(), Some("this is not a pair"));
    }

    #[test]
    fn a_click_in_the_window_selects_that_row_and_a_click_outside_closes_it() {
        let mut app = noted(20);
        app.apply(Action::ToggleProperties);
        let inner = view::properties_inner(Rect::new(0, 0, 60, 20), app.properties.rows.len());

        app.apply(Action::PropertiesClick { row: inner.y + 2 });
        assert_eq!(app.mode, Mode::Properties);
        assert_eq!(app.properties.selected, 2);

        app.apply(Action::PropertiesClick { row: inner.bottom() });
        assert_eq!(app.mode, Mode::Browse, "a click outside the box closes the window");
    }

    #[test]
    fn the_properties_follow_the_document_across_a_reload() {
        let (dir, mut app) = on_disk("---\ntitle: Notes\ntags: [inbox]\n---\n\nbody\n");
        assert_eq!(app.properties.rows.len(), 2);

        rewrite(&dir, "---\nonly: one\n---\n\nbody\n");
        app.apply(Action::Reload);

        assert_eq!(app.properties.rows.len(), 1, "the window reads the document that is there now");
        assert_eq!(app.properties.selected().map(|row| row.key.as_str()), Some("only"));
    }

    #[test]
    fn the_properties_follow_the_document_across_a_link() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("other.md"), "---\nonly: one\n---\n\nbody\n").expect("write");
        let source = "---\ntitle: Notes\ntags: [inbox]\n---\n\n[other](other.md)\n";
        std::fs::write(dir.path().join("note.md"), source).expect("write");
        let document = Document::load(&dir.path().join("note.md")).expect("load");
        let mut app = App::new(document, Theme::default(), &Options { width: Some(40), ..Options::default() }, Size::new(60, 14));
        assert_eq!(app.properties.rows.len(), 2);

        app.apply(Action::Focus { forward: true });
        app.apply(Action::Follow);

        assert_eq!(app.properties.rows.len(), 1, "{:?}", app.status);
    }

    fn paged() -> App {
        app(&numbered(40), 14)
    }

    #[test]
    fn the_viewport_is_the_terminal_less_the_chrome() {
        assert_eq!(paged().viewport_height(), 10);
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
    fn a_click_on_plain_text_changes_nothing() {
        let mut app = paged();
        app.apply(Action::Scroll(12));
        let (cursor, top) = (app.cursor, app.top);

        click(&mut app, 0, CONTENT_TOP + 3);
        assert_eq!((app.cursor, app.top), (cursor, top), "a click is for links, not for the reading position");
    }

    #[test]
    fn the_wheel_takes_the_cursor_with_it_rather_than_leaving_it_behind() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Line(1)));

        for _ in 0..10 {
            app.apply(Action::Scroll(3));
        }
        assert_eq!((app.top, app.cursor), (30, 30), "scrolling down leaves the cursor on the first visible row");

        for _ in 0..10 {
            app.apply(Action::Scroll(-3));
        }
        assert_eq!((app.top, app.cursor), (0, 9), "and scrolling up, on the last one");
    }

    #[test]
    fn a_wheel_notch_pulls_the_cursor_to_the_nearest_line_it_can_still_see() {
        let mut app = paged();
        app.apply(Action::Scroll(3));
        assert_eq!((app.top, app.cursor), (3, 3), "the cursor was above the view, so it came to the first row");

        app.apply(Action::Move(Motion::Page(1)));
        assert_eq!(app.cursor, app.top, "the page put the cursor on the first row");

        app.apply(Action::Scroll(-3));
        assert_eq!(app.cursor, 12, "a notch that leaves the cursor visible does not move it");

        app.apply(Action::Scroll(-30));
        assert_eq!(app.top, 0);
        assert_eq!(app.cursor, app.viewport_height() - 1, "the cursor was below the view, so it came to the last row");
    }

    #[test]
    fn the_wheel_moves_nothing_in_a_document_shorter_than_the_screen() {
        let mut app = app(&numbered(3), 14);
        assert!(app.lines.len() < app.viewport_height());

        for delta in [3, -3, 30, -30] {
            app.apply(Action::Scroll(delta));
            assert_eq!((app.top, app.cursor), (0, 0), "a notch of {delta} moved something");
        }
    }

    #[test]
    fn the_cursor_is_never_off_the_screen_whatever_the_reader_does() {
        let (dir, mut app) = on_disk(&numbered(40));
        let actions = [
            Action::Scroll(3),
            Action::Scroll(-3),
            Action::Scroll(1000),
            Action::Scroll(-1000),
            Action::Move(Motion::Line(1)),
            Action::Move(Motion::Line(-1)),
            Action::Move(Motion::HalfPage(1)),
            Action::Move(Motion::Page(1)),
            Action::Move(Motion::Bottom),
            Action::Move(Motion::Top),
            Action::Resize(Size::new(60, 40)),
            Action::Resize(Size::new(60, 6)),
            Action::Reload,
            Action::Focus { forward: true },
        ];

        for action in actions {
            app.apply(action);
            let visible = app.top..app.top + app.viewport_height();
            assert!(visible.contains(&app.cursor), "{action:?} left the cursor at {} outside {visible:?}", app.cursor);
        }

        click(&mut app, 0, CONTENT_TOP + 1);
        let visible = app.top..app.top + app.viewport_height();
        assert!(visible.contains(&app.cursor), "a click left the cursor outside {visible:?}");
        drop(dir);
    }

    #[test]
    fn a_shrinking_terminal_keeps_the_reader_on_their_line_and_on_the_screen() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Page(1)));
        app.apply(Action::Move(Motion::Line(5)));
        let source_line = app.lines[app.cursor].source_line;

        app.apply(Action::Resize(Size::new(60, 6)));

        assert_eq!(app.lines[app.cursor].source_line, source_line, "the reader was moved off their line");
        let visible = app.top..app.top + app.viewport_height();
        assert!(visible.contains(&app.cursor), "the cursor is outside {visible:?}");
    }

    #[test]
    fn a_query_confirmed_after_a_scroll_seeks_from_the_line_on_screen() {
        let mut app = app(&numbered(40), 14);
        app.apply(Action::Scroll(20));
        let from = app.cursor;

        search_for(&mut app, "LINE");

        assert!(app.cursor >= from, "the search went backwards, from a line the reader had left");
    }

    #[test]
    fn the_wheel_changes_nothing_when_the_view_is_already_at_either_end() {
        let mut app = paged();
        let (top, cursor) = (app.top, app.cursor);
        for _ in 0..5 {
            app.apply(Action::Scroll(-3));
        }
        assert_eq!((app.top, app.cursor), (top, cursor), "the view is already at the head");

        app.apply(Action::Move(Motion::Bottom));
        let (top, cursor) = (app.top, app.cursor);
        for _ in 0..5 {
            app.apply(Action::Scroll(3));
        }
        assert_eq!((app.top, app.cursor), (top, cursor), "the view is already at the foot");
    }

    #[test]
    fn a_motion_key_carries_on_from_where_the_wheel_left_the_cursor() {
        let mut app = paged();
        app.apply(Action::Scroll(30));
        assert_eq!((app.top, app.cursor), (30, 30), "the wheel already put the cursor on screen");

        app.apply(Action::Move(Motion::Line(1)));
        assert_eq!(app.cursor, 31, "moved on from the line the reader can see");
        assert_eq!(app.top, 30, "and the viewport the reader chose is left alone");

        app.apply(Action::Scroll(-30));
        assert_eq!((app.top, app.cursor), (0, 9), "scrolling back put it on the last visible row");
        app.apply(Action::Move(Motion::Line(-1)));
        assert_eq!(app.cursor, 8, "moved on from there");
        assert_eq!(app.top, 0);
    }

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
        while app.lines[app.cursor].is_blank() {
            app.apply(Action::Move(Motion::Line(1)));
        }
        let (source_line, row, lines) = (app.lines[app.cursor].source_line, app.cursor - app.top, app.lines.len());

        rewrite(&dir, &format!("{}{}", numbered(40), numbered(10)));
        app.apply(Action::Reload);

        assert!(app.lines.len() > lines, "the file on disk was not re-read");
        assert_eq!(app.lines[app.cursor].source_line, source_line, "the reader lost the line they were on");
        assert_eq!(app.cursor - app.top, row, "and the height on the screen it was at");
    }

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
        let source =
            (1..=20).map(|n| format!("paragraph {n} with enough words in it to wrap twice over\n\n")).collect::<String>();
        let document = Document::new(Some(PathBuf::from("x.md")), PathBuf::from("."), source.clone());
        let mut app = App::new(document, Theme::default(), &Options { width: None, ..Options::default() }, Size::new(70, 14));

        app.apply(Action::Move(Motion::Page(1)));
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

    fn sectioned() -> App {
        let source: String = (1..=12).map(|n| format!("## Section {n}\n\n{}", "filler\n\n".repeat(4))).collect();
        app(&source, 14)
    }

    fn heading_line(app: &App, text: &str) -> usize {
        app.outline.entries.iter().find(|entry| entry.text == text).expect("heading").line
    }

    #[test]
    fn the_contents_overlay_toggles() {
        let mut app = sectioned();
        app.apply(Action::ToggleToc);
        assert_eq!(app.mode, Mode::Toc);

        app.apply(Action::ToggleToc);
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn a_document_with_no_headings_says_so_rather_than_opening_an_empty_box() {
        let mut app = paged();
        app.apply(Action::ToggleToc);

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.status, Status::Notice(String::from("no headings in this document")));
    }

    #[test]
    fn the_contents_overlay_opens_on_the_section_the_reader_is_in() {
        let mut app = sectioned();
        app.apply(Action::Move(Motion::Bottom));
        app.apply(Action::ToggleToc);

        assert_eq!(app.outline.selected().map(|entry| entry.text.as_str()), Some("Section 12"));
    }

    #[test]
    fn choosing_a_heading_closes_the_overlay_and_puts_it_on_the_first_row() {
        let mut app = sectioned();
        let wanted = heading_line(&app, "Section 2");

        app.apply(Action::ToggleToc);
        app.apply(Action::TocMove(Motion::Line(1)));
        app.apply(Action::TocSelect);

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.top, wanted, "the heading is the first line in view");
        assert_eq!(app.cursor, wanted, "and the cursor is on it");
    }

    #[test]
    fn history_goes_back_to_where_the_reader_was_before_the_jump() {
        let mut app = sectioned();
        app.apply(Action::Move(Motion::Line(1)));
        let (top, cursor) = (app.top, app.cursor);

        app.apply(Action::ToggleToc);
        app.apply(Action::TocMove(Motion::Bottom));
        app.apply(Action::TocSelect);
        assert_ne!(app.cursor, cursor);

        app.apply(Action::History { forward: false });
        assert_eq!((app.top, app.cursor), (top, cursor));
    }

    #[test]
    fn a_click_in_the_overlay_chooses_the_heading_on_that_row() {
        let mut app = sectioned();
        let wanted = heading_line(&app, "Section 2");
        app.apply(Action::ToggleToc);

        let row = crate::ui::view::toc_inner(Rect::new(0, 0, 60, 14), app.outline.entries.len()).y + 1;
        app.apply(Action::TocClick { row });

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.cursor, wanted);
    }

    #[test]
    fn a_click_outside_the_listing_closes_the_overlay_without_moving() {
        let mut app = sectioned();
        app.apply(Action::ToggleToc);
        let (top, cursor) = (app.top, app.cursor);

        app.apply(Action::TocClick { row: 0 });

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!((app.top, app.cursor), (top, cursor));
    }

    #[test]
    fn the_wheel_scrolls_the_listing_and_not_the_document_behind_it() {
        let mut app = sectioned();
        app.apply(Action::ToggleToc);
        let (top, cursor) = (app.top, app.cursor);

        app.apply(Action::TocScroll(1));

        assert_eq!((app.top, app.cursor), (top, cursor), "the document stays put");
        assert_eq!(app.outline.top, 1);
    }

    #[test]
    fn the_outline_still_points_at_its_headings_after_a_resize() {
        let mut app = sectioned();
        app.apply(Action::Resize(Size::new(30, 20)));

        assert!(!app.outline.entries.is_empty());
        for entry in &app.outline.entries {
            assert!(app.lines[entry.line].anchor.is_some(), "{entry:?} is not on a heading line");
        }
    }

    #[test]
    fn the_outline_follows_the_document_across_a_reload() {
        let (dir, mut app) = on_disk("# One\n\ntext\n");
        assert_eq!(app.outline.entries.len(), 1);

        rewrite(&dir, "# One\n\ntext\n\n## Added\n\nmore\n");
        app.apply(Action::Reload);

        assert_eq!(app.outline.entries.iter().map(|entry| entry.text.as_str()).collect::<Vec<_>>(), vec!["One", "Added"]);
        for entry in &app.outline.entries {
            assert!(app.lines[entry.line].anchor.is_some(), "{entry:?} is not on a heading line");
        }
    }

    #[test]
    fn the_document_does_not_move_behind_the_overlay() {
        let mut app = paged();
        app.apply(Action::Move(Motion::Page(1)));
        app.apply(Action::ToggleHelp);
        let (top, cursor) = (app.top, app.cursor);

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

    fn vault() -> App {
        let document = Document::load(std::path::Path::new("tests/fixtures/vault/index.md")).expect("the fixture is there");
        App::new(document, Theme::default(), &Options { width: Some(78), ..Options::default() }, Size::new(80, 24))
    }

    fn inside_vault(source: &str) -> App {
        let document = Document::new(
            Some(PathBuf::from("tests/fixtures/vault/scratch.md")),
            PathBuf::from("tests/fixtures/vault"),
            source.to_string(),
        );
        App::new(document, Theme::default(), &Options { width: Some(78), ..Options::default() }, Size::new(80, 24))
    }

    #[test]
    fn a_drag_across_one_line_copies_what_it_covered() {
        let mut app = app("the quick brown fox\n", 14);
        drag(&mut app, (layout::GUTTER as u16, CONTENT_TOP), (layout::GUTTER as u16 + 9, CONTENT_TOP));

        assert_eq!(app.take_copy().as_deref(), Some("the quick"));
        assert_eq!(app.status, Status::Notice(String::from("Copied the line")));
    }

    #[test]
    fn a_drag_starting_mid_line_copies_from_where_it_started() {
        let mut app = app("the quick brown fox\n", 14);
        drag(&mut app, (layout::GUTTER as u16 + 4, CONTENT_TOP), (layout::GUTTER as u16 + 19, CONTENT_TOP));

        assert_eq!(app.take_copy().as_deref(), Some("quick brown fox"));
    }

    #[test]
    fn a_drag_down_the_page_copies_every_line_it_crossed() {
        let mut app = app("first\n\nsecond\n\nthird\n", 14);
        drag(&mut app, (layout::GUTTER as u16, CONTENT_TOP), (layout::GUTTER as u16 + 5, CONTENT_TOP + 2));

        assert_eq!(app.take_copy().as_deref(), Some("first\n\nsecond"));
        assert_eq!(app.status, Status::Notice(String::from("Copied 3 lines")));
    }

    #[test]
    fn a_drag_up_the_page_reads_the_same_as_a_drag_down_it() {
        let mut app = app("first\n\nsecond\n", 14);
        drag(&mut app, (layout::GUTTER as u16 + 6, CONTENT_TOP + 2), (layout::GUTTER as u16, CONTENT_TOP));

        assert_eq!(app.take_copy().as_deref(), Some("first\n\nsecond"));
    }

    #[test]
    fn a_drag_never_reaches_the_gutter() {
        let mut app = app("indented text\n", 14);
        drag(&mut app, (0, CONTENT_TOP), (layout::GUTTER as u16 + 8, CONTENT_TOP));

        assert_eq!(app.take_copy().as_deref(), Some("indented"), "the gutter is chrome, not text");
    }

    #[test]
    fn a_press_and_release_on_one_cell_is_a_click_rather_than_a_selection() {
        let mut app = inside_vault("some text and [[note]] on one line\n");
        click_link(&mut app, "note", 0);

        assert_eq!(app.file, "note.md", "the link was followed");
        assert_eq!(app.take_copy(), None, "nothing was copied");
    }

    #[test]
    fn a_selection_is_dropped_by_the_next_move() {
        let mut app = app("first\n\nsecond\n", 14);
        app.apply(Action::SelectStart { column: 1, row: CONTENT_TOP });
        app.apply(Action::SelectExtend { column: 5, row: CONTENT_TOP });
        assert!(app.selected(0).is_some());

        app.apply(Action::Move(Motion::Line(1)));
        assert_eq!(app.selected(0), None);
    }

    #[test]
    fn a_selection_is_dropped_when_the_document_is_laid_out_again() {
        let mut app = app("first\n\nsecond\n", 14);
        app.apply(Action::SelectStart { column: 1, row: CONTENT_TOP });
        app.apply(Action::SelectExtend { column: 5, row: CONTENT_TOP });

        app.apply(Action::Resize(Size::new(30, 14)));
        assert_eq!(app.selected(0), None);
    }

    #[test]
    fn a_drag_outside_the_content_pane_selects_nothing() {
        let mut app = app("first\n", 14);
        let below = CONTENT_TOP + app.viewport_height() as u16;
        drag(&mut app, (1, 0), (5, below));

        assert_eq!(app.take_copy(), None);
    }

    #[test]
    fn a_selection_covers_the_whole_of_the_lines_between_its_ends() {
        let mut app = app("first\n\nsecond\n\nthird\n", 14);
        app.apply(Action::SelectStart { column: layout::GUTTER as u16 + 3, row: CONTENT_TOP });
        app.apply(Action::SelectExtend { column: layout::GUTTER as u16 + 2, row: CONTENT_TOP + 4 });

        assert_eq!(app.selected(0), Some(4..6), "from where it started to the end");
        assert_eq!(app.selected(2), Some(1..7), "all of the line in between");
        assert_eq!(app.selected(4), Some(1..4), "through the cell it stopped on");
    }

    #[test]
    fn yanking_copies_the_cursor_line_without_the_gutter() {
        let mut app = app("a paragraph to copy\n", 14);
        app.apply(Action::Yank);

        assert_eq!(app.take_copy().as_deref(), Some("a paragraph to copy"));
        assert_eq!(app.status, Status::Notice(String::from("Copied the line")));
    }

    #[test]
    fn yanking_a_code_line_copies_the_code_without_its_padding() {
        let mut app = app("```rust\n    let x = 1;\n```\n", 14);
        app.apply(Action::Move(Motion::Line(1)));
        app.apply(Action::Yank);

        assert_eq!(app.take_copy().as_deref(), Some("    let x = 1;"), "the pad was copied, or the indentation was not");
    }

    #[test]
    fn yanking_a_quoted_code_line_copies_only_the_code() {
        let mut app = app("> ```rust\n>     let x = 1;\n> ```\n", 14);
        app.apply(Action::Move(Motion::Line(1)));
        app.apply(Action::Yank);

        assert_eq!(app.take_copy().as_deref(), Some("    let x = 1;"), "the quote gutter or the pad came along");
    }

    #[test]
    fn a_drag_down_a_code_block_copies_the_lines_without_their_padding() {
        let mut app = app("```rust\nlet x = 1;\nlet y = 2;\n```\n", 14);
        let first = app.lines.iter().position(|line| line.text().contains("let x")).expect("a code row");
        let row = CONTENT_TOP + first as u16;
        let inset = app.lines[first].inset as u16;
        drag(&mut app, (inset, row), (60, row + 1));

        assert_eq!(app.take_copy().as_deref(), Some("let x = 1;\nlet y = 2;"));
    }

    #[test]
    fn a_copy_is_handed_over_once() {
        let mut app = app("a paragraph to copy\n", 14);
        app.apply(Action::Yank);

        assert!(app.take_copy().is_some());
        assert_eq!(app.take_copy(), None);
    }

    #[test]
    fn yanking_a_blank_line_copies_nothing() {
        let mut app = app("first\n\nthird\n", 14);
        app.apply(Action::Move(Motion::Line(1)));
        app.apply(Action::Yank);

        assert_eq!(app.take_copy(), None);
        assert_eq!(app.status, Status::Idle);
    }

    #[test]
    fn yanking_a_link_copies_where_it_points_rather_than_its_label() {
        let mut app = inside_vault("see [the note](notes/note.md) for more\n");
        focus_link(&mut app, "notes/note.md");
        app.apply(Action::YankLink);

        assert_eq!(app.take_copy().as_deref(), Some("notes/note.md"));
        assert_eq!(app.status, Status::Notice(String::from("Copied notes/note.md")));
    }

    #[test]
    fn yanking_a_link_off_a_line_that_has_none_says_so() {
        let mut app = app("no links here\n", 14);
        app.apply(Action::YankLink);

        assert_eq!(app.take_copy(), None);
        assert_eq!(app.status, Status::Notice(String::from("no link on this line")));
    }

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

    fn column_of(line: &RenderedLine, link: &LinkRef) -> u16 {
        line.spans[..link.span_range.start].iter().map(crate::render::line::StyledSpan::width).sum::<usize>() as u16
    }

    fn click_link(app: &mut App, destination: &str, occurrence: usize) {
        let mut seen = 0;
        for index in 0..app.lines.len() {
            let line = &app.lines[index];
            let Some(link) = line.links.iter().find(|link| link.kind.destination() == destination) else { continue };
            if seen < occurrence {
                seen += 1;
                continue;
            }
            let column = column_of(line, link);
            let row = CONTENT_TOP + (index - app.top) as u16;
            click(app, column, row);
            return;
        }
        panic!("no link to {destination:?} in the fixture");
    }

    #[test]
    fn a_click_on_a_wikilink_opens_it() {
        let mut app = inside_vault("some text and [[note]] on one line\n");
        click_link(&mut app, "note", 0);

        assert_eq!(app.file, "note.md");
        assert_eq!(open_path(&app), PathBuf::from("tests/fixtures/vault/note.md"));
        assert_eq!(app.status, Status::Notice(String::from("Opened note.md")));
    }

    #[test]
    fn a_click_picks_the_link_it_landed_on_rather_than_the_first_one() {
        let mut app = inside_vault("[[missing]] and [[note]] on one line\n");
        click_link(&mut app, "note", 0);

        assert_eq!(app.file, "note.md", "the second link is the one under the pointer");
        assert_eq!(app.focus, 0, "a new document opens with nothing focused");
    }

    #[test]
    fn a_click_on_a_broken_link_says_so_rather_than_doing_nothing() {
        let mut app = inside_vault("a line with [[missing]] in it\n");
        click_link(&mut app, "missing", 0);

        assert_eq!(app.file, "scratch.md", "nothing was opened");
        assert!(matches!(app.status, Status::Error(_)), "{:?}", app.status);
    }

    #[test]
    fn a_click_beside_a_link_changes_nothing() {
        let mut app = inside_vault("plain\n\nsome text and [[note]] here\n");
        let line = app.lines.iter().position(|line| !line.links.is_empty()).expect("a line with a link");

        click(&mut app, 0, CONTENT_TOP + line as u16);
        assert_eq!(app.cursor, 0, "landing beside a link is not landing on it");
        assert_eq!(app.file, "scratch.md");
        assert_eq!(app.status, Status::Idle);
    }

    #[test]
    fn a_click_outside_the_content_pane_follows_nothing() {
        let mut app = inside_vault("[[note]] on the first line\n");
        let height = app.viewport_height() as u16;

        for row in [0, 1, CONTENT_TOP + height, CONTENT_TOP + height + 1] {
            click(&mut app, 1, row);
            assert_eq!(app.file, "scratch.md", "row {row} is chrome, not content");
        }
    }

    #[test]
    fn either_half_of_a_link_broken_across_lines_opens_it() {
        let text = "the same note under a deliberately long label that will not fit on one rendered line";
        let source = format!("a paragraph containing [{text}](note.md) and nothing else\n");

        let mut first = inside_vault(&source);
        let carrying: Vec<usize> = (0..first.lines.len()).filter(|index| !first.lines[*index].links.is_empty()).collect();
        assert_eq!(carrying.len(), 2, "the fixture must wrap the link");

        click_link(&mut first, "note.md", 0);
        let mut second = inside_vault(&source);
        click_link(&mut second, "note.md", 1);

        assert_eq!(open_path(&first), PathBuf::from("tests/fixtures/vault/note.md"));
        assert_eq!(open_path(&second), open_path(&first));
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

        app.apply(Action::History { forward: false });
        assert_eq!(app.cursor, app.lines.len() - 1);
    }

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

        app.apply(Action::Move(Motion::HalfPage(1)));
        app.cursor = app.top + app.viewport_height() / 2;
        app.apply(Action::Focus { forward: true });
        let (cursor, focus) = (app.cursor, app.focus);

        app.apply(Action::Scroll(1));
        assert_eq!(app.cursor, cursor, "a notch this small left the cursor visible, so it did not move");
        assert_eq!(app.focus, focus, "and the reader's chosen link stood");
    }

    #[test]
    fn a_wheel_notch_that_moves_the_cursor_drops_the_focus_with_it() {
        let mut app = inside_vault("[[note]] and [[missing]]\n\n".to_string().repeat(30).as_str());
        let line = app.lines.iter().position(|line| line.links.len() > 1).expect("a line with several links");
        app.cursor = line;
        app.apply(Action::Focus { forward: true });
        assert_eq!(app.focus, 1);

        let away = app.viewport_height() as isize * 2;
        app.apply(Action::Scroll(away));
        assert_eq!(app.cursor, app.top, "the wheel carried the cursor to the top of the new view");
        assert_eq!(app.focus, 0, "a link chosen on the line left behind cannot still be chosen");

        let links = app.lines[app.cursor].links.len();
        assert_eq!(app.link_progress(), (links > 0).then_some((1, links)), "the statusbar counts the new line's links");
    }

    #[test]
    fn a_fragment_that_names_no_heading_does_not_spend_a_history_entry() {
        let mut app = vault();
        focus_link(&mut app, "note");
        app.apply(Action::Follow);
        app.apply(Action::History { forward: false });
        assert_eq!(app.file, "index.md");

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
