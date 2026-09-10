//! Gather footnote definitions under a synthesized heading at the end.

use crate::markdown::ast::{Block, Inline, SourceBlock};

pub fn gather(blocks: Vec<SourceBlock>, anchor: impl FnOnce(&str) -> String) -> Vec<SourceBlock> {
    let mut body = Vec::with_capacity(blocks.len());
    let mut defs: Vec<(String, SourceBlock)> = Vec::new();

    for block in blocks {
        match &block.block {
            Block::FootnoteDef { label, .. } => {
                let label = label.clone();
                if !defs.iter().any(|(seen, _)| *seen == label) {
                    defs.push((label, block));
                }
            }
            _ => body.push(block),
        }
    }

    if defs.is_empty() {
        return body;
    }

    let mut order = Vec::new();
    for block in &body {
        references(&block.block, &mut order);
    }

    defs.sort_by_key(|(label, _)| order.iter().position(|seen| seen == label).unwrap_or(usize::MAX));

    let heading_line = defs.first().map_or(0, |(_, block)| block.line);
    body.push(SourceBlock {
        line: heading_line,
        block: Block::Heading { level: 2, inlines: vec![Inline::Text("Footnotes".to_string())], anchor: anchor("Footnotes") },
    });
    body.extend(defs.into_iter().map(|(_, block)| block));
    body
}

pub fn labels(blocks: &[SourceBlock]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| match &block.block {
            Block::FootnoteDef { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect()
}

fn references(block: &Block, order: &mut Vec<String>) {
    match block {
        Block::Paragraph(inlines) | Block::Heading { inlines, .. } => references_in(inlines, order),
        Block::Quote(blocks) | Block::FootnoteDef { blocks, .. } => {
            blocks.iter().for_each(|block| references(&block.block, order))
        }
        Block::List { items, .. } => items.iter().flat_map(|item| &item.blocks).for_each(|block| references(&block.block, order)),
        Block::Table { header, rows, .. } => {
            header.iter().chain(rows.iter().flatten()).for_each(|cell| references_in(cell, order))
        }
        Block::CodeBlock { .. } | Block::Rule | Block::Html(_) => {}
    }
}

fn references_in(inlines: &[Inline], order: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::FootnoteRef(label) => {
                if !order.iter().any(|seen| seen == label) {
                    order.push(label.clone());
                }
            }
            Inline::Emphasis(children) | Inline::Strong(children) | Inline::Strike(children) => references_in(children, order),
            Inline::Link { inlines, .. } => references_in(inlines, order),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::markdown::ast::{Block, Inline, parse};

    fn kinds(source: &str) -> Vec<Block> {
        parse(source).into_iter().map(|block| block.block).collect()
    }

    #[test]
    fn definitions_move_behind_a_synthesized_heading() {
        let blocks = kinds("a[^1]\n\n[^1]: note\n\nb\n");
        assert!(matches!(blocks[0], Block::Paragraph(_)));
        assert!(matches!(blocks[1], Block::Paragraph(_)));
        let Block::Heading { level, anchor, .. } = &blocks[2] else { panic!("no heading: {blocks:?}") };
        assert_eq!((*level, anchor.as_str()), (2, "footnotes"));
        assert!(matches!(blocks[3], Block::FootnoteDef { .. }));
    }

    #[test]
    fn a_document_without_footnotes_is_untouched() {
        assert_eq!(
            kinds("# One\n\npara\n"),
            vec![
                Block::Heading { level: 1, inlines: vec![Inline::Text("One".into())], anchor: "one".into() },
                Block::Paragraph(vec![Inline::Text("para".into())]),
            ]
        );
    }

    #[test]
    fn definitions_are_ordered_by_first_reference() {
        let blocks = kinds("first[^b] then[^a]\n\n[^a]: A\n\n[^b]: B\n");
        let labels: Vec<_> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::FootnoteDef { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["b", "a"]);
    }

    #[test]
    fn an_unreferenced_definition_falls_to_the_end() {
        let blocks = kinds("see[^used]\n\n[^unused]: never\n\n[^used]: yes\n");
        let labels: Vec<_> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::FootnoteDef { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["used", "unused"]);
    }

    #[test]
    fn a_reference_inside_a_list_still_orders_its_definition() {
        let blocks = kinds("- item[^x]\n\n[^y]: Y\n\n[^x]: X\n\ntext[^y]\n");
        let labels: Vec<_> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::FootnoteDef { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["x", "y"]);
    }

    #[test]
    fn a_synthesized_heading_does_not_collide_with_an_authored_one() {
        let blocks = kinds("## Footnotes\n\ntext[^1]\n\n[^1]: note\n");
        let anchors: Vec<_> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Heading { anchor, .. } => Some(anchor.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(anchors, ["footnotes", "footnotes-1"]);
    }
}
