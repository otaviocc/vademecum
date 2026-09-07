//! Block tree → `Vec<RenderedLine>` at a given width.

use std::path::PathBuf;

use ratatui::style::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::markdown::ast::{Alignment, Block, Inline, ListItem, SourceBlock, plain_text};
use crate::markdown::links::{LinkKind, Links, ResolveError, Target};
use crate::render::code;
use crate::render::line::{self, LinkRef, RenderedLine, StyledSpan};
use crate::theme::{Element, Theme};

pub const GUTTER: usize = 1;
const CODE_PAD: usize = 1;
const DEFAULT_WIDTH: usize = 100;
const TERMINAL_MARGIN: usize = 2;
const BULLETS: [&str; 3] = ["•", "◦", "▪"];

pub fn wrap_width(explicit: Option<u16>, columns: Option<u16>) -> usize {
    if let Some(width) = explicit {
        return usize::from(width).max(1);
    }
    match columns {
        Some(columns) => usize::from(columns).saturating_sub(TERMINAL_MARGIN).clamp(1, DEFAULT_WIDTH),
        None => DEFAULT_WIDTH,
    }
}

pub struct Ctx<'a> {
    pub theme: &'a Theme,
    pub links: &'a Links,
}

impl<'a> Ctx<'a> {
    pub fn new(theme: &'a Theme, links: &'a Links) -> Self {
        Self { theme, links }
    }
}

pub fn render(blocks: &[SourceBlock], ctx: &Ctx<'_>, width: usize) -> Vec<RenderedLine> {
    let content_width = width.saturating_sub(GUTTER).max(1);
    let mut lines = blocks_to_lines(blocks, ctx, ctx.theme.style(Element::Paragraph), content_width, 0);

    let gutter = StyledSpan::new(" ".repeat(GUTTER), Style::default());
    for line in &mut lines {
        if !line.is_blank() {
            line.prefix(gutter.clone());
            line.inset = line.inset.max(GUTTER);
        }
        clamp_to_width(line, width);
    }
    lines
}

fn clamp_to_width(line: &mut RenderedLine, width: usize) {
    if line.width() <= width {
        return;
    }

    let budget = width.saturating_sub(1);
    let mut used = 0;
    let mut spans: Vec<StyledSpan> = Vec::new();
    let mut cut_style = Style::default();
    for span in std::mem::take(&mut line.spans) {
        let span_width = span.width();
        if used + span_width <= budget {
            used += span_width;
            spans.push(span);
            continue;
        }
        cut_style = span.style;
        let (head, _) = split_at_width(&span.text, budget.saturating_sub(used));
        if !head.is_empty() {
            spans.push(StyledSpan::new(head, cut_style));
        }
        break;
    }

    let kept = spans.len();
    line.links.retain(|link| link.span_range.end <= kept);
    spans.push(StyledSpan::new("…", cut_style));
    line.spans = spans;
}

fn blocks_to_lines(blocks: &[SourceBlock], ctx: &Ctx<'_>, base: Style, width: usize, depth: usize) -> Vec<RenderedLine> {
    let mut lines: Vec<RenderedLine> = Vec::new();
    for block in blocks {
        if !lines.is_empty() {
            lines.push(RenderedLine::blank());
        }
        lines.extend(block_to_lines(block, ctx, base, width, depth));
    }
    lines
}

fn block_to_lines(block: &SourceBlock, ctx: &Ctx<'_>, base: Style, width: usize, depth: usize) -> Vec<RenderedLine> {
    let line = block.line;
    match &block.block {
        Block::Paragraph(inlines) => wrap_inlines(inlines, ctx, base, width, line),
        Block::Heading { level, inlines, anchor } => {
            let style = ctx.theme.style(Element::heading(*level));
            let mut lines = wrap_inlines(inlines, ctx, style, width, line);
            if let Some(first) = lines.first_mut() {
                first.anchor = Some(anchor.clone());
            }
            lines
        }
        Block::Quote(blocks) => quote_to_lines(blocks, ctx, base, width, depth),
        Block::List { ordered, items } => list_to_lines(*ordered, items, ctx, base, width, depth),
        Block::CodeBlock { lang, text } => code_to_lines(lang.as_deref(), text, ctx, width, line),
        Block::Table { header, rows, alignments } => table_to_lines(header, rows, alignments, ctx, width, line),
        Block::Rule => vec![styled_line("─".repeat(width), ctx.theme.style(Element::Hr), line)],
        Block::Html(text) => {
            text.lines().map(|html| styled_line(truncate(html, width), ctx.theme.style(Element::Html), line)).collect()
        }
        Block::FootnoteDef { label, blocks } => footnote_to_lines(label, blocks, ctx, base, width, depth, line),
    }
}

