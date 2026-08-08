//! Block IR to rendered lines.
//!
//! Container blocks lay their children out at a reduced width and then prepend
//! a prefix, so quote borders and list indentation compose naturally and
//! continuation lines align with content rather than with the marker.

use std::rc::Rc;

use crate::cli::MermaidMode;
use crate::layout::highlight;
use crate::layout::inline::{InlineContext, segments, spans};
use crate::layout::table::layout_table;
use crate::layout::wrap::{display_width, wrap_spans};
use crate::layout::{LayoutOptions, RenderedDocument, RenderedLine, RenderedSpan, StyleRole};
use crate::markdown::model::{
    Block, CodeBlock, DetailsBlock, DiagramBlock, Document, FootnoteBlock, HeadingBlock, Inline,
    ListBlock, ParagraphBlock, PlaceholderBlock, QuoteBlock, RawHtmlBlock, TocBlock,
};
use crate::source::SourceRange;

const TAB_STOP: usize = 4;

pub fn layout_document(
    document: &Document,
    options: LayoutOptions,
    ctx: &InlineContext,
) -> RenderedDocument {
    let mut lines = Vec::new();
    layout_blocks(&document.blocks, options.width, ctx, &mut lines);
    RenderedDocument {
        lines,
        diagnostics: document.diagnostics.clone(),
    }
}

fn layout_blocks(blocks: &[Block], width: usize, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 && needs_separator(&blocks[index - 1], block) {
            out.push(RenderedLine::blank());
        }
        layout_block(block, width, ctx, out);
    }
}

/// A list that follows its item's own text belongs to that text, so no blank
/// line separates them. Everything else is separated.
fn needs_separator(previous: &Block, current: &Block) -> bool {
    !matches!((previous, current), (Block::Paragraph(_), Block::List(_)))
}

fn layout_block(block: &Block, width: usize, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    match block {
        Block::Heading(h) => heading(h, width, ctx, out),
        Block::Paragraph(p) => paragraph(p, width, ctx, out),
        Block::List(l) => list(l, width, ctx, out),
        Block::Quote(q) => quote(q, width, ctx, out),
        Block::Code(c) => code(c, ctx, out),
        Block::Table(t) => out.extend(layout_table(t, width, ctx)),
        Block::HorizontalRule(range) => out.push(line(
            vec![RenderedSpan::new("─".repeat(width), StyleRole::Muted)],
            *range,
        )),
        Block::Footnote(f) => footnote(f, width, ctx, out),
        Block::RawHtml(h) => raw_html(h, out),
        Block::Toc(t) => toc(t, width, out),
        Block::Details(d) => details(d, width, ctx, out),
        Block::Placeholder(p) => placeholder(p, width, out),
        Block::Diagram(d) => diagram(d, width, ctx, out),
    }
}

fn toc(t: &TocBlock, width: usize, out: &mut Vec<RenderedLine>) {
    out.push(line(
        vec![RenderedSpan::new("Contents", StyleRole::Strong)],
        t.range,
    ));
    if t.entries.is_empty() {
        out.push(line(
            vec![RenderedSpan::new("  (no headings)", StyleRole::Muted)],
            t.range,
        ));
        return;
    }
    let base = t.entries.iter().map(|e| e.level).min().unwrap_or(1);
    for entry in &t.entries {
        let indent = "  ".repeat(usize::from(entry.level.saturating_sub(base)) + 1);
        let marker = RenderedSpan::new(format!("{indent}• "), StyleRole::ListMarker);
        let available = width.saturating_sub(display_width(&marker.text)).max(1);
        let text = wrap_spans(
            &[RenderedSpan::new(&entry.text, StyleRole::Link)],
            available,
        );
        for (index, mut spans) in text.into_iter().enumerate() {
            let prefix = if index == 0 {
                marker.clone()
            } else {
                RenderedSpan::new(" ".repeat(display_width(&marker.text)), StyleRole::Normal)
            };
            let mut all = vec![prefix];
            all.append(&mut spans);
            // TOC entries map back to the heading they came from.
            out.push(line(all, entry.range));
        }
    }
}

