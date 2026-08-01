//! Azure DevOps Wiki extensions.
//!
//! The source is scanned line by line and split into ordinary Markdown
//! segments and virtual extension blocks. Ordinary segments keep their original
//! byte offsets and go to `pulldown-cmark` untouched, so source mapping
//! survives. Nothing is rewritten into synthetic Markdown.

use crate::diagnostic::Diagnostic;
use crate::markdown::model::*;
use crate::markdown::parser::parse_range;
use crate::source::SourceText;

pub const TOSP_MESSAGE: &str = "Child pages unavailable in single-file mode";

/// One piece of the document in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Markdown { start: usize, end: usize },
    Toc { start: usize, end: usize },
    Tosp { start: usize, end: usize },
    Container(Container),
    Details(Details),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Container {
    name: String,
    argument: String,
    body: String,
    start: usize,
    end: usize,
    /// A container that ran to the end of the file without a closing `:::`.
    unterminated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Details {
    summary: String,
    body_start: usize,
    body_end: usize,
    start: usize,
    end: usize,
    unterminated: bool,
}

pub fn parse(source: SourceText) -> Document {
    let mut blocks = Vec::new();
    let mut diagnostics = Vec::new();
    assemble(
        &source,
        0,
        source.as_str().len(),
        &mut blocks,
        &mut diagnostics,
    );
    expand_toc(&mut blocks, &mut diagnostics);

    Document {
        blocks,
        source,
        diagnostics,
    }
}

/// Scan one byte range and turn it into blocks. Recursive so a `<details>` body
/// gets the same extension handling as the top level.
fn assemble(
    source: &SourceText,
    range_start: usize,
    range_end: usize,
    blocks: &mut Vec<Block>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for segment in scan(source.as_str(), range_start, range_end) {
        match segment {
            Segment::Markdown { start, end } => {
                let (mut parsed, mut found) = parse_range(source, start, end);
                post_process_blocks(&mut parsed);
                blocks.append(&mut parsed);
                diagnostics.append(&mut found);
            }
            Segment::Toc { start, end } => blocks.push(Block::Toc(TocBlock {
                entries: Vec::new(),
                range: source.range(start, end),
            })),
            Segment::Tosp { start, end } => blocks.push(Block::Placeholder(PlaceholderBlock {
                kind: "child pages".to_string(),
                message: TOSP_MESSAGE.to_string(),
                source: None,
                range: source.range(start, end),
            })),
            Segment::Container(container) => {
                blocks.push(container_block(source, &container, diagnostics));
            }
            Segment::Details(details) => {
                blocks.push(details_block(source, &details, diagnostics));
            }
        }
    }
}

fn container_block(
    source: &SourceText,
    container: &Container,
    diagnostics: &mut Vec<Diagnostic>,
) -> Block {
    let range = source.range(container.start, container.end);
    if container.unterminated {
        diagnostics.push(Diagnostic::warning(
            format!("unterminated ::: {} block", container.name),
            Some(range),
        ));
    }
    match container.name.as_str() {
        "mermaid" => Block::Diagram(DiagramBlock::new(
            "mermaid".to_string(),
            container.body.clone(),
            range,
        )),
        "video" => Block::Placeholder(PlaceholderBlock {
            kind: "video".to_string(),
            message: "Video embeds are not supported in a terminal".to_string(),
            source: Some(container.body.trim().to_string()).filter(|s| !s.is_empty()),
            range,
        }),
        "query-table" => Block::Placeholder(PlaceholderBlock {
            kind: "query-table".to_string(),
            message: match query_id(&container.body) {
                Some(id) => format!("Work item query {id} requires Azure DevOps"),
                None => "Work item query requires Azure DevOps".to_string(),
            },
            source: None,
            range,
        }),
        other => {
            diagnostics.push(Diagnostic::warning(
                format!("unsupported ::: {other} block"),
                Some(range),
            ));
            Block::Placeholder(PlaceholderBlock {
                kind: other.to_string(),
                message: format!("Unsupported ::: {other} block"),
                // Malformed and unknown containers keep their body so nothing
                // is silently dropped.
                source: Some(container.body.clone()).filter(|s| !s.trim().is_empty()),
                range,
            })
        }
    }
}

fn query_id(body: &str) -> Option<String> {
    body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .find(|token| token.len() >= 32 || (token.contains('-') && token.len() >= 8))
        .map(str::to_string)
}

fn details_block(
    source: &SourceText,
    details: &Details,
    diagnostics: &mut Vec<Diagnostic>,
) -> Block {
    let range = source.range(details.start, details.end);
    if details.unterminated {
        diagnostics.push(Diagnostic::warning(
            "unterminated <details> block",
            Some(range),
        ));
    }
    // The body is scanned recursively so a nested `<details>` or `:::` block
    // inside it is handled rather than leaking as raw HTML.
    let mut blocks = Vec::new();
    assemble(
        source,
        details.body_start,
        details.body_end,
        &mut blocks,
        diagnostics,
    );
    let summary = if details.summary.trim().is_empty() {
        vec![Inline::Text("Details".to_string())]
    } else {
        inline_text(&details.summary)
    };
    Block::Details(DetailsBlock {
        summary,
        blocks,
        range,
    })
}

/// Fill the first `[[_TOC_]]` from the document headings and mark any later one
/// as ignored, matching Azure DevOps behaviour.
fn expand_toc(blocks: &mut [Block], diagnostics: &mut Vec<Diagnostic>) {
    let entries: Vec<TocEntry> = headings(blocks)
        .into_iter()
        .map(|h| TocEntry {
            level: h.level,
            text: h.plain.clone(),
            range: h.range,
        })
        .collect();

    let mut seen = false;
    for block in blocks.iter_mut() {
        let Block::Toc(toc) = block else {
            continue;
        };
        if seen {
            let range = toc.range;
            diagnostics.push(Diagnostic::warning(
                "only the first [[_TOC_]] is expanded",
                Some(range),
            ));
            *block = Block::Placeholder(PlaceholderBlock {
                kind: "toc".to_string(),
                message: "Ignored: only the first [[_TOC_]] is expanded".to_string(),
                source: None,
                range,
            });
            continue;
        }
        seen = true;
        toc.entries = entries.clone();
    }
}

// ---------------------------------------------------------------- scanning

fn scan(text: &str, range_start: usize, range_end: usize) -> Vec<Segment> {
    if range_start >= range_end || range_end > text.len() {
        return Vec::new();
    }
    let mut segments = Vec::new();
    let mut markdown_start = range_start;
    let mut fence: Option<String> = None;

    let push_markdown = |segments: &mut Vec<Segment>, start: usize, end: usize| {
        if start < end {
            segments.push(Segment::Markdown { start, end });
        }
    };

    // Offsets stay absolute so segments compose with the enclosing document.
    let lines: Vec<(usize, &str)> = line_offsets(&text[range_start..range_end])
        .into_iter()
        .map(|(offset, line)| (range_start + offset, line))
        .collect();
    let mut index = 0usize;
    while index < lines.len() {
        let (line_start, line) = lines[index];
        let trimmed = line.trim();

        // Everything inside a code fence is literal, including Azure macros.
        if let Some(marker) = &fence {
            if trimmed.starts_with(marker.as_str()) {
                fence = None;
            }
            index += 1;
            continue;
        }
        if let Some(marker) = opening_fence(line) {
            fence = Some(marker);
            index += 1;
            continue;
        }

        if trimmed == "[[_TOC_]]" {
            push_markdown(&mut segments, markdown_start, line_start);
            segments.push(Segment::Toc {
                start: line_start,
                end: line_start + line.len(),
            });
            markdown_start = next_start(&lines, index);
            index += 1;
            continue;
        }
        if trimmed == "[[_TOSP_]]" {
            push_markdown(&mut segments, markdown_start, line_start);
            segments.push(Segment::Tosp {
                start: line_start,
                end: line_start + line.len(),
            });
            markdown_start = next_start(&lines, index);
            index += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix(":::") {
            let header = rest.trim();
            // A bare `:::` with no name closes nothing here; treat it as text.
            if !header.is_empty() {
                push_markdown(&mut segments, markdown_start, line_start);
                let (container, next) = read_container(&lines, index, header, line_start);
                segments.push(Segment::Container(container));
                markdown_start = next_start(&lines, next.saturating_sub(1));
                index = next;
                continue;
            }
        }
        if trimmed.starts_with("<details") {
            push_markdown(&mut segments, markdown_start, line_start);
            let (details, next) = read_details(&lines, index, line_start);
            segments.push(Segment::Details(details));
            markdown_start = next_start(&lines, next.saturating_sub(1));
            index = next;
            continue;
        }

        index += 1;
    }

    push_markdown(&mut segments, markdown_start, range_end);
    segments
}

fn line_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for line in text.split_inclusive('\n') {
        out.push((start, line.trim_end_matches(['\n', '\r'])));
        start += line.len();
    }
    if out.is_empty() {
        out.push((0, ""));
    }
    out
}

fn next_start(lines: &[(usize, &str)], index: usize) -> usize {
    lines
        .get(index + 1)
        .map(|(start, _)| *start)
        .unwrap_or(usize::MAX)
}

fn opening_fence(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    // More than three leading spaces makes it indented code, not a fence.
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    for marker in ["```", "~~~"] {
        if trimmed.starts_with(marker) {
            return Some(marker.to_string());
        }
    }
    None
}

fn read_container(
    lines: &[(usize, &str)],
    index: usize,
    header: &str,
    start: usize,
) -> (Container, usize) {
    let (name, argument) = match header.split_once(char::is_whitespace) {
        Some((name, rest)) => (name.to_string(), rest.trim().to_string()),
        None => (header.to_string(), String::new()),
    };
    let mut body = String::new();
    let mut cursor = index + 1;
    // Where the body would end if no closing `:::` is ever found. Stopping at
    // the first blank line keeps an unterminated container from swallowing the
    // rest of the document, which would make everything after it unreadable.
    let mut fallback: Option<(usize, String, usize)> = None;
    let mut fence: Option<String> = None;
    while cursor < lines.len() {
        let (line_start, line) = lines[cursor];
        // A `:::` inside a code fence is literal text, not a closing marker.
        if let Some(marker) = &fence {
            if line.trim().starts_with(marker.as_str()) {
                fence = None;
            }
            body.push_str(line);
            body.push('\n');
            cursor += 1;
            continue;
        }
        if let Some(marker) = opening_fence(line) {
            fence = Some(marker);
            body.push_str(line);
            body.push('\n');
            cursor += 1;
            continue;
        }
        if line.trim().is_empty() && fallback.is_none() {
            fallback = Some((cursor, body.clone(), line_start));
        }
        if line.trim() == ":::" {
            return (
                Container {
                    name,
                    argument,
                    body,
                    start,
                    end: line_start + line.len(),
                    unterminated: false,
                },
                cursor + 1,
            );
        }
        body.push_str(line);
        body.push('\n');
        cursor += 1;
    }
    match fallback {
        Some((blank_cursor, partial, blank_start)) => (
            Container {
                name,
                argument,
                body: partial,
                start,
                end: blank_start,
                unterminated: true,
            },
            blank_cursor,
        ),
        None => {
            let end = lines.last().map(|(s, l)| s + l.len()).unwrap_or(start);
            (
                Container {
                    name,
                    argument,
                    body,
                    start,
                    end,
                    unterminated: true,
                },
                cursor,
            )
        }
    }
}

const CLOSE_TAG: &str = "</details>";

/// Find the extent of a `<details>` block.
///
/// Only the outermost `<summary>` belongs to this block, and the body is
/// everything between the opening tag (or the summary, when present) and the
/// `</details>` that returns the nesting depth to zero. Both bounds are byte
/// offsets rather than whole lines, so a one-line block keeps its body.
fn read_details(lines: &[(usize, &str)], index: usize, start: usize) -> (Details, usize) {
    let mut summary = String::new();
    let mut summary_seen = false;
    let mut body_start: Option<usize> = None;
    let mut depth = 0usize;
    let mut cursor = index;
    let mut last_end = start;

    while cursor < lines.len() {
        let (line_start, line) = lines[cursor];
        depth += line.matches("<details").count();

        if !summary_seen && let Some((text, after)) = extract_summary(line) {
            summary = text;
            summary_seen = true;
            body_start = Some(line_start + after);
        }
        // No summary yet: the body begins right after the opening tag, so a
        // `<details>` without a `<summary>` still keeps its content.
        if body_start.is_none()
            && let Some(gt) = line.find('>')
        {
            body_start = Some(line_start + gt + 1);
        }

        let mut from = 0usize;
        while let Some(relative) = line[from..].find(CLOSE_TAG) {
            let at = from + relative;
            depth = depth.saturating_sub(1);
            if depth == 0 {
                let close = line_start + at;
                let body_start = body_start.unwrap_or(close).min(close);
                return (
                    Details {
                        summary,
                        body_start,
                        body_end: close,
                        start,
                        end: close + CLOSE_TAG.len(),
                        unterminated: false,
                    },
                    cursor + 1,
                );
            }
            from = at + CLOSE_TAG.len();
        }

        last_end = line_start + line.len();
        cursor += 1;
    }
    (
        Details {
            summary,
            body_start: body_start.unwrap_or(start).min(last_end),
            body_end: last_end,
            start,
            end: last_end,
            unterminated: true,
        },
        cursor,
    )
}

/// The summary text and the byte offset just past `</summary>` on this line.
fn extract_summary(line: &str) -> Option<(String, usize)> {
    let open = line.find("<summary>")? + "<summary>".len();
    let close = line[open..].find("</summary>")? + open;
    Some((
        line[open..close].trim().to_string(),
        close + "</summary>".len(),
    ))
}

// ------------------------------------------------------- inline extensions

/// Rewrite plain text runs into Azure inline constructs. Only `Inline::Text`
/// is touched, so code spans and link destinations are never reinterpreted.
fn post_process_blocks(blocks: &mut [Block]) {
    for block in blocks.iter_mut() {
        match block {
            Block::Heading(h) => post_process_inlines(&mut h.content),
            Block::Paragraph(p) => post_process_inlines(&mut p.content),
            Block::Quote(q) => post_process_blocks(&mut q.blocks),
            Block::Details(d) => post_process_blocks(&mut d.blocks),
            Block::Footnote(f) => post_process_blocks(&mut f.blocks),
            Block::List(l) => {
                for item in &mut l.items {
                    post_process_blocks(&mut item.blocks);
                }
            }
            Block::Table(t) => {
                for cell in t.header.iter_mut().chain(t.rows.iter_mut().flatten()) {
                    post_process_inlines(&mut cell.content);
                    expand_cell_breaks(&mut cell.content);
                }
            }
            Block::Code(c) if c.language.as_deref() == Some("mermaid") => {
                *block = Block::Diagram(DiagramBlock::new(
                    "mermaid".to_string(),
                    c.text.clone(),
                    c.range,
                ));
            }
            _ => {}
        }
    }
}

fn post_process_inlines(inlines: &mut Vec<Inline>) {
    let mut out = Vec::with_capacity(inlines.len());
    for inline in inlines.drain(..) {
        match inline {
            Inline::Text(text) => out.extend(split_text(&text)),
            Inline::Strong(mut c) => {
                post_process_inlines(&mut c);
                out.push(Inline::Strong(c));
            }
            Inline::Emphasis(mut c) => {
                post_process_inlines(&mut c);
                out.push(Inline::Emphasis(c));
            }
            Inline::Strikethrough(mut c) => {
                post_process_inlines(&mut c);
                out.push(Inline::Strikethrough(c));
            }
            Inline::Link(mut l) => {
                post_process_inlines(&mut l.content);
                out.push(Inline::Link(l));
            }
            other => out.push(other),
        }
    }
    *inlines = out;
}

/// `<br/>` inside a table cell becomes a real line break.
fn expand_cell_breaks(inlines: &mut Vec<Inline>) {
    let mut out = Vec::with_capacity(inlines.len());
    for inline in inlines.drain(..) {
        match &inline {
            Inline::RawHtml(html) if is_br(html) => out.push(Inline::HardBreak),
            _ => out.push(inline),
        }
    }
    *inlines = out;
}

fn is_br(html: &str) -> bool {
    let normalized: String = html
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '/')
        .collect();
    normalized.eq_ignore_ascii_case("<br>")
}