fn quote_to_lines(blocks: &[SourceBlock], ctx: &Ctx<'_>, base: Style, width: usize, depth: usize) -> Vec<RenderedLine> {
    let gutter = StyledSpan::new("┃ ", Style::default().fg(ctx.theme.palette.muted));
    let quoted = base.patch(ctx.theme.style(Element::Quote));

    let mut lines = blocks_to_lines(blocks, ctx, quoted, width.saturating_sub(gutter.width()).max(1), depth);
    for line in &mut lines {
        line.prefix(gutter.clone());
    }
    lines
}

fn list_to_lines(
    ordered: Option<u64>,
    items: &[ListItem],
    ctx: &Ctx<'_>,
    base: Style,
    width: usize,
    depth: usize,
) -> Vec<RenderedLine> {
    let mut lines = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let (marker, style) = marker_for(item, ordered, index, ctx, depth);
        let indent = marker.width();
        let mut blocks = item.blocks.clone();
        strip_task_marker(&mut blocks);

        let mut item_lines = item_to_lines(&blocks, ctx, base, width.saturating_sub(indent).max(1), depth + 1);
        if item_lines.is_empty() {
            item_lines.push(RenderedLine::blank());
        }
        for (offset, line) in item_lines.iter_mut().enumerate() {
            let prefix = if offset == 0 {
                StyledSpan::new(marker.clone(), style)
            } else {
                StyledSpan::new(" ".repeat(indent), Style::default())
            };
            line.prefix(prefix);
        }
        lines.extend(item_lines);
    }
    lines
}

fn item_to_lines(blocks: &[SourceBlock], ctx: &Ctx<'_>, base: Style, width: usize, depth: usize) -> Vec<RenderedLine> {
    let mut lines: Vec<RenderedLine> = Vec::new();
    for block in blocks {
        if !lines.is_empty() && !matches!(block.block, Block::List { .. }) {
            lines.push(RenderedLine::blank());
        }
        lines.extend(block_to_lines(block, ctx, base, width, depth));
    }
    lines
}

fn marker_for(item: &ListItem, ordered: Option<u64>, index: usize, ctx: &Ctx<'_>, depth: usize) -> (String, Style) {
    match (item.task(), ordered) {
        (Some(true), _) => ("☑ ".to_string(), ctx.theme.style(Element::TaskDone)),
        (Some(false), _) => ("☐ ".to_string(), ctx.theme.style(Element::TaskTodo)),
        (None, Some(start)) => {
            let number = start.saturating_add(index as u64);
            (format!("{number}. "), ctx.theme.style(Element::ListNumber))
        }
        (None, None) => (format!("{} ", BULLETS[depth % BULLETS.len()]), ctx.theme.style(Element::ListBullet)),
    }
}

fn strip_task_marker(blocks: &mut [SourceBlock]) {
    if let Some(SourceBlock { block: Block::Paragraph(inlines), .. }) = blocks.first_mut()
        && matches!(inlines.first(), Some(Inline::TaskMarker(_)))
    {
        inlines.remove(0);
        if let Some(Inline::Text(text)) = inlines.first_mut() {
            let trimmed = text.trim_start().to_string();
            *text = trimmed;
        }
    }
}

fn code_to_lines(lang: Option<&str>, text: &str, ctx: &Ctx<'_>, width: usize, source_line: usize) -> Vec<RenderedLine> {
    let block = ctx.theme.style(Element::CodeBlock);
    let mut lines = vec![fence_line(lang, ctx, width, source_line)];

    for (offset, code) in code::highlight(lang, text, ctx.theme.syntax_theme.as_deref()).iter().enumerate() {
        lines.push(code_line(code, block, width, source_line + offset + 1));
    }

    lines.push(styled_line(" ".repeat(width), block, source_line));

    let pad = code_pad(width);
    for line in &mut lines {
        line.inset = pad;
    }
    lines
}

fn code_pad(width: usize) -> usize {
    if width > 2 * CODE_PAD + 1 { CODE_PAD } else { 0 }
}

fn code_line(code: &[StyledSpan], block: Style, width: usize, source_line: usize) -> RenderedLine {
    let pad = code_pad(width);
    let room = width - 2 * pad;
    let total: usize = code.iter().map(StyledSpan::width).sum();
    let cut = total > room;
    let budget = if cut { room - 1 } else { room };

    let mut spans: Vec<StyledSpan> = Vec::with_capacity(code.len() + 2);
    spans.push(StyledSpan::new(" ".repeat(pad), block));
    let mut used = 0;
    for span in code {
        if used >= budget {
            break;
        }
        let (head, tail) = split_at_width(&span.text, budget - used);
        if !head.is_empty() {
            used += head.width();
            spans.push(StyledSpan::new(head, block.patch(span.style)));
        }
        if !tail.is_empty() {
            break;
        }
    }

    if cut {
        let style = spans.last().map_or(block, |span| span.style);
        spans.push(StyledSpan::new("…", style));
        used += 1;
    }
    spans.push(StyledSpan::new(" ".repeat(width - pad - used), block));

    RenderedLine { spans: line::merge(spans), source_line, ..RenderedLine::default() }
}