fn details(d: &DetailsBlock, width: usize, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    const BORDER: &str = "│ ";
    let marker = "▾ ";
    let marker_width = display_width(marker);
    let summary = spans(&d.summary, ctx);
    for (index, mut line_spans) in wrap_spans(&summary, width.saturating_sub(marker_width).max(1))
        .into_iter()
        .enumerate()
    {
        let prefix = if index == 0 {
            RenderedSpan::new(marker, StyleRole::ListMarker)
        } else {
            RenderedSpan::new(" ".repeat(marker_width), StyleRole::Normal)
        };
        let mut all = vec![prefix];
        for span in &mut line_spans {
            if span.role == StyleRole::Normal {
                span.role = StyleRole::Strong;
            }
        }
        all.append(&mut line_spans);
        out.push(line(all, d.range));
    }

    let border_width = display_width(BORDER);
    let mut child = Vec::new();
    layout_blocks(
        &d.blocks,
        width.saturating_sub(border_width).max(1),
        ctx,
        &mut child,
    );
    for mut rendered in child {
        let mut all = vec![RenderedSpan::new(BORDER, StyleRole::CodeBorder)];
        all.append(&mut rendered.spans);
        out.push(RenderedLine {
            spans: all,
            source_range: rendered.source_range.or(Some(d.range)),
            no_wrap: rendered.no_wrap,
            image: rendered.image,
        });
    }
}

fn placeholder(p: &PlaceholderBlock, width: usize, out: &mut Vec<RenderedLine>) {
    let label = format!("[{}] ", p.kind);
    let marker_width = display_width(&label);
    let message = wrap_spans(
        &[RenderedSpan::new(&p.message, StyleRole::Muted)],
        width.saturating_sub(marker_width).max(1),
    );
    for (index, mut spans) in message.into_iter().enumerate() {
        let prefix = if index == 0 {
            RenderedSpan::new(label.clone(), StyleRole::Warning)
        } else {
            RenderedSpan::new(" ".repeat(marker_width), StyleRole::Normal)
        };
        let mut all = vec![prefix];
        all.append(&mut spans);
        out.push(line(all, p.range));
    }
    // Unsupported bodies are preserved rather than dropped.
    if let Some(source) = &p.source {
        for text in source.lines() {
            out.push(RenderedLine {
                spans: vec![
                    RenderedSpan::new("│ ", StyleRole::CodeBorder),
                    RenderedSpan::new(expand_tabs(text), StyleRole::Muted),
                ],
                source_range: Some(p.range),
                no_wrap: true,
                image: None,
            });
        }
    }
}

fn diagram(d: &DiagramBlock, width: usize, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    if ctx.mermaid == MermaidMode::Off {
        out.push(line(
            vec![RenderedSpan::new(
                format!("[{} diagram omitted]", d.language),
                StyleRole::Muted,
            )],
            d.range,
        ));
        return;
    }

    // Compatibility warnings sit directly above the diagram they describe.
    for warning in &d.warnings {
        for spans in wrap_spans(
            &[RenderedSpan::new(
                format!("! {warning}"),
                StyleRole::Warning,
            )],
            width,
        ) {
            out.push(line(spans, d.range));
        }
    }

    if let Some(message) = &d.error {
        error_fallback(d, message, width, out);
        return;
    }

    let (header, body): (String, Vec<String>) = match &d.rendered {
        Some(lines) => (format!("╭─ {}", d.language), lines.clone()),
        None => (
            format!("╭─ {} (source)", d.language),
            d.source.lines().map(str::to_string).collect(),
        ),
    };

    out.push(preformatted(header, StyleRole::DiagramBorder, d, true));
    for text in body {
        out.push(RenderedLine {
            spans: vec![
                RenderedSpan::new("│ ", StyleRole::DiagramBorder),
                // Diagram lines keep their leading whitespace and are never
                // re-wrapped; the pager scrolls horizontally instead.
                RenderedSpan::new(expand_tabs(&text), StyleRole::Diagram),
            ],
            source_range: Some(d.range),
            no_wrap: true,
            image: None,
        });
    }
    out.push(preformatted(
        "╰─".to_string(),
        StyleRole::DiagramBorder,
        d,
        true,
    ));
}

