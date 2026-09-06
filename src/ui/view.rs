//! Painting one frame: header, hairline rules, content, statusbar.
//!
//! The content area is drawn a cell at a time from the visible slice of
//! `App::lines`, never through a widget built over the whole document. That is
//! what keeps a frame's cost proportional to the terminal rather than to the
//! file: a hundred-thousand-line note paints exactly as fast as a short one.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Clear, Widget};
use ratatui::{Frame, symbols};
use unicode_width::UnicodeWidthStr;

use crate::render::line::RenderedLine;
use crate::theme::Element;
use crate::ui::app::{App, Mode, Status};

/// The shortcut hints, right-aligned in the header. They list the bindings
/// this build has, and grow as milestones land.
const HINTS: &str = "? help  / search  q quit";
/// Ahead of the title, so the reader can see what they are running.
const TITLE_PREFIX: &str = " vademecum · ";
/// One column between the title and the hints before the hints give way.
const HINT_GAP: usize = 2;
/// The overlay's share of the terminal: 60% of the width, and a height clamped
/// between these shares of the height.
const HELP_WIDTH: (u32, u32) = (3, 5);
const HELP_FLOOR: (u32, u32) = (4, 10);
const HELP_CEILING: (u32, u32) = (9, 10);

/// Every binding, as the overlay lists them — the README's table, in order.
/// The rows for links and history arrive with milestone 5.
const HELP: &[(&str, &str)] = &[
    ("j / k, ↓ / ↑", "Move cursor line down / up"),
    ("d / u, Ctrl-D / Ctrl-U", "Half page down / up"),
    ("Space / b, PgDn / PgUp", "Page down / up"),
    ("g / G, Home / End", "Top / bottom"),
    ("/", "Search (Enter confirms, Esc cancels)"),
    ("n / N", "Next / previous match"),
    ("?", "Help overlay"),
    ("Esc", "Close overlay, clear search highlight"),
    ("q, Ctrl-C", "Quit"),
];

/// Paint the pager. The single entry point the event loop calls.
pub fn draw(frame: &mut Frame, app: &App) {
    frame.render_widget(Screen { app }, frame.area());
}

/// The whole screen for one frame.
struct Screen<'a> {
    app: &'a App,
}

impl Widget for Screen<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows =
            [Constraint::Length(1), Constraint::Length(1), Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)];
        let [header_row, top_rule, content_rows, bottom_rule, status_row] = Layout::vertical(rows).areas(area);

        let hint = self.app.theme.style(Element::Hint);
        header(header_row, buf, self.app);
        rule(top_rule, buf, hint);
        content(content_rows, buf, self.app);
        rule(bottom_rule, buf, hint);
        statusbar(status_row, buf, self.app);

        if self.app.mode == Mode::Help {
            help(area, buf, self.app);
        }
    }
}

/// Write one row, clipped to the area, and draw nothing at all when there is
/// no row to draw on. A terminal too short for the chrome makes `Layout` hand
/// back zero-height rects, and it places them one row past the buffer; writing
/// to one of those panics inside ratatui rather than being ignored.
fn row(area: Rect, buf: &mut Buffer, x: u16, text: &str, style: Style) {
    if area.height == 0 || x >= area.right() {
        return;
    }
    buf.set_stringn(x, area.y, text, (area.right() - x) as usize, style);
}

/// ` vademecum · <title>` on the left, the hints on the right. When the two
/// would collide the hints give way: the title is what says which document
/// this is.
fn header(area: Rect, buf: &mut Buffer, app: &App) {
    let title = format!("{TITLE_PREFIX}{}", app.title);
    row(area, buf, area.x, &title, app.theme.style(Element::HeaderTitle));

    let hints = HINTS.width();
    if title.width() + HINT_GAP + hints <= area.width as usize {
        row(area, buf, area.right() - hints as u16, HINTS, app.theme.style(Element::Hint));
    }
}

/// A hairline across the full width.
fn rule(area: Rect, buf: &mut Buffer, style: Style) {
    row(area, buf, area.x, &symbols::line::HORIZONTAL.repeat(area.width as usize), style);
}

fn content(area: Rect, buf: &mut Buffer, app: &App) {
    let cursor_line = app.theme.style(Element::CursorLine);
    let last = app.lines.len().min(app.top + area.height as usize);

    for (row, index) in (app.top..last).enumerate() {
        let y = area.y + row as u16;
        let line = &app.lines[index];
        let on_cursor = index == app.cursor;

        // The cursor line is a bar across the whole terminal, padding
        // included, so it reads as one band however short the text is.
        if on_cursor {
            buf.set_style(Rect::new(area.x, y, area.width, 1), cursor_line);
        }

        let mut x = area.x;
        for span in &line.spans {
            if x >= area.right() {
                break;
            }
            // Patching puts the cursor's background over the span's own, so
            // the bar survives a code block, and leaves the foreground alone.
            let style = if on_cursor { span.style.patch(cursor_line) } else { span.style };
            let (next, _) = buf.set_stringn(x, y, &span.text, (area.right() - x) as usize, style);
            if next == x {
                break;
            }
            x = next;
        }

        highlight(area, buf, y, app, index, line);
    }
}