fn fence_line(lang: Option<&str>, ctx: &Ctx<'_>, width: usize, source_line: usize) -> RenderedLine {
    let block = ctx.theme.style(Element::CodeBlock);
    let pad = code_pad(width);
    let Some(lang) = lang.filter(|lang| lang.width() + pad <= width) else {
        return styled_line(" ".repeat(width), block, source_line);
    };

    let padding = width - lang.width() - pad;
    RenderedLine {
        spans: vec![
            StyledSpan::new(" ".repeat(padding), block),
            StyledSpan::new(lang, ctx.theme.style(Element::CodeBlockLang)),
            StyledSpan::new(" ".repeat(pad), block),
        ],
        source_line,
        ..RenderedLine::default()
    }
}

fn footnote_to_lines(
    label: &str,
    blocks: &[SourceBlock],
    ctx: &Ctx<'_>,
    base: Style,
    width: usize,
    depth: usize,
    source_line: usize,
) -> Vec<RenderedLine> {
    let marker = format!("[^{label}] ");
    let indent = marker.width();
    let style = ctx.theme.style(Element::Footnote);

    let mut lines = blocks_to_lines(blocks, ctx, base, width.saturating_sub(indent).max(1), depth);
    if lines.is_empty() {
        lines.push(RenderedLine { source_line, ..RenderedLine::default() });
    }
    for (offset, line) in lines.iter_mut().enumerate() {
        let prefix = if offset == 0 {
            StyledSpan::new(marker.clone(), style)
        } else {
            StyledSpan::new(" ".repeat(indent), Style::default())
        };
        line.prefix(prefix);
    }
    lines
}

fn table_to_lines(
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    alignments: &[Alignment],
    ctx: &Ctx<'_>,
    width: usize,
    source_line: usize,
) -> Vec<RenderedLine> {
    let columns = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if columns == 0 {
        return Vec::new();
    }
    let widths = column_widths(header, rows, columns, width);
    let border = ctx.theme.style(Element::TableBorder);
    let align = |column: usize| alignments.get(column).copied().unwrap_or(Alignment::None);

    let mut lines = vec![rule_line("┌", "┬", "┐", &widths, border, source_line)];
    if !header.is_empty() {
        lines.extend(row_to_lines(header, &widths, ctx, ctx.theme.style(Element::TableHeader), &align, source_line));
        lines.push(rule_line("├", "┼", "┤", &widths, border, source_line));
    }
    for row in rows {
        lines.extend(row_to_lines(row, &widths, ctx, ctx.theme.style(Element::Paragraph), &align, source_line));
    }
    lines.push(rule_line("└", "┴", "┘", &widths, border, source_line));
    lines
}

fn column_widths(header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>], columns: usize, width: usize) -> Vec<usize> {
    const MIN: usize = 3;

    let mut widths = vec![0; columns];
    for row in std::iter::once(header).chain(rows.iter().map(Vec::as_slice)) {
        for (column, cell) in row.iter().enumerate().take(columns) {
            widths[column] = widths[column].max(plain_text(cell).width());
        }
    }

    let furniture = 3 * columns + 1;
    let natural: usize = widths.iter().sum();
    let available = width.saturating_sub(furniture);
    if natural <= available {
        return widths;
    }

    let floor = MIN.min(available / columns).max(1);
    let mut shrunk: Vec<usize> = widths.iter().map(|w| (w * available / natural.max(1)).max(floor)).collect();

    let mut slack = available.saturating_sub(shrunk.iter().sum::<usize>());
    for (column, target) in shrunk.iter_mut().enumerate() {
        let room = widths[column].saturating_sub(*target).min(slack);
        *target += room;
        slack -= room;
    }

    let mut total: usize = shrunk.iter().sum();
    while total > available {
        let widest = shrunk.iter().enumerate().filter(|(_, width)| **width > floor).max_by_key(|(_, width)| **width);
        let Some((column, _)) = widest.map(|(column, width)| (column, *width)) else { break };
        shrunk[column] -= 1;
        total -= 1;
    }
    shrunk
}

fn row_to_lines(
    cells: &[Vec<Inline>],
    widths: &[usize],
    ctx: &Ctx<'_>,
    style: Style,
    align: &impl Fn(usize) -> Alignment,
    source_line: usize,
) -> Vec<RenderedLine> {
    let border = ctx.theme.style(Element::TableBorder);
    let wrapped: Vec<Vec<RenderedLine>> = widths
        .iter()
        .enumerate()
        .map(|(column, width)| {
            let inlines = cells.get(column).map(Vec::as_slice).unwrap_or(&[]);
            wrap_inlines(inlines, ctx, style, *width, source_line)
        })
        .collect();

    let height = wrapped.iter().map(Vec::len).max().unwrap_or(1).max(1);
    (0..height)
        .map(|offset| {
            let mut line = RenderedLine { source_line, ..RenderedLine::default() };
            for (column, width) in widths.iter().enumerate() {
                line.push(StyledSpan::new("│ ", border));
                let cell = wrapped[column].get(offset).cloned().unwrap_or_default();
                let padding = width.saturating_sub(cell.width());
                let (before, after) = pad_split(align(column), padding);
                line.push(StyledSpan::new(" ".repeat(before), style));
                line.append(cell);
                line.push(StyledSpan::new(" ".repeat(after), style));
                line.push(StyledSpan::new(" ", border));
            }
            line.push(StyledSpan::new("│", border));
            line
        })
        .collect()
}