/// Show a short error and the original source, so a broken diagram never hides
/// what the author wrote.
fn error_fallback(d: &DiagramBlock, message: &str, width: usize, out: &mut Vec<RenderedLine>) {
    let (title, role) = if d.unsupported {
        ("Mermaid diagram not supported as text", StyleRole::Warning)
    } else {
        ("Mermaid render error", StyleRole::Error)
    };
    let dashes = width.saturating_sub(display_width(title) + 4).max(2);
    out.push(preformatted(
        format!("╭─ {title} {}", "─".repeat(dashes)),
        role,
        d,
        true,
    ));
    for spans in wrap_spans(
        &[RenderedSpan::new(message, role)],
        width.saturating_sub(2).max(1),
    ) {
        let mut all = vec![RenderedSpan::new("│ ", role)];
        all.extend(spans);
        out.push(RenderedLine {
            spans: all,
            source_range: Some(d.range),
            no_wrap: false,
            image: None,
        });
    }
    out.push(preformatted(
        format!("├{}", "─".repeat(width.saturating_sub(1).max(2))),
        role,
        d,
        true,
    ));
    for text in d.source.lines() {
        out.push(RenderedLine {
            spans: vec![
                RenderedSpan::new("│ ", role),
                RenderedSpan::new(expand_tabs(text), StyleRole::Diagram),
            ],
            source_range: Some(d.range),
            no_wrap: true,
            image: None,
        });
    }
    out.push(preformatted(
        format!("╰{}", "─".repeat(width.saturating_sub(1).max(2))),
        role,
        d,
        true,
    ));
}

fn preformatted(text: String, role: StyleRole, d: &DiagramBlock, no_wrap: bool) -> RenderedLine {
    RenderedLine {
        spans: vec![RenderedSpan::new(text, role)],
        source_range: Some(d.range),
        no_wrap,
        image: None,
    }
}

fn heading(h: &HeadingBlock, width: usize, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    let level = h.level.min(6);
    let marker = format!("{} ", "#".repeat(level as usize));
    let marker_width = display_width(&marker);
    let content = spans(&h.content, ctx);
    let wrapped = wrap_spans(&content, width.saturating_sub(marker_width).max(1));

    for (index, mut line_spans) in wrapped.into_iter().enumerate() {
        for span in &mut line_spans {
            // Heading text keeps its own inline roles only where they add
            // meaning; plain runs take the heading role.
            if span.role == StyleRole::Normal {
                span.role = StyleRole::Heading(level);
            }
        }
        let prefix = if index == 0 {
            RenderedSpan::new(marker.clone(), StyleRole::Muted)
        } else {
            RenderedSpan::new(" ".repeat(marker_width), StyleRole::Normal)
        };
        let mut all = vec![prefix];
        all.extend(line_spans);
        out.push(line(all, h.range));
    }

    if level <= 2 {
        out.push(line(
            vec![RenderedSpan::new("─".repeat(width), StyleRole::Muted)],
            h.range,
        ));
    }
}

fn paragraph(p: &ParagraphBlock, width: usize, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    if let Some(placement) = drawable_image(p, width, ctx) {
        // The image covers cells the backend paints over; the reserved lines
        // stay blank and carry the paragraph's range so `--line` and `--watch`
        // still find their way back to the source.
        for row in 0..placement.rows {
            out.push(RenderedLine {
                spans: Vec::new(),
                source_range: Some(p.range),
                no_wrap: true,
                image: (row == 0).then(|| Rc::clone(&placement)),
            });
        }
        return;
    }
    for segment in segments(&p.content, ctx) {
        for line_spans in wrap_spans(&segment, width) {
            out.push(line(line_spans, p.range));
        }
    }
}

/// A paragraph whose whole content is one image, resolved against the terminal.
///
/// Only this shape becomes a picture. An image sitting among words would need a
/// multi-row box inside a wrapped line, so it keeps its text placeholder.
fn drawable_image(
    p: &ParagraphBlock,
    width: usize,
    ctx: &InlineContext,
) -> Option<Rc<crate::image::Placement>> {
    let support = ctx.images?;
    let [Inline::Image(image)] = p.content.as_slice() else {
        return None;
    };
    crate::image::resolve(&image.dest, ctx.base_dir.as_deref(), support, width)
}

