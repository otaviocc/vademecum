//! Painting one frame: header, hairline rules, content, statusbar.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Clear, Widget};
use ratatui::{Frame, symbols};
use unicode_width::UnicodeWidthStr;

use crate::render::line::RenderedLine;
use crate::theme::Element;
use crate::ui::app::{App, Mode, Status};

const HINTS: &str = "? help  / search  ⇥ link  ⏎ follow  h/l back/fwd  q quit";
const TITLE_PREFIX: &str = " vademecum · ";
const HINT_GAP: usize = 2;
const HELP_WIDTH: (u32, u32) = (3, 5);
const HELP_FLOOR: (u32, u32) = (4, 10);
const HELP_CEILING: (u32, u32) = (9, 10);

const HELP: &[(&str, &str)] = &[
    ("j / k, ↓ / ↑", "Move cursor line down / up"),
    ("d / u, Ctrl-D / Ctrl-U", "Half page down / up"),
    ("Space / b, PgDn / PgUp", "Page down / up"),
    ("g / G, Home / End", "Top / bottom"),
    ("Tab / Shift-Tab", "Cycle link focus on the cursor line"),
    ("Enter", "Follow focused local/wiki link"),
    ("o", "Open focused external link in the browser"),
    ("h / Backspace, l", "History back, forward"),
    ("/", "Search (Enter confirms, Esc cancels)"),
    ("n / N", "Next / previous match"),
    ("?", "Help overlay"),
    ("Esc", "Close overlay, clear search highlight"),
    ("q, Ctrl-C", "Quit"),
    ("Left click", "Follow the link under the pointer"),
];

pub fn draw(frame: &mut Frame, app: &App) {
    frame.render_widget(Screen { app }, frame.area());
}

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

fn row(area: Rect, buf: &mut Buffer, x: u16, text: &str, style: Style) {
    if area.height == 0 || x >= area.right() {
        return;
    }
    buf.set_stringn(x, area.y, text, (area.right() - x) as usize, style);
}

fn header(area: Rect, buf: &mut Buffer, app: &App) {
    let title = format!("{TITLE_PREFIX}{}", app.title);
    row(area, buf, area.x, &title, app.theme.style(Element::HeaderTitle));

    let hints = HINTS.width();
    if title.width() + HINT_GAP + hints <= area.width as usize {
        row(area, buf, area.right() - hints as u16, HINTS, app.theme.style(Element::Hint));
    }
}

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

        if on_cursor {
            buf.set_style(Rect::new(area.x, y, area.width, 1), cursor_line);
        }

        let focused = on_cursor.then(|| line.links.get(app.focus).map(|link| link.span_range.clone())).flatten();

        let mut x = area.x;
        for (index_of_span, span) in line.spans.iter().enumerate() {
            if span.text.is_empty() {
                continue;
            }
            if x >= area.right() {
                break;
            }
            let mut style = if on_cursor { span.style.patch(cursor_line) } else { span.style };
            if focused.as_ref().is_some_and(|range| range.contains(&index_of_span)) {
                style = style.patch(app.theme.style(Element::LinkFocused));
            }
            let (next, _) = buf.set_stringn(x, y, &span.text, (area.right() - x) as usize, style);
            if next == x {
                break;
            }
            x = next;
        }

        highlight(area, buf, y, app, index, line);
    }
}

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