fn pad_split(alignment: Alignment, padding: usize) -> (usize, usize) {
    match alignment {
        Alignment::Right => (padding, 0),
        Alignment::Center => (padding / 2, padding - padding / 2),
        Alignment::None | Alignment::Left => (0, padding),
    }
}

fn rule_line(left: &str, join: &str, right: &str, widths: &[usize], style: Style, source_line: usize) -> RenderedLine {
    let mut text = String::from(left);
    for (column, width) in widths.iter().enumerate() {
        if column > 0 {
            text.push_str(join);
        }
        text.push_str(&"─".repeat(width + 2));
    }
    text.push_str(right);
    styled_line(text, style, source_line)
}

fn wrap_inlines(inlines: &[Inline], ctx: &Ctx<'_>, base: Style, width: usize, source_line: usize) -> Vec<RenderedLine> {
    let mut flat = Flat::default();
    flat.push_inlines(inlines, ctx, base, None);
    Wrapper::new(width, source_line, &flat.links).run(&flat.pieces)
}

struct Piece {
    text: String,
    style: Style,
    link: Option<usize>,
    hard_break: bool,
}

struct Resolved {
    kind: LinkKind,
    target: Option<PathBuf>,
}

fn file_of(target: Result<Target, ResolveError>) -> Option<PathBuf> {
    match target {
        Ok(Target::File(path)) => Some(path),
        _ => None,
    }
}

#[derive(Default)]
struct Flat {
    pieces: Vec<Piece>,
    links: Vec<Resolved>,
}

impl Flat {
    fn push_inlines(&mut self, inlines: &[Inline], ctx: &Ctx<'_>, base: Style, link: Option<usize>) {
        for inline in inlines {
            self.push_inline(inline, ctx, base, link);
        }
    }

    fn push_inline(&mut self, inline: &Inline, ctx: &Ctx<'_>, base: Style, link: Option<usize>) {
        let text = |flat: &mut Self, text: String, style: Style| {
            flat.pieces.push(Piece { text, style, link, hard_break: false });
        };

        match inline {
            Inline::Text(value) => text(self, value.clone(), base),
            Inline::Code(value) => text(self, value.clone(), base.patch(ctx.theme.style(Element::InlineCode))),
            Inline::Html(value) => text(self, value.clone(), base.patch(ctx.theme.style(Element::Html))),
            Inline::Image { alt, .. } => text(self, format!("[image: {alt}]"), base.patch(ctx.theme.style(Element::Image))),
            Inline::FootnoteRef(label) => text(self, format!("[^{label}]"), base.patch(ctx.theme.style(Element::Footnote))),
            Inline::SoftBreak => text(self, " ".to_string(), base),
            Inline::Emphasis(children) => self.push_inlines(children, ctx, base.patch(ctx.theme.style(Element::Emphasis)), link),
            Inline::Strong(children) => self.push_inlines(children, ctx, base.patch(ctx.theme.style(Element::Strong)), link),
            Inline::Strike(children) => {
                self.push_inlines(children, ctx, base.patch(ctx.theme.style(Element::Strikethrough)), link)
            }
            Inline::Link { kind, inlines } => {
                let target = ctx.links.resolve(kind);
                let element = match (kind, &target) {
                    (LinkKind::External(_), _) => Element::Link,
                    (_, Err(_)) => Element::LinkBroken,
                    _ => Element::Wikilink,
                };
                let id = self.links.len();
                self.links.push(Resolved { kind: kind.clone(), target: file_of(target) });
                self.push_inlines(inlines, ctx, base.patch(ctx.theme.style(element)), Some(id));
            }
            Inline::HardBreak => self.pieces.push(Piece { text: String::new(), style: base, link, hard_break: true }),
            Inline::TaskMarker(_) => {}
        }
    }
}

struct Wrapper<'a> {
    width: usize,
    source_line: usize,
    kinds: &'a [Resolved],
    lines: Vec<RenderedLine>,
    current: RenderedLine,
    used: usize,
    open: Option<(usize, usize)>,
    pending_space: Option<(Style, Option<usize>)>,
}

impl<'a> Wrapper<'a> {
    fn new(width: usize, source_line: usize, kinds: &'a [Resolved]) -> Self {
        Self {
            width: width.max(1),
            source_line,
            kinds,
            lines: Vec::new(),
            current: RenderedLine { source_line, ..RenderedLine::default() },
            used: 0,
            open: None,
            pending_space: None,
        }
    }