fn list(l: &ListBlock, width: usize, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    // A list is loose when some item carries more than one block of its own,
    // ignoring nested lists, which attach to the item's text.
    let loose = l.items.iter().any(|item| {
        item.blocks
            .iter()
            .filter(|b| !matches!(b, Block::List(_)))
            .count()
            > 1
    });
    for (index, item) in l.items.iter().enumerate() {
        if index > 0 && loose {
            out.push(RenderedLine::blank());
        }
        let (marker, marker_role) = match (l.start, item.task) {
            (_, Some(true)) => ("[x] ".to_string(), StyleRole::TaskChecked),
            (_, Some(false)) => ("[ ] ".to_string(), StyleRole::TaskUnchecked),
            (Some(start), None) => (
                format!("{}. ", start.saturating_add(index as u64)),
                StyleRole::ListMarker,
            ),
            (None, None) => ("• ".to_string(), StyleRole::ListMarker),
        };
        let marker_width = display_width(&marker);

        let mut child = Vec::new();
        layout_blocks(
            &item.blocks,
            width.saturating_sub(marker_width).max(1),
            ctx,
            &mut child,
        );
        if child.is_empty() {
            child.push(RenderedLine::blank());
        }

        for (line_index, mut rendered) in child.into_iter().enumerate() {
            let prefix = if line_index == 0 {
                RenderedSpan::new(marker.clone(), marker_role)
            } else {
                RenderedSpan::new(" ".repeat(marker_width), StyleRole::Normal)
            };
            let mut all = vec![prefix];
            all.append(&mut rendered.spans);
            out.push(RenderedLine {
                spans: all,
                source_range: rendered.source_range.or(Some(item.range)),
                no_wrap: rendered.no_wrap,
                image: rendered.image,
            });
        }
    }
}

fn quote(q: &QuoteBlock, width: usize, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    const BORDER: &str = "│ ";
    let border_width = display_width(BORDER);
    let mut child = Vec::new();
    layout_blocks(
        &q.blocks,
        width.saturating_sub(border_width).max(1),
        ctx,
        &mut child,
    );
    for mut rendered in child {
        let mut all = vec![RenderedSpan::new(BORDER, StyleRole::Quote)];
        all.append(&mut rendered.spans);
        out.push(RenderedLine {
            spans: all,
            source_range: rendered.source_range.or(Some(q.range)),
            no_wrap: rendered.no_wrap,
            image: rendered.image,
        });
    }
}

fn code(c: &CodeBlock, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    let header = match &c.language {
        Some(language) => format!("╭─ {language}"),
        None => "╭─".to_string(),
    };
    out.push(RenderedLine {
        spans: vec![RenderedSpan::new(header, StyleRole::CodeBorder)],
        source_range: Some(c.range),
        no_wrap: true,
        image: None,
    });
    // Tabs are expanded first so highlighting and the drawn columns agree.
    let body: String = c
        .text
        .lines()
        .map(expand_tabs)
        .collect::<Vec<_>>()
        .join("\n");
    let highlighted = ctx
        .highlight
        .then_some(c.language.as_deref())
        .flatten()
        .and_then(|language| highlight::highlight(language, &body));

    for (index, text) in body.lines().enumerate() {
        let content = match &highlighted {
            Some(lines) => lines[index].clone(),
            None => vec![RenderedSpan::new(text, StyleRole::Code)],
        };
        let mut spans = vec![RenderedSpan::new("│ ", StyleRole::CodeBorder)];
        spans.extend(content);
        out.push(RenderedLine {
            spans,
            source_range: Some(c.range),
            no_wrap: true,
            image: None,
        });
    }
    out.push(RenderedLine {
        spans: vec![RenderedSpan::new("╰─", StyleRole::CodeBorder)],
        source_range: Some(c.range),
        no_wrap: true,
        image: None,
    });
}

fn footnote(f: &FootnoteBlock, width: usize, ctx: &InlineContext, out: &mut Vec<RenderedLine>) {
    let marker = format!("[^{}] ", f.label);
    let marker_width = display_width(&marker);
    let mut child = Vec::new();
    layout_blocks(
        &f.blocks,
        width.saturating_sub(marker_width).max(1),
        ctx,
        &mut child,
    );
    for (index, mut rendered) in child.into_iter().enumerate() {
        let prefix = if index == 0 {
            RenderedSpan::new(marker.clone(), StyleRole::Muted)
        } else {
            RenderedSpan::new(" ".repeat(marker_width), StyleRole::Normal)
        };
        let mut all = vec![prefix];
        all.append(&mut rendered.spans);
        out.push(RenderedLine {
            spans: all,
            source_range: rendered.source_range.or(Some(f.range)),
            no_wrap: rendered.no_wrap,
            image: rendered.image,
        });
    }
}

