//! `RenderedLine` → the bytes that go to stdout.
//!
//! SGR sequences are written by hand rather than through crossterm so the
//! output is exactly reproducible, which is what the snapshot tests compare.

use std::io::{self, Write};

use ratatui::style::{Color, Modifier, Style};

use crate::markdown::links::LinkKind;
use crate::render::line::RenderedLine;

/// Write laid-out lines, with or without color.
pub fn write_lines(out: &mut impl Write, lines: &[RenderedLine], color: bool) -> io::Result<()> {
    for line in lines {
        if color {
            write_colored(out, line)?;
        } else {
            writeln!(out, "{}", line.text().trim_end())?;
        }
    }
    Ok(())
}

fn write_colored(out: &mut impl Write, line: &RenderedLine) -> io::Result<()> {
    let mut styled = false;

    for (index, span) in line.spans.iter().enumerate() {
        if let Some(url) = link_opening_at(line, index) {
            write!(out, "\x1b]8;;{url}\x1b\\")?;
        }

        let codes = sgr(&span.style);
        if codes.is_empty() {
            if styled {
                out.write_all(b"\x1b[0m")?;
                styled = false;
            }
        } else {
            write!(out, "\x1b[0;{codes}m")?;
            styled = true;
        }

        out.write_all(span.text.as_bytes())?;

        if link_closing_at(line, index) {
            out.write_all(b"\x1b]8;;\x1b\\")?;
        }
    }

    // A reset on every line keeps `less -R` and a truncated pipe honest.
    if styled {
        out.write_all(b"\x1b[0m")?;
    }
    out.write_all(b"\n")
}

/// The URL to open a hyperlink with at this span, if one starts here. Only
/// external links become OSC-8: a local path means nothing to the terminal.
fn link_opening_at(line: &RenderedLine, index: usize) -> Option<&str> {
    line.links.iter().find_map(|link| match &link.kind {
        LinkKind::External(url) if link.span_range.start == index => Some(url.as_str()),
        _ => None,
    })
}

fn link_closing_at(line: &RenderedLine, index: usize) -> bool {
    line.links.iter().any(|link| matches!(link.kind, LinkKind::External(_)) && link.span_range.end == index + 1)
}

/// The SGR parameters for a style, without the leading reset or the `m`.
fn sgr(style: &Style) -> String {
    let mut codes: Vec<String> = Vec::new();

    if let Some(color) = style.fg {
        codes.push(color_codes(color, false));
    }
    if let Some(color) = style.bg {
        codes.push(color_codes(color, true));
    }
    for (modifier, code) in [
        (Modifier::BOLD, 1),
        (Modifier::DIM, 2),
        (Modifier::ITALIC, 3),
        (Modifier::UNDERLINED, 4),
        (Modifier::SLOW_BLINK, 5),
        (Modifier::RAPID_BLINK, 6),
        (Modifier::REVERSED, 7),
        (Modifier::HIDDEN, 8),
        (Modifier::CROSSED_OUT, 9),
    ] {
        if style.add_modifier.contains(modifier) {
            codes.push(code.to_string());
        }
    }

    codes.join(";")
}