    fn run(mut self, pieces: &[Piece]) -> Vec<RenderedLine> {
        for piece in pieces {
            if piece.hard_break {
                self.newline();
                continue;
            }
            for token in tokenize(&piece.text) {
                match token {
                    Token::Space if !self.current.spans.is_empty() => {
                        self.pending_space = Some((piece.style, piece.link));
                    }
                    Token::Space => {}
                    Token::Word(word) => self.word(word, piece.style, piece.link),
                }
            }
        }
        self.finish()
    }

    fn word(&mut self, word: &str, style: Style, link: Option<usize>) {
        let space = usize::from(self.pending_space.is_some());
        if !self.current.spans.is_empty() && self.used + space + word.width() > self.width {
            self.newline();
        }
        if let Some((style, link)) = self.pending_space.take() {
            self.emit(" ", style, link);
        }

        let mut rest = word;
        loop {
            let room = self.width.saturating_sub(self.used);
            if rest.width() <= room {
                break;
            }
            let (head, tail) = split_at_width(rest, room);
            if head.is_empty() {
                if !self.current.spans.is_empty() {
                    self.newline();
                    continue;
                }
                let (head, tail) = split_first_char(rest);
                self.emit(head, style, link);
                self.newline();
                rest = tail;
                continue;
            }
            self.emit(head, style, link);
            self.newline();
            rest = tail;
        }
        if !rest.is_empty() {
            self.emit(rest, style, link);
        }
    }

    fn emit(&mut self, text: &str, style: Style, link: Option<usize>) {
        self.pending_space = None;
        let same_link = self.open.map(|(id, _)| id) == link;
        if !same_link {
            self.close_link();
            if let Some(id) = link {
                self.open = Some((id, self.current.spans.len()));
            }
        }

        match self.current.spans.last_mut() {
            Some(last) if last.style == style && same_link => last.text.push_str(text),
            _ => self.current.push(StyledSpan::new(text, style)),
        }
        self.used += text.width();
    }

    fn close_link(&mut self) {
        if let Some((id, start)) = self.open.take()
            && start < self.current.spans.len()
        {
            let resolved = &self.kinds[id];
            self.current.links.push(LinkRef {
                span_range: start..self.current.spans.len(),
                kind: resolved.kind.clone(),
                resolved: resolved.target.clone(),
            });
        }
    }

    fn newline(&mut self) {
        self.close_link();
        let finished =
            std::mem::replace(&mut self.current, RenderedLine { source_line: self.source_line, ..RenderedLine::default() });
        self.lines.push(finished);
        self.used = 0;
        self.pending_space = None;
    }

    fn finish(mut self) -> Vec<RenderedLine> {
        self.close_link();
        if !self.current.spans.is_empty() {
            self.lines.push(self.current);
        }
        self.lines
    }
}

enum Token<'a> {
    Word(&'a str),
    Space,
}

fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let whitespace = rest.starts_with(char::is_whitespace);
        let end = rest.find(|c: char| c.is_whitespace() != whitespace).unwrap_or(rest.len());
        let (head, tail) = rest.split_at(end);
        tokens.push(if whitespace { Token::Space } else { Token::Word(head) });
        rest = tail;
    }
    tokens
}

fn split_first_char(text: &str) -> (&str, &str) {
    text.split_at(text.chars().next().map_or(0, char::len_utf8))
}

fn split_at_width(text: &str, width: usize) -> (&str, &str) {
    let mut used = 0;
    for (offset, c) in text.char_indices() {
        let next = used + c.width().unwrap_or(0);
        if next > width {
            return text.split_at(offset);
        }
        used = next;
    }
    (text, "")
}

fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let (head, _) = split_at_width(text, width.saturating_sub(1));
    format!("{head}…")
}

