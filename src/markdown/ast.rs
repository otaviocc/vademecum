//! `pulldown-cmark` events → a `Block`/`Inline` tree.
//!
//! The renderer wants a tree, not a stream: it has to know a list's nesting
//! depth before it can indent it and a table's cell widths before it can size
//! its columns. This module does that one conversion and nothing else — no
//! styling, no wrapping, no link resolution.

use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Tag};

use crate::markdown::links::{LinkKind, classify};

/// A block paired with the 1-based source line it starts on. Every
/// `RenderedLine` the block produces carries that number, which is what lets a
/// resize re-layout and still put the cursor back where the reader left it.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceBlock {
    pub line: usize,
    pub block: Block,
}

/// A block-level construct.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::enum_variant_names, reason = "CodeBlock is the name the README's data model gives it")]
pub enum Block {
    Heading { level: u8, inlines: Vec<Inline>, anchor: String },
    Paragraph(Vec<Inline>),
    Quote(Vec<SourceBlock>),
    List { ordered: Option<u64>, items: Vec<ListItem> },
    CodeBlock { lang: Option<String>, text: String },
    Table { header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>>, alignments: Vec<Alignment> },
    Rule,
    Html(String),
    FootnoteDef { label: String, blocks: Vec<SourceBlock> },
}

/// One item of a list.
#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub blocks: Vec<SourceBlock>,
}

impl ListItem {
    /// `Some(checked)` when the item opens with a task marker, which the
    /// renderer shows in place of the bullet.
    pub fn task(&self) -> Option<bool> {
        let first = self.blocks.first()?;
        match &first.block {
            Block::Paragraph(inlines) => match inlines.first()? {
                Inline::TaskMarker(checked) => Some(*checked),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Column alignment of a table, mirrored from `pulldown_cmark::Alignment` so
/// the parser type does not leak into the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

/// An inline construct.
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Code(String),
    Link { kind: LinkKind, inlines: Vec<Inline> },
    Image { alt: String, url: String },
    Html(String),
    FootnoteRef(String),
    SoftBreak,
    HardBreak,
    TaskMarker(bool),
}

/// Parse Markdown into a block tree.
pub fn parse(source: &str) -> Vec<SourceBlock> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_WIKILINKS;
    let events = pulldown_cmark::Parser::new_ext(source, options).into_offset_iter();

    Ast { events: events.peekable(), lines: LineIndex::new(source), anchors: HashMap::new() }.blocks()
}

/// The flat text of a run of inlines, as it will read on screen.
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    push_plain_text(inlines, &mut text);
    text
}

fn push_plain_text(inlines: &[Inline], text: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(value) | Inline::Code(value) | Inline::Html(value) => text.push_str(value),
            Inline::Emphasis(children) | Inline::Strong(children) | Inline::Strike(children) => push_plain_text(children, text),
            Inline::Link { inlines, .. } => push_plain_text(inlines, text),
            Inline::Image { alt, .. } => text.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => text.push(' '),
            Inline::FootnoteRef(_) | Inline::TaskMarker(_) => {}
        }
    }
}

struct Ast<'a, I: Iterator<Item = (Event<'a>, Range<usize>)>> {
    events: std::iter::Peekable<I>,
    lines: LineIndex,
    /// Slugs already handed out, so a repeated heading gets `-1`, `-2`, ….
    anchors: HashMap<String, usize>,
}

