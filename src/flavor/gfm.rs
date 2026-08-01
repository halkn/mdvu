//! Plain GFM. Azure recognition and its diagnostics are disabled; only fenced
//! Mermaid is promoted, since that is standard GFM fence syntax rather than an
//! Azure extension.

use crate::markdown::model::{Block, DiagramBlock, Document};
use crate::source::SourceText;

pub fn parse(source: SourceText) -> Document {
    let mut document = crate::markdown::parse(source);
    promote_diagrams(&mut document.blocks);
    document
}

fn promote_diagrams(blocks: &mut [Block]) {
    for block in blocks.iter_mut() {
        match block {
            Block::Code(c) if c.language.as_deref() == Some("mermaid") => {
                *block = Block::Diagram(DiagramBlock::new(
                    "mermaid".to_string(),
                    c.text.clone(),
                    c.range,
                ));
            }
            Block::Quote(q) => promote_diagrams(&mut q.blocks),
            Block::List(l) => {
                for item in &mut l.items {
                    promote_diagrams(&mut item.blocks);
                }
            }
            _ => {}
        }
    }
}