/// Repaint the matched cells of one line. Byte offsets become columns here,
/// on the handful of visible lines that have a match, rather than everywhere.
fn highlight(area: Rect, buf: &mut Buffer, y: u16, app: &App, index: usize, line: &RenderedLine) {
    let found = app.search.on_line(index);
    if found.is_empty() {
        return;
    }

    let text = line.text();
    let current = app.search.current.and_then(|index| app.search.matches.get(index));
    for hit in found {
        let style =
            if Some(hit) == current { app.theme.style(Element::SearchCurrent) } else { app.theme.style(Element::SearchMatch) };
        let start = area.x + text[..hit.start].width() as u16;
        let width = text[hit.start..hit.end].width() as u16;
        if start >= area.right() {
            continue;
        }
        buf.set_style(Rect::new(start, y, width.min(area.right() - start), 1), style);
    }
}

/// The `/` prompt while a query is being typed, else a notice, else
/// `file · line X/Y · N%` with the match count when a query is standing.
fn statusbar(area: Rect, buf: &mut Buffer, app: &App) {
    let (text, element) = match (app.mode, &app.status) {
        (Mode::Search, _) => (format!("/{}", app.search.input), Element::Status),
        (_, Status::Notice(notice)) => (notice.clone(), Element::StatusNotice),
        (_, Status::Idle) => {
            // `min` so an empty document reads `line 0/0` rather than `1/0`.
            let total = app.lines.len();
            let mut status = format!("{} · line {}/{} · {}%", app.file, (app.cursor + 1).min(total), total, app.percent());
            if let Some((index, total)) = app.search.progress() {
                status.push_str(&format!(" · match {index}/{total}"));
            }
            (status, Element::Status)
        }
    };
    row(area, buf, area.x, &text, app.theme.style(element));
}

/// The keybinding table, in a centred popup over the document.
fn help(area: Rect, buf: &mut Buffer, app: &App) {
    let popup = help_area(area);
    if popup.height == 0 || popup.width == 0 {
        return;
    }
    Clear.render(popup, buf);

    let border = Style::default().fg(app.theme.palette.accent);
    let block = Block::bordered().title(" Help ").style(app.theme.style(Element::HelpWindow)).border_style(border);
    let inner = block.inner(popup);
    block.render(popup, buf);

    // The overlay does not scroll: a terminal too short for the table clips it.
    let column = HELP.iter().map(|(key, _)| key.width()).max().unwrap_or(0) + HINT_GAP;
    for (row, (key, action)) in HELP.iter().take(inner.height as usize).enumerate() {
        let y = inner.y + row as u16;
        buf.set_stringn(inner.x, y, key, inner.width as usize, app.theme.style(Element::HeaderTitle));
        if column < inner.width as usize {
            let x = inner.x + column as u16;
            buf.set_stringn(x, y, action, (inner.right() - x) as usize, app.theme.style(Element::Hint));
        }
    }
}

