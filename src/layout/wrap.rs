//! Display-width aware line breaking.
//!
//! Widths come from `unicode-width` and breaks land on grapheme boundaries, so
//! byte length and scalar count are never used as a width. Japanese text breaks
//! between characters rather than between words, with a best-effort kinsoku
//! pass that keeps closing punctuation off the start of a line and opening
//! punctuation off the end.

use std::rc::Rc;
use unicode_segmentation::UnicodeSegmentation;

use unicode_width::UnicodeWidthStr;

use crate::layout::{RenderedSpan, StyleRole};

/// Must not begin a line.
const CLOSING: &[char] = &['、', '。', '）', '」', '』', '】', '〕', '〉', '》'];
/// Must not end a line.
const OPENING: &[char] = &['（', '「', '『', '【', '〔', '〈', '《'];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cell {
    text: String,
    role: StyleRole,
    width: usize,
    /// Shared so a long link does not allocate its URL once per grapheme.
    link: Option<Rc<str>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkKind {
    Space,
    Word,
    Closing,
    Opening,
}

#[derive(Debug, Clone)]
struct Chunk {
    cells: Vec<Cell>,
    width: usize,
    kind: ChunkKind,
    /// Holds a wide character. A narrow run must not join such a chunk, or the
    /// CJK/Latin boundary would become unbreakable.
    wide: bool,
}

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn spans_width(spans: &[RenderedSpan]) -> usize {
    spans.iter().map(|s| display_width(&s.text)).sum()
}

/// Width of the widest run that has no break opportunity inside it, such as a
/// long identifier. Used as a table column's intrinsic minimum: narrower than
/// this and the run would have to be split mid-word.
pub fn min_unbreakable_width(spans: &[RenderedSpan]) -> usize {
    chunk(cells(spans))
        .iter()
        .filter(|c| c.kind != ChunkKind::Space)
        .map(|c| c.width)
        .max()
        .unwrap_or(0)
}

/// Break `spans` into lines no wider than `width`. A `width` of zero yields a
/// single unbroken line.
pub fn wrap_spans(spans: &[RenderedSpan], width: usize) -> Vec<Vec<RenderedSpan>> {
    if width == 0 {
        return vec![merge(cells(spans))];
    }
    let chunks = chunk(cells(spans));
    if chunks.is_empty() {
        return vec![Vec::new()];
    }

    let mut lines: Vec<Vec<Cell>> = Vec::new();
    let mut line: Vec<Chunk> = Vec::new();
    let mut used = 0usize;

    for ch in chunks {
        // Leading whitespace on a continuation line is dropped.
        if line.is_empty() && ch.kind == ChunkKind::Space {
            continue;
        }
        if used + ch.width <= width || line.is_empty() {
            used += ch.width;
            line.push(ch);
            continue;
        }
        let carried = break_line(&mut line, &ch);
        lines.push(flatten(std::mem::take(&mut line)));
        line.extend(carried);
        used = line.iter().map(|c| c.width).sum::<usize>() + ch.width;
        line.push(ch);
    }
    if !line.is_empty() {
        lines.push(flatten(line));
    }

    lines
        .into_iter()
        .flat_map(|cells| split_oversized(cells, width))
        .map(merge)
        .collect()
}

/// Decide which trailing chunks of the current line move down with `next`.
/// Returns the chunks to carry over. Never empties the line, so wrapping always
/// makes progress.
fn break_line(line: &mut Vec<Chunk>, next: &Chunk) -> Vec<Chunk> {
    // Trailing spaces never survive a break.
    while line.last().map(|c| c.kind) == Some(ChunkKind::Space) {
        line.pop();
    }
    let mut carried = Vec::new();
    // Opening punctuation must not sit at the end of a line: push it down.
    if line.len() > 1 && line.last().map(|c| c.kind) == Some(ChunkKind::Opening) {
        carried.push(line.pop().expect("checked above"));
    }
    // Closing punctuation must not start a line: pull the preceding chunk down
    // with it so the pair stays together.
    if next.kind == ChunkKind::Closing && line.len() > 1 && carried.is_empty() {
        carried.push(line.pop().expect("checked above"));
    }
    carried
}

fn cells(spans: &[RenderedSpan]) -> Vec<Cell> {
    let mut out = Vec::new();
    for span in spans {
        let link: Option<Rc<str>> = span.link.as_deref().map(Rc::from);
        for g in span.text.graphemes(true) {
            out.push(Cell {
                text: g.to_string(),
                role: span.role,
                width: display_width(g),
                link: link.clone(),
            });
        }
    }
    out
}

fn chunk(cells: Vec<Cell>) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    for cell in cells {
        let kind = cell_kind(&cell);
        let standalone = kind != ChunkKind::Word || cell.width > 1;
        let merges = match chunks.last() {
            Some(last) => {
                last.kind == kind
                    && kind != ChunkKind::Closing
                    && kind != ChunkKind::Opening
                    && !(standalone && kind == ChunkKind::Word)
                    && !(last.wide && kind == ChunkKind::Word)
            }
            None => false,
        };
        if merges {
            let last = chunks.last_mut().expect("checked above");
            last.width += cell.width;
            last.wide |= cell.width > 1;
            last.cells.push(cell);
        } else {
            chunks.push(Chunk {
                width: cell.width,
                wide: cell.width > 1,
                cells: vec![cell],
                kind,
            });
        }
    }
    chunks
}