fn statusbar(area: Rect, buf: &mut Buffer, app: &App) {
    let (text, element) = match (app.mode, &app.status) {
        (Mode::Search, _) => (format!("/{}", app.search.input), Element::Status),
        (_, Status::Error(error)) => (error.clone(), Element::StatusError),
        (_, Status::Notice(notice)) => (notice.clone(), Element::StatusNotice),
        (_, Status::Idle) => {
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
    use crate::ui::Options;
    use crate::ui::app::CONTENT_TOP;
    use crate::ui::input::{Action, Motion};

    fn app(source: &str, size: Size) -> App {
        let document = Document::new(Some(PathBuf::from("notes/x.md")), PathBuf::from("notes"), source.to_string());
        App::new(document, Theme::default(), &Options { width: None, ..Options::default() }, size)
    }

    fn frame(app: &App, size: Size) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(size.width, size.height)).expect("terminal");
        terminal.draw(|frame| draw(frame, app)).expect("draw");
        terminal.backend().buffer().clone()
    }

    fn row(buffer: &Buffer, y: u16) -> String {
        let mut text = String::new();
        let mut x = 0;
        while x < buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            text.push_str(symbol);
            x += (symbol.width() as u16).max(1);
        }
        text.trim_end().to_string()
    }

    #[test]
    fn the_content_pane_starts_where_the_reducer_thinks_it_does() {
        let app = app("first line\n", Size::new(40, 12));
        let buffer = frame(&app, Size::new(40, 12));
        assert_eq!(row(&buffer, CONTENT_TOP).trim(), "first line");
        assert_eq!(row(&buffer, CONTENT_TOP - 1), symbols::line::HORIZONTAL.repeat(40));
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
        let size = Size::new(90, 12);
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

        assert_eq!(buffer[(1, 2)].bg, current.bg.expect("a background"));
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
        assert_eq!(popup.height, 16);
        assert_eq!(popup.y, 12, "centred");
    }

    #[test]
    fn the_overlay_is_clamped_rather_than_scrolled_on_a_short_terminal() {
        let popup = help_area(Rect::new(0, 0, 60, 12));
        assert_eq!(popup.height, 10);

        let popup = help_area(Rect::new(0, 0, 60, 100));
        assert_eq!(popup.height, 40);
    }

    #[test]
    fn a_terminal_too_short_for_the_chrome_draws_what_it_can_and_does_not_panic() {
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
    fn the_pager_paints_every_line_the_stdout_writer_would() {
        let source = include_str!("../../tests/fixtures/elements.md");
        let width = 82;
        let measured = app(source, Size::new(width, u16::MAX)).lines.len();
        let size =
            Size::new(width, u16::try_from(measured).expect("the fixture is not 65535 lines") + crate::ui::app::CHROME_ROWS);
        let app = app(source, size);
        assert!(app.lines.len() <= app.viewport_height(), "the whole fixture has to be on screen");

        let buffer = frame(&app, size);
        for (offset, line) in app.lines.iter().enumerate() {
            let painted = row(&buffer, CONTENT_TOP + offset as u16);
            assert_eq!(painted, line.text().trim_end(), "line {offset}");
        }
    }

    #[test]
    fn a_document_from_stdin_is_titled_as_such() {
        let size = Size::new(60, 12);
        let piped = Document::new(None, PathBuf::from("."), "hi\n".to_string());
        let app = App::new(piped, Theme::default(), &Options { width: None, ..Options::default() }, size);
        let buffer = frame(&app, size);
        assert!(row(&buffer, 0).starts_with(" vademecum · stdin"));
        assert!(row(&buffer, 11).starts_with("stdin · line 1/1"));
    }

    #[test]
    fn the_focused_link_is_painted_and_the_others_are_not() {
        let size = Size::new(60, 12);
        let mut app = app("[one](a.md) and [two](b.md)\n", size);
        let selection = app.theme.style(Element::LinkFocused);

        let wanted = selection.bg.expect("the focused link has a background");
        let painted = |app: &App| -> Vec<ratatui::style::Color> {
            let buffer = frame(app, size);
            (0..size.width).map(|x| buffer[(x, CONTENT_TOP)].bg).collect()
        };

        let (first, second) = (1..4, 9..12);
        let before = painted(&app);
        assert!(before[first.clone()].iter().all(|bg| *bg == wanted), "the only link on the line is focused");
        assert!(!before[second.clone()].contains(&wanted), "the second one is not");

        app.apply(Action::Focus { forward: true });
        let after = painted(&app);
        assert!(after[second].iter().all(|bg| *bg == wanted), "Tab moved it along");
        assert!(!after[first].contains(&wanted), "and off the first");
    }

    #[test]
    fn a_link_off_the_cursor_line_is_never_focused() {
        let size = Size::new(60, 12);
        let mut app = app("[one](a.md)\n\n[two](b.md)\n", size);
        app.apply(Action::Move(Motion::Bottom));

        let buffer = frame(&app, size);
        let selection = app.theme.style(Element::LinkFocused).bg.expect("a background");
        assert!((0..size.width).all(|x| buffer[(x, CONTENT_TOP)].bg != selection), "the first line is not the cursor line");
    }

    #[test]
    fn an_error_outranks_a_notice_in_the_statusbar() {
        let size = Size::new(60, 12);
        let mut app = app(&body(), size);
        let status = size.height - 1;

        app.status = Status::Notice(String::from("a notice"));
        assert_eq!(row(&frame(&app, size), status), "a notice");

        app.status = Status::Error(String::from("note.md: not found"));
        let buffer = frame(&app, size);
        assert_eq!(row(&buffer, status), "note.md: not found");
        assert_eq!(buffer[(0, status)].fg, app.theme.style(Element::StatusError).fg.expect("a foreground"));
    }

    #[test]
    fn the_help_overlay_lists_every_binding_the_readme_names() {
        for key in ["Tab / Shift-Tab", "Enter", "o", "h / Backspace, l"] {
            assert!(HELP.iter().any(|(row, _)| *row == key), "{key} is not in the help table");
        }
    }
}