/// One color as SGR parameters, as a foreground or a background.
fn color_codes(color: Color, background: bool) -> String {
    let offset = if background { 10 } else { 0 };
    let named = |base: u8| (base + offset).to_string();

    match color {
        Color::Reset => named(39),
        Color::Black => named(30),
        Color::Red => named(31),
        Color::Green => named(32),
        Color::Yellow => named(33),
        Color::Blue => named(34),
        Color::Magenta => named(35),
        Color::Cyan => named(36),
        Color::Gray => named(37),
        Color::DarkGray => named(90),
        Color::LightRed => named(91),
        Color::LightGreen => named(92),
        Color::LightYellow => named(93),
        Color::LightBlue => named(94),
        Color::LightMagenta => named(95),
        Color::LightCyan => named(96),
        Color::White => named(97),
        Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", 38 + offset),
        Color::Indexed(index) => format!("{};5;{index}", 38 + offset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::links::Url;
    use crate::render::line::{LinkRef, StyledSpan};

    fn write(lines: &[RenderedLine], color: bool) -> String {
        let mut out = Vec::new();
        write_lines(&mut out, lines, color).expect("writing to a Vec cannot fail");
        String::from_utf8(out).expect("output is utf-8")
    }

    fn line(spans: Vec<StyledSpan>) -> RenderedLine {
        RenderedLine { spans, ..RenderedLine::default() }
    }

    #[test]
    fn without_color_the_output_has_no_escapes() {
        let styled = line(vec![StyledSpan::new("hi", Style::default().fg(Color::Red))]);
        let out = write(&[styled], false);
        assert_eq!(out, "hi\n");
        assert!(!out.contains('\x1b'));
    }

    #[test]
    fn without_color_trailing_padding_is_dropped() {
        let padded = line(vec![StyledSpan::new("code    ", Style::default())]);
        assert_eq!(write(&[padded], false), "code\n");
    }

    #[test]
    fn truecolor_is_written_as_38_2() {
        let styled = line(vec![StyledSpan::new("hi", Style::default().fg(Color::Rgb(0x56, 0xD0, 0xB3)))]);
        assert_eq!(write(&[styled], true), "\x1b[0;38;2;86;208;179mhi\x1b[0m\n");
    }

    #[test]
    fn indexed_and_named_colors_have_their_own_forms() {
        assert_eq!(color_codes(Color::Indexed(208), false), "38;5;208");
        assert_eq!(color_codes(Color::Indexed(208), true), "48;5;208");
        assert_eq!(color_codes(Color::Blue, false), "34");
        assert_eq!(color_codes(Color::Blue, true), "44");
        assert_eq!(color_codes(Color::DarkGray, false), "90");
        assert_eq!(color_codes(Color::Reset, true), "49");
    }

    #[test]
    fn modifiers_follow_the_colors() {
        let style = Style::default().fg(Color::Blue).bg(Color::Black).add_modifier(Modifier::BOLD | Modifier::ITALIC);
        assert_eq!(sgr(&style), "34;40;1;3");
    }

    #[test]
    fn an_unstyled_span_resets_what_came_before_it() {
        let mixed = line(vec![
            StyledSpan::new("bold", Style::default().add_modifier(Modifier::BOLD)),
            StyledSpan::new(" plain", Style::default()),
        ]);
        assert_eq!(write(&[mixed], true), "\x1b[0;1mbold\x1b[0m plain\n");
    }

    #[test]
    fn every_styled_line_ends_reset() {
        let styled = line(vec![StyledSpan::new("hi", Style::default().add_modifier(Modifier::BOLD))]);
        assert!(write(&[styled], true).ends_with("\x1b[0m\n"));
    }

    #[test]
    fn external_links_become_osc_8_hyperlinks() {
        let mut linked = line(vec![StyledSpan::new("docs", Style::default())]);
        linked.links.push(LinkRef {
            span_range: 0..1,
            kind: LinkKind::External(Url::from("https://example.com")),
            resolved: None,
        });
        assert_eq!(write(&[linked], true), "\x1b]8;;https://example.com\x1b\\docs\x1b]8;;\x1b\\\n");
    }

    #[test]
    fn local_links_are_not_hyperlinked() {
        let mut linked = line(vec![StyledSpan::new("note", Style::default())]);
        linked.links.push(LinkRef {
            span_range: 0..1,
            kind: LinkKind::Wiki { target: "note".into(), fragment: None },
            resolved: None,
        });
        assert_eq!(write(&[linked], true), "note\n");
    }

    #[test]
    fn blank_lines_stay_blank() {
        assert_eq!(write(&[RenderedLine::blank()], true), "\n");
        assert_eq!(write(&[RenderedLine::blank()], false), "\n");
    }
}