/// 60% of the width, and a height clamped to 40-90%, centred. The shares are
/// worked out in `u32`: `height * 9` overflows a `u16` at 7282 rows. Both can
/// come out zero, on a terminal with no room for a popup; `help` draws nothing
/// rather than drawing outside itself.
fn help_area(area: Rect) -> Rect {
    let share = |whole: u16, (numerator, denominator): (u32, u32)| (u32::from(whole) * numerator / denominator) as u16;

    let width = share(area.width, HELP_WIDTH).min(area.width);
    let wanted = HELP.len() as u16 + 2;
    let height = wanted.clamp(share(area.height, HELP_FLOOR), share(area.height, HELP_CEILING)).min(area.height);

    Rect { x: area.x + (area.width - width) / 2, y: area.y + (area.height - height) / 2, width, height }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Size;

    use crate::document::Document;
    use crate::theme::Theme;
    use crate::ui::input::{Action, Motion};

    fn app(source: &str, size: Size) -> App {
        let document = Document::new(Some(PathBuf::from("notes/x.md")), PathBuf::from("notes"), source.to_string());
        App::new(document, Theme::default(), None, size)
    }

    /// One frame, as a buffer to assert against.
    fn frame(app: &App, size: Size) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(size.width, size.height)).expect("terminal");
        terminal.draw(|frame| draw(frame, app)).expect("draw");
        terminal.backend().buffer().clone()
    }

    /// One row of a buffer as plain text, trailing blanks trimmed.
    fn row(buffer: &Buffer, y: u16) -> String {
        let text: String = (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect();
        text.trim_end().to_string()
    }

    fn body() -> String {
        (1..=20).map(|n| format!("line {n}\n\n")).collect()
    }

    #[test]
    fn the_header_names_the_binary_and_the_document() {
        let size = Size::new(60, 12);
        let buffer = frame(&app(&body(), size), size);
        assert!(row(&buffer, 0).starts_with(" vademecum · x.md"), "{:?}", row(&buffer, 0));
    }

    #[test]
    fn the_hints_sit_against_the_right_edge() {
        let size = Size::new(60, 12);
        let buffer = frame(&app(&body(), size), size);
        assert!(row(&buffer, 0).ends_with(HINTS), "{:?}", row(&buffer, 0));
    }

    #[test]
    fn a_narrow_header_drops_the_hints_rather_than_wrapping_them() {
        let size = Size::new(20, 12);
        let buffer = frame(&app(&body(), size), size);
        assert!(!row(&buffer, 0).contains("? help"));
        assert!(row(&buffer, 0).contains("vademecum"));
        assert_eq!(row(&buffer, 1).chars().count(), 20, "the rule still spans the width");
    }

    #[test]
    fn a_hairline_rules_off_the_content_at_both_ends() {
        let size = Size::new(30, 12);
        let buffer = frame(&app(&body(), size), size);
        let hairline = symbols::line::HORIZONTAL.repeat(30);
        assert_eq!(row(&buffer, 1), hairline);
        assert_eq!(row(&buffer, size.height - 2), hairline);
    }

    #[test]
    fn the_statusbar_reads_the_file_the_line_and_the_share_read() {
        let size = Size::new(60, 12);
        let mut app = app(&body(), size);
        let buffer = frame(&app, size);
        assert_eq!(row(&buffer, 11), format!("x.md · line 1/{} · 0%", app.lines.len()));

        app.apply(Action::Move(Motion::Bottom));
        let buffer = frame(&app, size);
        let last = app.lines.len();
        assert_eq!(row(&buffer, 11), format!("x.md · line {last}/{last} · 100%"));
    }

    #[test]
    fn the_content_starts_at_the_top_of_the_document_and_scrolls_with_the_cursor() {
        let size = Size::new(60, 12);
        let mut app = app(&body(), size);
        let buffer = frame(&app, size);
        assert_eq!(row(&buffer, 2).trim(), "line 1");

        app.apply(Action::Move(Motion::Bottom));
        let buffer = frame(&app, size);
        assert_eq!(row(&buffer, size.height - 3).trim(), "line 20");
    }

    #[test]
    fn the_cursor_bar_reaches_the_last_column() {
        let size = Size::new(60, 12);
        let app = app(&body(), size);
        let buffer = frame(&app, size);
        let cursor_bg = app.theme.style(Element::CursorLine).bg.expect("the cursor line has a background");

        let cursor_row = 2;
        for x in 0..size.width {
            assert_eq!(buffer[(x, cursor_row)].bg, cursor_bg, "column {x}");
        }
    }

    #[test]
    fn only_the_cursor_line_carries_the_bar() {
        let size = Size::new(60, 12);
        let app = app(&body(), size);
        let buffer = frame(&app, size);
        let cursor_bg = app.theme.style(Element::CursorLine).bg.expect("background");
        assert_ne!(buffer[(0, 4)].bg, cursor_bg);
    }

    #[test]
    fn a_document_shorter_than_the_screen_leaves_the_rest_blank() {
        let size = Size::new(40, 12);
        let buffer = frame(&app("only line\n", size), size);
        assert_eq!(row(&buffer, 2).trim(), "only line");
        assert_eq!(row(&buffer, 5), "");
    }

    fn search_for(app: &mut App, query: &str) {
        app.apply(Action::SearchStart);
        for character in query.chars() {
            app.apply(Action::SearchType(character));
        }
        app.apply(Action::SearchConfirm);
    }

    #[test]
    fn the_prompt_shows_the_query_as_it_is_typed() {
        let size = Size::new(60, 12);
        let mut app = app(&body(), size);
        app.apply(Action::SearchStart);
        app.apply(Action::SearchType('l'));
        app.apply(Action::SearchType('i'));

        let buffer = frame(&app, size);
        assert_eq!(row(&buffer, 11), "/li");
    }

    #[test]
    fn the_prompt_outranks_a_notice_the_reader_has_not_dismissed() {
        let size = Size::new(60, 12);
        let mut app = app(&body(), size);
        search_for(&mut app, "absent");
        assert_eq!(row(&frame(&app, size), 11), "no matches for \"absent\"");

        app.apply(Action::SearchStart);
        assert_eq!(row(&frame(&app, size), 11), "/");
    }

    #[test]
    fn a_standing_query_puts_the_match_count_on_the_statusbar() {
        let size = Size::new(60, 12);
        let mut app = app(&body(), size);
        search_for(&mut app, "line 1");

        let status = row(&frame(&app, size), 11);
        assert!(status.contains(&format!("· match 1/{}", app.search.matches.len())), "{status:?}");
    }

    #[test]
    fn a_match_is_painted_over_and_the_current_one_differently() {
        let size = Size::new(60, 12);
        let mut app = app(&body(), size);
        search_for(&mut app, "line");

        let buffer = frame(&app, size);
        let current = app.theme.style(Element::SearchCurrent);
        let other = app.theme.style(Element::SearchMatch);
        assert_ne!(current.bg, other.bg, "the two are meant to be told apart");

        // " line 1" — the gutter is column 0, so the match starts at column 1.
        assert_eq!(buffer[(1, 2)].bg, current.bg.expect("a background"));
        // The second match is two rows down, past the blank line.
        assert_eq!(buffer[(1, 4)].bg, other.bg.expect("a background"));
    }

    #[test]
    fn the_overlay_covers_the_document_and_lists_the_bindings() {
        let size = Size::new(100, 24);
        let mut app = app(&body(), size);
        app.apply(Action::ToggleHelp);

        let buffer = frame(&app, size);
        let text: String = (0..size.height).map(|y| row(&buffer, y)).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Help"), "{text}");
        assert!(text.contains("Next / previous match"), "{text}");
        assert!(text.contains("q, Ctrl-C"), "{text}");
    }

    #[test]
    fn the_overlay_takes_three_fifths_of_the_width_and_sits_in_the_middle() {
        let area = Rect::new(0, 0, 100, 40);
        let popup = help_area(area);
        assert_eq!(popup.width, 60);
        assert_eq!(popup.x, 20, "centred");
        // The table wants eleven rows but the floor is 40% of forty.
        assert_eq!(popup.height, 16);
        assert_eq!(popup.y, 12, "centred");
    }

    #[test]
    fn the_overlay_is_clamped_rather_than_scrolled_on_a_short_terminal() {
        // Nine bindings plus a border want thirteen rows; 90% of twelve is ten.
        let popup = help_area(Rect::new(0, 0, 60, 12));
        assert_eq!(popup.height, 10);

        // And on a very tall one it is floored at 40%.
        let popup = help_area(Rect::new(0, 0, 60, 100));
        assert_eq!(popup.height, 40);
    }

    #[test]
    fn a_terminal_too_short_for_the_chrome_draws_what_it_can_and_does_not_panic() {
        // ratatui hands a zero-height rect a `y` one row past the buffer, so
        // every one of these used to abort the pager on the next frame.
        for height in 0..=6 {
            for width in [0, 1, 2, 3, 60] {
                let size = Size::new(width, height);
                let mut app = app(&body(), size);
                frame(&app, size);

                app.apply(Action::ToggleHelp);
                frame(&app, size);
            }
        }
    }

    #[test]
    fn the_overlay_draws_nothing_rather_than_outside_itself_when_there_is_no_room() {
        for height in 0..=2 {
            let popup = help_area(Rect::new(0, 0, 4, height));
            assert!(popup.bottom() <= height, "{popup:?} escapes a {height}-row terminal");
        }
    }

    #[test]
    fn a_very_tall_terminal_does_not_overflow_the_share_arithmetic() {
        // `height * 9` leaves a u16 at 7282 rows.
        let popup = help_area(Rect::new(0, 0, 300, 30000));
        assert!(popup.height <= 30000 && popup.height >= 12000);
    }

    #[test]
    fn an_empty_document_counts_no_lines_rather_than_one() {
        let size = Size::new(40, 12);
        let buffer = frame(&app("", size), size);
        assert!(row(&buffer, 11).starts_with("x.md · line 0/0"), "{:?}", row(&buffer, 11));
    }

    #[test]
    fn a_document_from_stdin_is_titled_as_such() {
        let size = Size::new(60, 12);
        let piped = Document::new(None, PathBuf::from("."), "hi\n".to_string());
        let app = App::new(piped, Theme::default(), None, size);
        let buffer = frame(&app, size);
        assert!(row(&buffer, 0).starts_with(" vademecum · stdin"));
        assert!(row(&buffer, 11).starts_with("stdin · line 1/1"));
    }
}