/// Split a text run on `#123`, `@alias` and `$math$`.
fn split_text(text: &str) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let mut literal = String::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;

    while i < text.len() {
        if !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &text[i..];
        let at_start = i == 0 || is_boundary(bytes[i - 1]);

        if at_start && let Some(taken) = take_math(rest) {
            flush(&mut literal, &mut out);
            out.push(Inline::Math(taken.to_string()));
            i += taken.len();
            continue;
        }
        // An id that does not fit in u64 is not a work item reference; keeping
        // it as literal text preserves what the author wrote.
        if at_start
            && rest.starts_with('#')
            && let Some(id) = take_work_item(rest)
            && let Ok(number) = id.parse::<u64>()
        {
            flush(&mut literal, &mut out);
            out.push(Inline::WorkItem(number));
            i += id.len() + 1;
            continue;
        }
        if at_start
            && rest.starts_with('@')
            && let Some(alias) = take_mention(rest)
        {
            flush(&mut literal, &mut out);
            out.push(Inline::Mention(alias.to_string()));
            i += alias.len() + 1;
            continue;
        }

        let ch = rest.chars().next().expect("char boundary");
        literal.push(ch);
        i += ch.len_utf8();
    }
    flush(&mut literal, &mut out);
    out
}

fn flush(literal: &mut String, out: &mut Vec<Inline>) {
    if !literal.is_empty() {
        out.push(Inline::Text(std::mem::take(literal)));
    }
}

