//! Painting one frame: header, hairline rules, content, statusbar.
//!
//! The content area is drawn a cell at a time from the visible slice of
//! `App::lines`, never through a widget built over the whole document. That is
//! what keeps a frame's cost proportional to the terminal rather than to the
//! file: a hundred-thousand-line note paints exactly as fast as a short one.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Widget;
use ratatui::{Frame, symbols};
use unicode_width::UnicodeWidthStr;

use crate::theme::Element;
use crate::ui::app::App;

/// The shortcut hints, right-aligned in the header. They list the bindings
/// this build has, and grow as milestones land.
const HINTS: &str = "? help  q quit";
/// Ahead of the title, so the reader can see what they are running.
const TITLE_PREFIX: &str = " vademecum · ";
/// One column between the title and the hints before the hints give way.
const HINT_GAP: usize = 2;

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
    }
}

/// ` vademecum · <title>` on the left, the hints on the right. When the two
/// would collide the hints give way: the title is what says which document
/// this is.
fn header(area: Rect, buf: &mut Buffer, app: &App) {
    let title = format!("{TITLE_PREFIX}{}", app.title);
    buf.set_stringn(area.x, area.y, &title, area.width as usize, app.theme.style(Element::HeaderTitle));

    let hints = HINTS.width();
    if title.width() + HINT_GAP + hints <= area.width as usize {
        let x = area.right() - hints as u16;
        buf.set_stringn(x, area.y, HINTS, hints, app.theme.style(Element::Hint));
    }
}

/// A hairline across the full width.
fn rule(area: Rect, buf: &mut Buffer, style: Style) {
    let line = symbols::line::HORIZONTAL.repeat(area.width as usize);
    buf.set_stringn(area.x, area.y, &line, area.width as usize, style);
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
    }
}

/// `file · line X/Y · N%`.
fn statusbar(area: Rect, buf: &mut Buffer, app: &App) {
    let status = format!("{} · line {}/{} · {}%", app.file, app.cursor + 1, app.lines.len(), app.percent());
    buf.set_stringn(area.x, area.y, &status, area.width as usize, app.theme.style(Element::Status));
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