fn styled_line(text: impl Into<String>, style: Style, source_line: usize) -> RenderedLine {
    RenderedLine { spans: vec![StyledSpan::new(text, style)], source_line, ..RenderedLine::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::document::Document;
    use crate::markdown::ast::parse;

    #[test]
    fn an_explicit_width_wins_over_the_terminal() {
        assert_eq!(wrap_width(Some(80), Some(200)), 80);
        assert_eq!(wrap_width(Some(80), None), 80);
    }

    #[test]
    fn a_zero_width_still_leaves_a_column_to_write_in() {
        assert_eq!(wrap_width(Some(0), None), 1);
    }

    #[test]
    fn no_terminal_to_measure_gets_the_default() {
        assert_eq!(wrap_width(None, None), DEFAULT_WIDTH);
    }

    #[test]
    fn a_terminal_is_measured_less_a_margin_and_capped() {
        assert_eq!(wrap_width(None, Some(40)), 38);
        assert_eq!(wrap_width(None, Some(200)), DEFAULT_WIDTH);
        assert_eq!(wrap_width(None, Some(1)), 1);
    }

    fn detached() -> Links {
        Links::new(Document::new(None, PathBuf::from("."), String::new()), None)
    }

    fn lines(source: &str, width: usize) -> Vec<String> {
        let theme = Theme::default();
        let links = detached();
        render(&parse(source), &Ctx::new(&theme, &links), width).iter().map(RenderedLine::text).collect()
    }

    fn bare(source: &str, width: usize) -> Vec<String> {
        let theme = Theme::default();
        let links = detached();
        blocks_to_lines(&parse(source), &Ctx::new(&theme, &links), theme.style(Element::Paragraph), width, 0)
            .iter()
            .map(RenderedLine::text)
            .collect()
    }

    #[test]
    fn a_paragraph_wraps_on_whitespace() {
        assert_eq!(bare("one two three four\n", 9), ["one two", "three", "four"]);
    }

    #[test]
    fn wrapping_never_exceeds_the_width() {
        let source = "The quick brown fox jumps over the lazy dog and keeps on going.\n";
        for width in 5..40 {
            for line in bare(source, width) {
                assert!(line.width() <= width, "{line:?} is wider than {width}");
            }
        }
    }

    #[test]
    fn the_gutter_takes_one_column_of_the_width() {
        assert_eq!(lines("one two three four\n", 10), [" one two", " three", " four"]);
    }

    #[test]
    fn a_word_longer_than_the_width_is_broken_hard() {
        assert_eq!(bare("supercalifragilistic\n", 7), ["superca", "lifragi", "listic"]);
    }

    #[test]
    fn cjk_counts_two_columns_per_character() {
        assert_eq!(bare("日本語 テスト\n", 6), ["日本語", "テスト"]);
    }

    #[test]
    fn a_hard_break_ends_the_line_early() {
        assert_eq!(bare("a\\\nb\n", 40), ["a", "b"]);
    }

    #[test]
    fn a_soft_break_wraps_like_a_space() {
        assert_eq!(bare("a\nb\n", 40), ["a b"]);
    }

    #[test]
    fn blocks_are_separated_by_one_blank_line() {
        assert_eq!(bare("a\n\nb\n", 40), ["a", "", "b"]);
    }

    #[test]
    fn the_document_never_ends_blank() {
        let rendered = bare("# Heading\n\ntext\n", 40);
        assert_eq!(rendered.last().map(String::as_str), Some("text"));
    }

    #[test]
    fn headings_hide_their_hashes_and_anchor_their_first_line() {
        let theme = Theme::default();
        let links = detached();
        let rendered = render(&parse("## A Heading\n"), &Ctx::new(&theme, &links), 40);
        assert_eq!(rendered[0].text(), " A Heading");
        assert_eq!(rendered[0].anchor.as_deref(), Some("a-heading"));
    }

    #[test]
    fn bullets_change_with_nesting_depth() {
        assert_eq!(bare("- a\n  - b\n    - c\n", 20), ["• a", "  ◦ b", "    ▪ c"]);
    }

    #[test]
    fn ordered_lists_count_from_their_start_number() {
        assert_eq!(bare("3. three\n4. four\n", 20), ["3. three", "4. four"]);
    }

    #[test]
    fn task_markers_replace_the_bullet() {
        assert_eq!(bare("- [x] done\n- [ ] todo\n", 20), ["☑ done", "☐ todo"]);
    }

    #[test]
    fn list_continuation_lines_line_up_under_the_text() {
        assert_eq!(bare("- one two three\n", 8), ["• one", "  two", "  three"]);
    }

    #[test]
    fn quotes_get_a_gutter_per_level() {
        assert_eq!(bare("> a\n>\n> > b\n", 20), ["┃ a", "┃ ", "┃ ┃ b"]);
    }

    #[test]
    fn quoted_text_wraps_inside_the_gutter() {
        assert_eq!(bare("> one two three\n", 7), ["┃ one", "┃ two", "┃ three"]);
    }

    #[test]
    fn quoted_text_takes_the_quote_style_but_a_link_keeps_its_own() {
        let theme = Theme::default();
        let links = detached();
        let rendered = render(&parse("> quoted [link](https://example.com)\n"), &Ctx::new(&theme, &links), 40);
        let styles: Vec<_> = rendered[0].spans.iter().map(|span| span.style.fg).collect();
        assert!(styles.contains(&Some(theme.palette.muted_text)), "quoted text should be muted: {styles:?}");
        assert!(styles.contains(&Some(theme.palette.highlight)), "the link should stay a link: {styles:?}");
    }

    #[test]
    fn code_blocks_truncate_rather_than_wrap() {
        let rendered = bare("```\nthis line is far too long\n```\n", 10);
        assert_eq!(rendered[1], " this li… ", "the pad costs two of the ten columns");
    }

    #[test]
    fn a_highlighted_line_truncates_and_stays_a_rectangle() {
        let theme = Theme::default();
        let links = detached();
        let rendered = blocks_to_lines(
            &parse("```rust\nfn main() { println!(\"far too long\"); }\n```\n"),
            &Ctx::new(&theme, &links),
            theme.style(Element::Paragraph),
            12,
            0,
        );
        assert_eq!(rendered[1].text(), " fn main()… ");
        assert!(rendered.iter().all(|line| line.width() == 12), "{rendered:?}");
        assert!(rendered[1].spans.len() > 1, "the line was not highlighted: {:?}", rendered[1]);
    }

    #[test]
    fn a_cut_that_lands_inside_a_wide_character_stops_there() {
        let theme = Theme::default();
        let links = detached();
        for width in 1..14 {
            let rendered = blocks_to_lines(
                &parse("```rust\nx = \"日本語\";\n```\n"),
                &Ctx::new(&theme, &links),
                theme.style(Element::Paragraph),
                width,
                0,
            );
            let line = &rendered[1];
            let text = line.text();
            let source = "x = \"日本語\";";
            let kept = text[line.byte_at(line.inset)..].trim_end().trim_end_matches('…');
            assert!(source.starts_with(kept), "{text:?} is not a prefix of {source:?} at width {width}");
            assert_eq!(line.width(), width, "{text:?} is not {width} columns wide");
        }
    }

    #[test]
    fn a_highlighted_line_keeps_the_block_background() {
        let theme = Theme::default();
        let links = detached();
        let rendered = blocks_to_lines(
            &parse("```rust\nfn main() {}\n```\n"),
            &Ctx::new(&theme, &links),
            theme.style(Element::Paragraph),
            20,
            0,
        );
        let block = theme.style(Element::CodeBlock);
        assert!(
            rendered[1].spans.iter().all(|span| span.style.bg == block.bg),
            "a .tmTheme repainted the block: {:?}",
            rendered[1]
        );
        assert!(rendered[1].spans.iter().any(|span| span.style.fg != block.fg), "nothing was highlighted: {:?}", rendered[1]);
    }

    #[test]
    fn a_fence_line_carries_the_language_right_aligned() {
        let rendered = bare("```rust\nfn main() {}\n```\n", 20);
        assert_eq!(rendered[0], "               rust ");
        assert_eq!(rendered[1], " fn main() {}       ");
    }

    #[test]
    fn code_lines_are_padded_so_the_background_is_a_rectangle() {
        let theme = Theme::default();
        let links = detached();
        for width in 1..=8 {
            let rendered =
                blocks_to_lines(&parse("```\nab\n```\n"), &Ctx::new(&theme, &links), theme.style(Element::Paragraph), width, 0);
            assert!(rendered.iter().all(|line| line.width() == width), "at width {width}: {rendered:?}");
        }
    }

    #[test]
    fn a_code_row_keeps_a_column_of_background_on_each_side() {
        let theme = Theme::default();
        let links = detached();
        let rendered = blocks_to_lines(
            &parse("```rust\n    let x = 1;\n```\n"),
            &Ctx::new(&theme, &links),
            theme.style(Element::Paragraph),
            20,
            0,
        );

        for line in &rendered {
            let text = line.text();
            assert!(text.starts_with(' '), "{text:?} starts hard against the edge");
            assert!(text.ends_with(' '), "{text:?} ends hard against the edge");
        }

        let code = &rendered[1];
        let text = code.text();
        assert_eq!(&text[code.byte_at(code.inset)..].trim_end(), &"    let x = 1;", "the indentation is part of the code");
    }

    #[test]
    fn a_code_block_too_narrow_to_pad_drops_the_padding() {
        let theme = Theme::default();
        let links = detached();
        for (width, expected) in [(1, "…"), (2, "a…"), (3, "ab…"), (4, " a… "), (5, " ab… ")] {
            let rendered = blocks_to_lines(
                &parse("```\nabcdef\n```\n"),
                &Ctx::new(&theme, &links),
                theme.style(Element::Paragraph),
                width,
                0,
            );
            assert!(rendered.iter().all(|line| line.width() == width), "at width {width}: {rendered:?}");
            assert_eq!(rendered[1].text(), expected, "at width {width}");
            assert_eq!(rendered[1].inset, code_pad(width), "at width {width}");
        }
    }

    #[test]
    fn a_language_tag_that_no_longer_fits_leaves_the_fence_blank() {
        assert_eq!(bare("```rust\nx\n```\n", 5)[0], "rust ");
        assert_eq!(bare("```rust\nx\n```\n", 4)[0], "    ");
    }

    #[test]
    fn a_rule_fills_the_width() {
        assert_eq!(bare("---\n", 5), ["─────"]);
    }

    #[test]
    fn images_render_as_a_label() {
        assert_eq!(bare("![a cat](cat.png)\n", 40), ["[image: a cat]"]);
    }

    #[test]
    fn html_blocks_are_shown_verbatim() {
        assert_eq!(bare("<div>\n  <p>hi</p>\n</div>\n", 40), ["<div>", "  <p>hi</p>", "</div>"]);
    }

    #[test]
    fn footnote_definitions_hang_under_their_marker() {
        assert_eq!(bare("[^1]: one two\n", 9), ["[^1] one", "     two"]);
    }

    #[test]
    fn a_table_that_fits_keeps_its_natural_widths() {
        let rendered = bare("| a | bb |\n| - | - |\n| 1 | 2 |\n", 40);
        assert_eq!(rendered, ["┌───┬────┐", "│ a │ bb │", "├───┼────┤", "│ 1 │ 2  │", "└───┴────┘"]);
    }

    #[test]
    fn table_alignment_places_the_padding() {
        let rendered = bare("| l | r | c |\n| :- | -: | :-: |\n| 1 | 2 | 3 |\n", 40);
        assert_eq!(rendered[3], "│ 1 │ 2 │ 3 │");
    }

    #[test]
    fn a_table_wider_than_the_width_shrinks_and_wraps() {
        let source = "| alpha beta | gamma delta |\n| - | - |\n| one two | three four |\n";
        let rendered = bare(source, 24);
        for line in &rendered {
            assert!(line.width() <= 24, "{line:?} is wider than 24");
        }
        assert!(rendered.len() > 5, "cells should have wrapped: {rendered:?}");
    }

    #[test]
    fn links_are_recorded_with_the_spans_they_cover() {
        let theme = Theme::default();
        let links = detached();
        let rendered = render(&parse("see [the docs](https://example.com) now\n"), &Ctx::new(&theme, &links), 40);
        let link = &rendered[0].links[0];
        assert_eq!(
            rendered[0].spans[link.span_range.clone()].iter().map(|span| span.text.as_str()).collect::<String>(),
            "the docs"
        );
        assert!(matches!(link.kind, LinkKind::External(_)));
    }

    #[test]
    fn a_link_broken_across_lines_is_recorded_on_both() {
        let theme = Theme::default();
        let links = detached();
        let rendered = render(&parse("[one two three](a.md)\n"), &Ctx::new(&theme, &links), 8);
        assert!(rendered.len() > 1);
        assert!(rendered.iter().all(|line| line.links.len() == 1), "{rendered:?}");
    }

    #[test]
    fn every_line_knows_its_source_line() {
        let theme = Theme::default();
        let links = detached();
        let rendered = render(&parse("# One\n\npara\n"), &Ctx::new(&theme, &links), 40);
        assert_eq!(rendered[0].source_line, 1);
        assert_eq!(rendered[2].source_line, 3);
    }

    #[test]
    fn a_character_wider_than_the_line_still_terminates() {
        for source in ["日本\n", "> 日\n", "- - - 日\n", "| 日 | 本 |\n| - | - |\n| 一 | 二 |\n"] {
            for width in 1..8 {
                let rendered = lines(source, width);
                assert!(rendered.len() < 100, "{source:?} at width {width} produced {} lines", rendered.len());
            }
        }
    }

    #[test]
    fn no_line_exceeds_the_width_at_any_width() {
        let source = concat!(
            "| a | b | c | d | e |\n| - | - | - | - | - |\n| 1 | 2 | 3 | 4 | 5 |\n\n",
            "> a quote with 日本語 in it\n\n- - - deeply nested\n\n```\nsome code\n```\n"
        );
        for width in 1..40 {
            for line in lines(source, width) {
                assert!(line.width() <= width, "at width {width}, {line:?} is {} columns", line.width());
            }
        }
    }

    #[test]
    fn adjacent_links_stay_two_links() {
        let theme = Theme::default();
        let links = detached();
        let rendered = render(&parse("[one](https://e.com)[two](https://f.com)\n"), &Ctx::new(&theme, &links), 40);
        assert_eq!(rendered[0].links.len(), 2, "{:?}", rendered[0]);

        let text_of =
            |link: &LinkRef| rendered[0].spans[link.span_range.clone()].iter().map(|span| span.text.as_str()).collect::<String>();
        assert_eq!(text_of(&rendered[0].links[0]), "one");
        assert_eq!(text_of(&rendered[0].links[1]), "two");
    }

    #[test]
    fn a_clamped_line_drops_the_links_it_cut() {
        let mut line = RenderedLine {
            spans: vec![StyledSpan::new("wide", Style::default()), StyledSpan::new("link", Style::default())],
            links: vec![LinkRef {
                span_range: 1..2,
                kind: LinkKind::Wiki { target: "x".into(), fragment: None },
                resolved: None,
            }],
            ..RenderedLine::default()
        };
        clamp_to_width(&mut line, 5);
        assert_eq!(line.text(), "wide…");
        assert!(line.links.is_empty(), "a link that was cut off cannot be followed");
    }

    #[test]
    fn truncation_marks_the_cut() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
        assert_eq!(truncate("日本語", 4), "日…");
    }

    #[test]
    fn splitting_respects_character_boundaries() {
        assert_eq!(split_at_width("日本語", 3), ("日", "本語"));
        assert_eq!(split_at_width("abc", 0), ("", "abc"));
    }
}