fn cell_kind(cell: &Cell) -> ChunkKind {
    let first = cell.text.chars().next().unwrap_or(' ');
    if CLOSING.contains(&first) {
        ChunkKind::Closing
    } else if OPENING.contains(&first) {
        ChunkKind::Opening
    } else if cell.text.chars().all(char::is_whitespace) {
        ChunkKind::Space
    } else {
        ChunkKind::Word
    }
}

fn flatten(chunks: Vec<Chunk>) -> Vec<Cell> {
    let mut cells: Vec<Cell> = chunks.into_iter().flat_map(|c| c.cells).collect();
    while cells
        .last()
        .is_some_and(|c| c.text.chars().all(char::is_whitespace))
    {
        cells.pop();
    }
    cells
}

/// A run wider than the line, such as a long URL, has no break opportunity, so
/// it is split by cells. Content is never dropped.
fn split_oversized(cells: Vec<Cell>, width: usize) -> Vec<Vec<Cell>> {
    if cells.iter().map(|c| c.width).sum::<usize>() <= width {
        return vec![cells];
    }
    let mut lines = Vec::new();
    let mut current: Vec<Cell> = Vec::new();
    let mut used = 0usize;
    for cell in cells {
        if used + cell.width > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            used = 0;
        }
        used += cell.width;
        current.push(cell);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn merge(cells: Vec<Cell>) -> Vec<RenderedSpan> {
    let mut spans: Vec<RenderedSpan> = Vec::new();
    for cell in cells {
        match spans.last_mut() {
            Some(last)
                if last.role == cell.role && last.link.as_deref() == cell.link.as_deref() =>
            {
                last.text.push_str(&cell.text)
            }
            _ => {
                let span = RenderedSpan::new(cell.text, cell.role);
                spans.push(match cell.link {
                    Some(link) => span.with_link(link.as_ref()),
                    None => span,
                });
            }
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(text: &str) -> Vec<RenderedSpan> {
        vec![RenderedSpan::new(text, StyleRole::Normal)]
    }

    fn texts(lines: &[Vec<RenderedSpan>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect())
            .collect()
    }

    fn widths(lines: &[Vec<RenderedSpan>]) -> Vec<usize> {
        lines.iter().map(|l| spans_width(l)).collect()
    }

    #[test]
    fn measures_wide_characters_as_two_columns() {
        assert_eq!(display_width("日本語"), 6);
        assert_eq!(display_width("abc"), 3);
    }

    #[test]
    fn breaks_latin_text_between_words() {
        let out = wrap_spans(&normal("the quick brown fox"), 10);
        assert_eq!(texts(&out), vec!["the quick", "brown fox"]);
    }

    #[test]
    fn never_exceeds_the_requested_width() {
        let out = wrap_spans(&normal("日本語のテキストを折り返します"), 8);
        assert!(widths(&out).iter().all(|w| *w <= 8), "{:?}", widths(&out));
    }

    #[test]
    fn breaks_japanese_between_characters_without_adding_spaces() {
        let out = wrap_spans(&normal("日本語テキスト"), 6);
        assert_eq!(texts(&out), vec!["日本語", "テキス", "ト"]);
    }

    #[test]
    fn keeps_closing_punctuation_off_the_start_of_a_line() {
        // Width 8 would otherwise leave "。" alone at the head of line 2.
        let out = wrap_spans(&normal("あいうえお。かき"), 8);
        for line in texts(&out).iter().skip(1) {
            assert!(!line.starts_with('。'), "{line:?}");
        }
    }

    #[test]
    fn keeps_opening_punctuation_off_the_end_of_a_line() {
        let out = wrap_spans(&normal("あいう「かきくけこ」"), 8);
        for line in texts(&out) {
            assert!(!line.ends_with('「'), "{line:?}");
        }
    }

    #[test]
    fn splits_a_word_longer_than_the_line() {
        let out = wrap_spans(&normal("aaaaaaaaaaaa"), 5);
        assert!(widths(&out).iter().all(|w| *w <= 5));
        assert_eq!(texts(&out).concat(), "aaaaaaaaaaaa");
    }

    #[test]
    fn preserves_roles_across_a_break() {
        let spans = vec![
            RenderedSpan::new("hello ", StyleRole::Normal),
            RenderedSpan::new("world", StyleRole::Strong),
        ];
        let out = wrap_spans(&spans, 6);
        assert_eq!(texts(&out), vec!["hello", "world"]);
        assert_eq!(out[1][0].role, StyleRole::Strong);
    }

    #[test]
    fn a_link_survives_a_line_break_and_does_not_bleed_into_its_neighbour() {
        let spans = vec![
            RenderedSpan::new("alpha beta", StyleRole::Link).with_link("https://example.com"),
            RenderedSpan::new(" gamma", StyleRole::Link),
        ];
        let url = || Some("https://example.com".to_string());
        let links = |lines: &[Vec<RenderedSpan>]| -> Vec<Vec<Option<String>>> {
            lines
                .iter()
                .map(|line| line.iter().map(|s| s.link.clone()).collect())
                .collect()
        };

        // Both halves of a broken label stay clickable.
        let broken = wrap_spans(&spans[..1], 7);
        assert_eq!(links(&broken), vec![vec![url()], vec![url()]]);

        // The unlinked run stays a separate span even though the role matches.
        let joined = wrap_spans(&spans, 40);
        assert_eq!(links(&joined), vec![vec![url(), None]]);
    }

    #[test]
    fn merges_adjacent_cells_of_the_same_role() {
        let out = wrap_spans(&normal("abc def"), 80);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 1);
    }

    #[test]
    fn a_latin_run_may_break_away_from_preceding_japanese() {
        // The Latin run must not fuse to the wide character before it, which
        // would leave an unbreakable chunk and waste most of the line.
        let out = wrap_spans(&normal("日本語日本語abcdefghij日本語です"), 20);
        assert!(widths(&out).iter().all(|w| *w <= 20));
        assert_eq!(texts(&out)[0], "日本語日本語");
        assert_eq!(texts(&out).concat(), "日本語日本語abcdefghij日本語です");
    }

    #[test]
    fn mixed_japanese_and_latin_stays_within_width() {
        let out = wrap_spans(&normal("これは mixed テキスト with English です"), 12);
        assert!(widths(&out).iter().all(|w| *w <= 12), "{:?}", texts(&out));
        assert!(texts(&out).concat().contains("English"));
    }

    #[test]
    fn empty_input_yields_one_empty_line() {
        assert_eq!(wrap_spans(&[], 10), vec![Vec::new()]);
    }

    #[test]
    fn zero_width_does_not_wrap() {
        let out = wrap_spans(&normal("a b c"), 0);
        assert_eq!(texts(&out), vec!["a b c"]);
    }
}
