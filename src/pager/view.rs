//! Frame rendering. Everything drawn here comes from the already laid out
//! surface; no Markdown parsing or diagram rendering happens per frame.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color as RatColor, Modifier, Style as RatStyle};
use ratatui::text::{Line as RatLine, Span as RatSpan};
use ratatui::widgets::Paragraph;
use unicode_segmentation::UnicodeSegmentation;

use crate::layout::theme::{Color, Style, Theme};
use crate::layout::wrap::display_width;
use crate::layout::{RenderedLine, StyleRole};
use crate::pager::state::{Mode, PagerState, source_line_at};

/// A run of text on screen with the role it should be drawn in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleSpan {
    pub text: String,
    pub role: StyleRole,
    pub highlighted: bool,
}

pub struct ViewContext<'a> {
    pub title: &'a str,
    pub flavor: &'a str,
    pub source_lines: usize,
    pub diagnostics: usize,
    pub theme: Theme,
}

pub fn draw(frame: &mut Frame, lines: &[RenderedLine], state: &PagerState, ctx: &ViewContext<'_>) {
    let [body, status] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    let rows: Vec<RatLine> = (state.top..(state.top + state.height).min(lines.len()))
        .map(|index| {
            let highlights: Vec<(usize, usize)> = state
                .search
                .matches_on(index)
                .map(|m| (m.start, m.end))
                .collect();
            let initial = state.initial_line == Some(index);
            let spans = visible(&lines[index], state.left, body.width as usize, &highlights);
            RatLine::from(
                spans
                    .into_iter()
                    .map(|span| {
                        let role = if initial && span.role == StyleRole::Normal {
                            StyleRole::InitialLine
                        } else {
                            span.role
                        };
                        let mut style = ctx.theme.style(role);
                        if span.highlighted {
                            style = ctx.theme.style(StyleRole::SearchMatch);
                        }
                        RatSpan::styled(span.text, convert(style))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    let current = source_line_at(lines, state.top).unwrap_or(0);
    frame.render_widget(Paragraph::new(rows), body);
    frame.render_widget(
        status_bar(state, ctx, current, status.width as usize),
        status,
    );
}

/// Slice one rendered line to the horizontal window, splitting spans where a
/// search match starts or ends. Grapheme boundaries and display widths are
/// respected so wide characters are never cut in half.
pub fn visible(
    line: &RenderedLine,
    left: usize,
    width: usize,
    highlights: &[(usize, usize)],
) -> Vec<VisibleSpan> {
    let mut out: Vec<VisibleSpan> = Vec::new();
    let mut column = 0usize;
    let mut offset = 0usize;

    for span in &line.spans {
        for grapheme in span.text.graphemes(true) {
            let cell_width = display_width(grapheme);
            let start = offset;
            offset += grapheme.len();

            // Skip everything scrolled off to the left.
            if column + cell_width <= left {
                column += cell_width;
                continue;
            }
            if column >= left + width {
                return out;
            }
            // A wide character straddling the left edge is dropped rather than
            // drawn half off screen.
            if column < left {
                column += cell_width;
                continue;
            }
            column += cell_width;

            let highlighted = highlights.iter().any(|(s, e)| start >= *s && start < *e);
            match out.last_mut() {
                Some(last) if last.role == span.role && last.highlighted == highlighted => {
                    last.text.push_str(grapheme);
                }
                _ => out.push(VisibleSpan {
                    text: grapheme.to_string(),
                    role: span.role,
                    highlighted,
                }),
            }
        }
    }
    out
}

fn status_bar<'a>(
    state: &PagerState,
    ctx: &ViewContext<'_>,
    source_line: usize,
    width: usize,
) -> Paragraph<'a> {
    let style = convert(ctx.theme.style(StyleRole::Status));
    if state.mode == Mode::Search {
        return Paragraph::new(RatLine::from(RatSpan::styled(
            format!("/{}", state.input),
            style,
        )));
    }

    let mut right = format!(
        "{}  {}/{}  {}%",
        ctx.flavor,
        source_line,
        ctx.source_lines,
        state.percentage()
    );
    if ctx.diagnostics > 0 {
        right.push_str(&format!("  {} diagnostics", ctx.diagnostics));
    }
    if let Some(status) = &state.status {
        right.push_str(&format!("  {status}"));
    }
    if state.search.is_active() {
        right.push_str(&format!(
            "  /{} ({})",
            state.search.query(),
            state.search.count()
        ));
    }

    // The status bar stays on one line: the path gives up its head first.
    let budget = width.saturating_sub(display_width(&right) + 2);
    let title = elide_start(ctx.title, budget);
    let gap = width.saturating_sub(display_width(&title) + display_width(&right));
    Paragraph::new(RatLine::from(RatSpan::styled(
        format!("{title}{}{right}", " ".repeat(gap.max(1))),
        style,
    )))
}

/// Drop leading path components so the file name stays visible.
pub fn elide_start(text: &str, budget: usize) -> String {
    if display_width(text) <= budget {
        return text.to_string();
    }
    if budget <= 1 {
        return String::new();
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 1usize; // the leading ellipsis
    for grapheme in text.graphemes(true).rev() {
        let w = display_width(grapheme);
        if used + w > budget {
            break;
        }
        used += w;
        kept.push(grapheme);
    }
    kept.reverse();
    format!("…{}", kept.concat())
}

fn convert(style: Style) -> RatStyle {
    let mut out = RatStyle::default();
    if let Some(fg) = style.fg {
        out = out.fg(color(fg));
    }
    if let Some(bg) = style.bg {
        out = out.bg(color(bg));
    }
    let mut modifiers = Modifier::empty();
    if style.bold {
        modifiers |= Modifier::BOLD;
    }
    if style.dim {
        modifiers |= Modifier::DIM;
    }
    if style.italic {
        modifiers |= Modifier::ITALIC;
    }
    if style.underline {
        modifiers |= Modifier::UNDERLINED;
    }
    if style.reverse {
        modifiers |= Modifier::REVERSED;
    }
    if style.strikethrough {
        modifiers |= Modifier::CROSSED_OUT;
    }
    out.add_modifier(modifiers)
}

fn color(value: Color) -> RatColor {
    match value {
        Color::Red => RatColor::Red,
        Color::Green => RatColor::Green,
        Color::Yellow => RatColor::Yellow,
        Color::Blue => RatColor::Blue,
        Color::Magenta => RatColor::Magenta,
        Color::BrightBlack => RatColor::DarkGray,
        Color::BrightGreen => RatColor::LightGreen,
        Color::BrightYellow => RatColor::LightYellow,
        Color::BrightBlue => RatColor::LightBlue,
        Color::BrightCyan => RatColor::LightCyan,
    }
}

/// Widest rendered line, used to bound horizontal scrolling.
pub fn widest_line(lines: &[RenderedLine]) -> usize {
    lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| display_width(&s.text))
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::RenderedSpan;

    fn line(text: &str) -> RenderedLine {
        RenderedLine {
            spans: vec![RenderedSpan::new(text, StyleRole::Normal)],
            source_range: None,
            no_wrap: false,
        }
    }

    fn text_of(spans: &[VisibleSpan]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn the_window_starts_at_the_horizontal_offset() {
        let l = line("abcdefghij");
        assert_eq!(text_of(&visible(&l, 0, 4, &[])), "abcd");
        assert_eq!(text_of(&visible(&l, 4, 4, &[])), "efgh");
        assert_eq!(text_of(&visible(&l, 8, 4, &[])), "ij");
    }

    #[test]
    fn wide_characters_are_never_split() {
        let l = line("あいうえお");
        assert_eq!(text_of(&visible(&l, 0, 4, &[])), "あい");
        // Offset 1 lands mid-character; that character is dropped, not halved.
        assert_eq!(text_of(&visible(&l, 1, 4, &[])), "いう");
    }

    #[test]
    fn scrolling_past_the_end_yields_nothing() {
        assert!(visible(&line("abc"), 10, 4, &[]).is_empty());
    }

    #[test]
    fn matches_are_marked_for_highlighting() {
        let l = line("find the needle here");
        let spans = visible(&l, 0, 40, &[(9, 15)]);
        let marked: String = spans
            .iter()
            .filter(|s| s.highlighted)
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(marked, "needle");
        assert_eq!(text_of(&spans), "find the needle here");
    }

    #[test]
    fn a_long_path_keeps_its_tail() {
        let elided = elide_start("a/very/long/path/to/document.md", 14);
        assert!(elided.ends_with("document.md"));
        assert!(elided.starts_with('…'));
        assert!(display_width(&elided) <= 14);
    }

    #[test]
    fn a_short_path_is_untouched() {
        assert_eq!(elide_start("doc.md", 20), "doc.md");
    }

    #[test]
    fn widest_line_measures_display_width() {
        let lines = vec![line("abc"), line("日本語")];
        assert_eq!(widest_line(&lines), 6);
    }
}