fn is_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'(' | b'[' | b'{' | b',' | b';' | b':')
}

fn take_work_item(rest: &str) -> Option<&str> {
    let digits: &str = rest[1..]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("");
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

fn take_mention(rest: &str) -> Option<&str> {
    let alias: &str = rest[1..]
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '@')))
        .next()
        .unwrap_or("");
    if alias.len() < 2 { None } else { Some(alias) }
}

/// Match `$...$` or `$$...$$`, returning the whole span including delimiters.
fn take_math(rest: &str) -> Option<&str> {
    let delimiter = if rest.starts_with("$$") { "$$" } else { "$" };
    if !rest.starts_with(delimiter) {
        return None;
    }
    let body_start = delimiter.len();
    let body = &rest[body_start..];
    // A digit right after the delimiter is far more likely to be a price than
    // the start of a formula, and prose with two prices would otherwise style
    // everything between them as math.
    if body.is_empty()
        || body.starts_with(char::is_whitespace)
        || body.starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    let end = body.find(delimiter)?;
    if end == 0 {
        return None;
    }
    Some(&rest[..body_start + end + delimiter.len()])
}

/// Parse a summary string as inline Markdown without a full document pass.
fn inline_text(summary: &str) -> Vec<Inline> {
    let source = SourceText::new(summary.to_string());
    let (blocks, _) = parse_range(&source, 0, summary.len());
    let mut inlines = match blocks.into_iter().next() {
        Some(Block::Paragraph(p)) => p.content,
        Some(Block::Heading(h)) => h.content,
        _ => vec![Inline::Text(summary.to_string())],
    };
    post_process_inlines(&mut inlines);
    inlines
}