fn raw_html(h: &RawHtmlBlock, out: &mut Vec<RenderedLine>) {
    for text in h.text.lines() {
        out.push(RenderedLine {
            spans: vec![RenderedSpan::new(expand_tabs(text), StyleRole::Muted)],
            source_range: Some(h.range),
            no_wrap: true,
            image: None,
        });
    }
}

fn line(spans: Vec<RenderedSpan>, range: SourceRange) -> RenderedLine {
    RenderedLine {
        spans,
        source_range: Some(range),
        no_wrap: false,
        image: None,
    }
}

/// Expand tabs to fixed tab stops so code indentation survives in a terminal
/// that would otherwise apply its own tab handling.
fn expand_tabs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut column = 0usize;
    for ch in text.chars() {
        if ch == '\t' {
            let advance = TAB_STOP - (column % TAB_STOP);
            out.extend(std::iter::repeat_n(' ', advance));
            column += advance;
        } else {
            out.push(ch);
            column += display_width(&ch.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Flavor;
    use crate::image::{CellSize, ImageSupport, Protocol};
    use crate::source::SourceText;
    use std::io::Write;
    use std::path::Path;

    fn png(dir: &Path, name: &str, width: u32, height: u32) {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        std::fs::File::create(dir.join(name))
            .expect("creating the fixture")
            .write_all(&bytes)
            .expect("writing the fixture");
    }

    fn context(base_dir: &Path, images: bool) -> InlineContext {
        InlineContext {
            base_dir: Some(base_dir.to_path_buf()),
            images: images.then_some(ImageSupport {
                protocol: Protocol::Kitty,
                cell: CellSize {
                    width: 10,
                    height: 20,
                },
            }),
            ..InlineContext::default()
        }
    }

    fn render(markdown: &str, ctx: &InlineContext) -> RenderedDocument {
        let document = crate::flavor::parse(SourceText::new(markdown.to_string()), Flavor::Gfm);
        layout_document(&document, LayoutOptions::new(80), ctx)
    }

    #[test]
    fn a_paragraph_holding_only_an_image_becomes_reserved_lines() {
        let dir = tempfile::tempdir().expect("temporary directory");
        png(dir.path(), "a.png", 200, 100);
        let rendered = render("![alt](a.png)\n", &context(dir.path(), true));

        assert_eq!(rendered.lines.len(), 5);
        let placement = rendered.lines[0]
            .image
            .as_ref()
            .expect("the first line carries the image");
        assert_eq!((placement.cols, placement.rows), (20, 5));
        // The rest of the block is reserved space, and every line points back
        // at the source so `--line` and `--watch` still work.
        assert!(rendered.lines[1..].iter().all(|l| l.image.is_none()));
        assert!(rendered.lines.iter().all(|l| l.source_range.is_some()));
        assert!(rendered.lines.iter().all(|l| l.text().is_empty()));
    }

    #[test]
    fn an_image_among_words_keeps_its_placeholder() {
        let dir = tempfile::tempdir().expect("temporary directory");
        png(dir.path(), "a.png", 200, 100);
        let rendered = render("see ![alt](a.png) here\n", &context(dir.path(), true));

        assert!(rendered.lines.iter().all(|l| l.image.is_none()));
        assert!(rendered.lines[0].text().contains("[image: alt]"));
    }

    /// Without a protocol the layout is byte for byte what it was before
    /// images existed, which is what the golden snapshots assert.
    #[test]
    fn a_terminal_without_images_gets_the_placeholder_layout() {
        let dir = tempfile::tempdir().expect("temporary directory");
        png(dir.path(), "a.png", 200, 100);
        let rendered = render("![alt](a.png)\n", &context(dir.path(), false));

        assert_eq!(rendered.lines.len(), 1);
        assert!(rendered.lines[0].image.is_none());
        assert!(rendered.lines[0].text().starts_with("[image: alt]"));
    }

    #[test]
    fn an_image_inside_a_quote_keeps_its_border_and_its_picture() {
        let dir = tempfile::tempdir().expect("temporary directory");
        png(dir.path(), "a.png", 200, 100);
        let rendered = render("> ![alt](a.png)\n", &context(dir.path(), true));

        assert!(rendered.lines[0].image.is_some());
        // The border is what the pager measures to place the image.
        assert_eq!(rendered.lines[0].text(), "│ ");
    }
}