impl<'a, I: Iterator<Item = (Event<'a>, Range<usize>)>> Ast<'a, I> {
    /// Blocks up to — but not including — the `End` that closes the enclosing
    /// tag. The caller consumes that `End`.
    fn blocks(&mut self) -> Vec<SourceBlock> {
        let mut blocks = Vec::new();
        // A tight list item has no `Paragraph` tag: its inlines sit directly
        // inside the item, so block context has to be able to gather them.
        let mut loose = Vec::new();
        let mut loose_line = 0;

        while let Some((start, ends, opens_block)) =
            self.events.peek().map(|(event, range)| (range.start, matches!(event, Event::End(_)), opens_block(event)))
        {
            if ends {
                break;
            }

            let line = self.lines.line_of(start);
            let (event, _) = self.events.next().expect("peeked an event");

            if opens_block {
                if !loose.is_empty() {
                    blocks.push(SourceBlock { line: loose_line, block: Block::Paragraph(std::mem::take(&mut loose)) });
                }
                if let Some(block) = self.block(event) {
                    blocks.push(SourceBlock { line, block });
                }
            } else {
                if loose.is_empty() {
                    loose_line = line;
                }
                if let Some(inline) = self.inline(event) {
                    loose.push(inline);
                }
            }
        }

        if !loose.is_empty() {
            blocks.push(SourceBlock { line: loose_line, block: Block::Paragraph(loose) });
        }
        blocks
    }

    fn block(&mut self, event: Event<'a>) -> Option<Block> {
        let tag = match event {
            Event::Rule => return Some(Block::Rule),
            Event::Start(tag) => tag,
            _ => return None,
        };

        Some(match tag {
            Tag::Paragraph => Block::Paragraph(self.inlines()),
            Tag::Heading { level, .. } => {
                let inlines = self.inlines();
                let anchor = self.anchor(&plain_text(&inlines));
                Block::Heading { level: heading_level(level), inlines, anchor }
            }
            Tag::BlockQuote(_) => {
                let blocks = self.blocks();
                self.skip_end();
                Block::Quote(blocks)
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().map(str::to_string),
                    CodeBlockKind::Indented => None,
                };
                let mut text = String::new();
                for (event, _) in self.events.by_ref() {
                    match event {
                        Event::End(_) => break,
                        Event::Text(value) => text.push_str(&value),
                        _ => {}
                    }
                }
                Block::CodeBlock { lang, text }
            }
            Tag::HtmlBlock => {
                let mut text = String::new();
                for (event, _) in self.events.by_ref() {
                    match event {
                        Event::End(_) => break,
                        Event::Html(value) | Event::Text(value) => text.push_str(&value),
                        _ => {}
                    }
                }
                Block::Html(text.trim_end_matches('\n').to_string())
            }
            Tag::List(ordered) => {
                let mut items = Vec::new();
                while let Some((event, _)) = self.events.next() {
                    match event {
                        Event::End(_) => break,
                        Event::Start(Tag::Item) => {
                            let blocks = self.blocks();
                            self.skip_end();
                            items.push(ListItem { blocks });
                        }
                        _ => {}
                    }
                }
                Block::List { ordered, items }
            }
            Tag::Table(alignments) => {
                let mut header = Vec::new();
                let mut rows = Vec::new();
                while let Some((event, _)) = self.events.next() {
                    match event {
                        Event::End(_) => break,
                        Event::Start(Tag::TableHead) => header = self.cells(),
                        Event::Start(Tag::TableRow) => rows.push(self.cells()),
                        _ => {}
                    }
                }
                Block::Table { header, rows, alignments: alignments.into_iter().map(alignment).collect() }
            }
            Tag::FootnoteDefinition(label) => {
                let blocks = self.blocks();
                self.skip_end();
                Block::FootnoteDef { label: label.to_string(), blocks }
            }
            _ => {
                self.skip_to_end();
                return None;
            }
        })
    }

    /// The cells of one table row, consuming the row's `End`.
    fn cells(&mut self) -> Vec<Vec<Inline>> {
        let mut cells = Vec::new();
        while let Some((event, _)) = self.events.next() {
            match event {
                Event::End(_) => break,
                Event::Start(Tag::TableCell) => cells.push(self.inlines()),
                _ => {}
            }
        }
        cells
    }

    /// Inlines up to and including the `End` that closes the enclosing tag.
    fn inlines(&mut self) -> Vec<Inline> {
        let mut inlines = Vec::new();
        while let Some((event, _)) = self.events.next() {
            match event {
                Event::End(_) => break,
                other => {
                    if let Some(inline) = self.inline(other) {
                        inlines.push(inline);
                    }
                }
            }
        }
        inlines
    }

    fn inline(&mut self, event: Event<'a>) -> Option<Inline> {
        Some(match event {
            Event::Text(value) => Inline::Text(value.to_string()),
            Event::Code(value) => Inline::Code(value.to_string()),
            Event::Html(value) | Event::InlineHtml(value) => Inline::Html(value.to_string()),
            Event::FootnoteReference(label) => Inline::FootnoteRef(label.to_string()),
            Event::SoftBreak => Inline::SoftBreak,
            Event::HardBreak => Inline::HardBreak,
            Event::TaskListMarker(checked) => Inline::TaskMarker(checked),
            Event::Start(Tag::Emphasis) => Inline::Emphasis(self.inlines()),
            Event::Start(Tag::Strong) => Inline::Strong(self.inlines()),
            Event::Start(Tag::Strikethrough) => Inline::Strike(self.inlines()),
            Event::Start(Tag::Link { link_type, dest_url, .. }) => {
                Inline::Link { kind: classify(&dest_url, link_type), inlines: self.inlines() }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                Inline::Image { alt: plain_text(&self.inlines()), url: dest_url.to_string() }
            }
            Event::Start(_) => {
                self.skip_to_end();
                return None;
            }
            _ => return None,
        })
    }

    /// Consume one `End`, which the caller knows is next.
    fn skip_end(&mut self) {
        if matches!(self.events.peek(), Some((Event::End(_), _))) {
            self.events.next();
        }
    }

    /// Consume everything up to and including the `End` of the tag just taken.
    fn skip_to_end(&mut self) {
        let mut depth = 1usize;
        for (event, _) in self.events.by_ref() {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    /// A GitHub-style slug, made unique within the document.
    fn anchor(&mut self, text: &str) -> String {
        let slug = slug(text);
        let seen = self.anchors.entry(slug.clone()).or_insert(0);
        *seen += 1;
        if *seen == 1 { slug } else { format!("{slug}-{}", *seen - 1) }
    }
}

/// Whether an event opens a block, as opposed to an inline.
fn opens_block(event: &Event<'_>) -> bool {
    match event {
        Event::Rule => true,
        Event::Start(tag) => matches!(
            tag,
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(_)
                | Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::List(_)
                | Tag::FootnoteDefinition(_)
                | Tag::Table(_)
        ),
        _ => false,
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn alignment(alignment: pulldown_cmark::Alignment) -> Alignment {
    match alignment {
        pulldown_cmark::Alignment::None => Alignment::None,
        pulldown_cmark::Alignment::Left => Alignment::Left,
        pulldown_cmark::Alignment::Center => Alignment::Center,
        pulldown_cmark::Alignment::Right => Alignment::Right,
    }
}

/// GitHub's heading slug: lowercase, punctuation dropped, spaces to hyphens.
pub fn slug(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if c == '-' || c == '_' {
            slug.push(c);
        } else if c.is_whitespace() {
            slug.push('-');
        }
    }
    slug
}

/// Byte offset → line number, over the offsets of every line start.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
        Self { starts }
    }

    /// The 1-based line containing `offset`.
    fn line_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|start| *start <= offset).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(source: &str) -> Vec<Block> {
        parse(source).into_iter().map(|block| block.block).collect()
    }

    fn one(source: &str) -> Block {
        let mut blocks = blocks(source);
        assert_eq!(blocks.len(), 1, "expected a single block from {source:?}, got {blocks:?}");
        blocks.remove(0)
    }

    fn text(value: &str) -> Inline {
        Inline::Text(value.to_string())
    }

    #[test]
    fn headings_carry_level_and_anchor() {
        for level in 1..=6u8 {
            let source = format!("{} Heading {level}\n", "#".repeat(level as usize));
            let Block::Heading { level: parsed, anchor, inlines } = one(&source) else { panic!("not a heading") };
            assert_eq!(parsed, level);
            assert_eq!(anchor, format!("heading-{level}"));
            assert_eq!(inlines, vec![text(&format!("Heading {level}"))]);
        }
    }

    #[test]
    fn repeated_headings_get_distinct_anchors() {
        let anchors: Vec<_> = parse("# Notes\n\n# Notes\n\n# Notes\n")
            .into_iter()
            .filter_map(|block| match block.block {
                Block::Heading { anchor, .. } => Some(anchor),
                _ => None,
            })
            .collect();
        assert_eq!(anchors, ["notes", "notes-1", "notes-2"]);
    }

    #[test]
    fn slugs_follow_github() {
        assert_eq!(slug("Hello, World!"), "hello-world");
        assert_eq!(slug("A B  C"), "a-b--c");
        assert_eq!(slug("keep_under-score"), "keep_under-score");
        assert_eq!(slug("Ünicode Ok"), "ünicode-ok");
    }

    #[test]
    fn a_paragraph_is_a_run_of_inlines() {
        assert_eq!(one("just text\n"), Block::Paragraph(vec![text("just text")]));
    }

    #[test]
    fn emphasis_strong_and_strikethrough_nest() {
        let Block::Paragraph(inlines) = one("*a* **b** ~~c~~\n") else { panic!("not a paragraph") };
        assert_eq!(inlines[0], Inline::Emphasis(vec![text("a")]));
        assert_eq!(inlines[2], Inline::Strong(vec![text("b")]));
        assert_eq!(inlines[4], Inline::Strike(vec![text("c")]));
    }

    #[test]
    fn inline_code_keeps_its_text() {
        let Block::Paragraph(inlines) = one("`let x = 1;`\n") else { panic!("not a paragraph") };
        assert_eq!(inlines, vec![Inline::Code("let x = 1;".into())]);
    }

    #[test]
    fn breaks_are_distinguished() {
        let Block::Paragraph(inlines) = one("a\nb\\\nc\n") else { panic!("not a paragraph") };
        assert!(inlines.contains(&Inline::SoftBreak));
        assert!(inlines.contains(&Inline::HardBreak));
    }

    #[test]
    fn a_fenced_block_keeps_its_language_and_text() {
        assert_eq!(
            one("```rust\nfn main() {}\n```\n"),
            Block::CodeBlock { lang: Some("rust".into()), text: "fn main() {}\n".into() }
        );
    }

    #[test]
    fn an_unlabelled_fence_has_no_language() {
        assert_eq!(one("```\nplain\n```\n"), Block::CodeBlock { lang: None, text: "plain\n".into() });
    }

    #[test]
    fn quotes_hold_blocks() {
        let Block::Quote(blocks) = one("> quoted\n") else { panic!("not a quote") };
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block, Block::Paragraph(vec![text("quoted")]));
    }

    #[test]
    fn nested_quotes_nest() {
        let Block::Quote(outer) = one("> a\n>\n> > b\n") else { panic!("not a quote") };
        assert!(matches!(outer.last().map(|block| &block.block), Some(Block::Quote(_))));
    }

    #[test]
    fn bullet_lists_have_no_start_number() {
        let Block::List { ordered, items } = one("- a\n- b\n") else { panic!("not a list") };
        assert_eq!(ordered, None);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].blocks[0].block, Block::Paragraph(vec![text("a")]));
    }

    #[test]
    fn ordered_lists_keep_their_start_number() {
        let Block::List { ordered, .. } = one("3. three\n4. four\n") else { panic!("not a list") };
        assert_eq!(ordered, Some(3));
    }

    #[test]
    fn nested_lists_nest() {
        let Block::List { items, .. } = one("- a\n  - b\n") else { panic!("not a list") };
        assert!(items[0].blocks.iter().any(|block| matches!(block.block, Block::List { .. })));
    }

    #[test]
    fn task_items_expose_their_marker() {
        let Block::List { items, .. } = one("- [x] done\n- [ ] todo\n") else { panic!("not a list") };
        assert_eq!(items[0].task(), Some(true));
        assert_eq!(items[1].task(), Some(false));
    }

    #[test]
    fn a_plain_item_has_no_marker() {
        let Block::List { items, .. } = one("- a\n") else { panic!("not a list") };
        assert_eq!(items[0].task(), None);
    }

    #[test]
    fn tables_split_header_rows_and_alignment() {
        let source = "| a | b |\n| :- | -: |\n| 1 | 2 |\n";
        let Block::Table { header, rows, alignments } = one(source) else { panic!("not a table") };
        assert_eq!(header, vec![vec![text("a")], vec![text("b")]]);
        assert_eq!(rows, vec![vec![vec![text("1")], vec![text("2")]]]);
        assert_eq!(alignments, vec![Alignment::Left, Alignment::Right]);
    }

    #[test]
    fn a_rule_is_a_rule() {
        assert_eq!(one("---\n"), Block::Rule);
    }

    #[test]
    fn an_html_block_is_kept_verbatim() {
        assert_eq!(one("<div>\n  <p>hi</p>\n</div>\n"), Block::Html("<div>\n  <p>hi</p>\n</div>".into()));
    }

    #[test]
    fn inline_html_stays_inline() {
        let Block::Paragraph(inlines) = one("a <br> b\n") else { panic!("not a paragraph") };
        assert!(inlines.contains(&Inline::Html("<br>".into())));
    }

    #[test]
    fn footnotes_split_into_reference_and_definition() {
        let blocks = blocks("text[^1]\n\n[^1]: the note\n");
        let Block::Paragraph(inlines) = &blocks[0] else { panic!("not a paragraph") };
        assert!(inlines.contains(&Inline::FootnoteRef("1".into())));

        let Block::FootnoteDef { label, blocks } = &blocks[1] else { panic!("not a footnote definition") };
        assert_eq!(label, "1");
        assert_eq!(blocks[0].block, Block::Paragraph(vec![text("the note")]));
    }

    #[test]
    fn links_are_classified_as_they_are_parsed() {
        let Block::Paragraph(inlines) = one("[text](other.md#sec)\n") else { panic!("not a paragraph") };
        let Inline::Link { kind, inlines } = &inlines[0] else { panic!("not a link") };
        assert_eq!(*kind, LinkKind::Local { path: "other.md".into(), fragment: Some("sec".into()) });
        assert_eq!(*inlines, vec![text("text")]);
    }

    #[test]
    fn wikilinks_keep_their_alias_as_text() {
        let Block::Paragraph(inlines) = one("[[note|the note]]\n") else { panic!("not a paragraph") };
        let Inline::Link { kind, inlines } = &inlines[0] else { panic!("not a link") };
        assert_eq!(*kind, LinkKind::Wiki { target: "note".into(), fragment: None });
        assert_eq!(plain_text(inlines), "the note");
    }

    #[test]
    fn images_keep_alt_and_url() {
        let Block::Paragraph(inlines) = one("![a cat](cat.png)\n") else { panic!("not a paragraph") };
        assert_eq!(inlines, vec![Inline::Image { alt: "a cat".into(), url: "cat.png".into() }]);
    }

    #[test]
    fn blocks_know_the_line_they_start_on() {
        let lines: Vec<_> = parse("# One\n\npara\n\n- item\n").into_iter().map(|block| block.line).collect();
        assert_eq!(lines, [1, 3, 5]);
    }

    #[test]
    fn nested_blocks_know_their_line_too() {
        let Block::Quote(blocks) = parse("\n\n> a\n>\n> b\n").remove(0).block else { panic!("not a quote") };
        assert_eq!(blocks.iter().map(|block| block.line).collect::<Vec<_>>(), [3, 5]);
    }

    #[test]
    fn line_index_counts_from_one() {
        let index = LineIndex::new("a\nbb\n\nc");
        assert_eq!(index.line_of(0), 1);
        assert_eq!(index.line_of(2), 2);
        assert_eq!(index.line_of(5), 3);
        assert_eq!(index.line_of(6), 4);
    }

    #[test]
    fn plain_text_flattens_a_run() {
        let inlines = vec![text("a "), Inline::Strong(vec![text("b")]), Inline::SoftBreak, Inline::Code("c".into())];
        assert_eq!(plain_text(&inlines), "a b c");
    }
}
