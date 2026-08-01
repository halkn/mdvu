//! Pager view state and the pure transitions the key map drives.
//!
//! The rendered document is owned by the app, not by this state, so scrolling
//! and searching never clone the surface.

use crate::layout::RenderedLine;
use crate::pager::search::Search;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    LineDown,
    LineUp,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
    ScrollLeft,
    ScrollRight,
    ResetHorizontal,
    StartSearch,
    NextMatch,
    PreviousMatch,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
}

/// How far a single horizontal scroll step moves.
const HORIZONTAL_STEP: usize = 8;

#[derive(Debug, Clone)]
pub struct PagerState {
    pub top: usize,
    pub left: usize,
    /// Rows available for the document, excluding the status bar.
    pub height: usize,
    pub width: usize,
    pub total: usize,
    pub widest: usize,
    pub mode: Mode,
    pub input: String,
    pub search: Search,
    pub status: Option<String>,
    /// Rendered line opened by `--line`, highlighted until the reader moves.
    pub initial_line: Option<usize>,
}

impl PagerState {
    pub fn new(total: usize, widest: usize, height: usize, width: usize) -> Self {
        Self {
            top: 0,
            left: 0,
            height: height.max(1),
            width: width.max(1),
            total,
            widest,
            mode: Mode::Normal,
            input: String::new(),
            search: Search::default(),
            status: None,
            initial_line: None,
        }
    }

    pub fn max_top(&self) -> usize {
        self.total.saturating_sub(self.height)
    }

    pub fn max_left(&self) -> usize {
        self.widest.saturating_sub(self.width)
    }

    pub fn clamp(&mut self) {
        self.top = self.top.min(self.max_top());
        self.left = self.left.min(self.max_left());
    }

    pub fn percentage(&self) -> usize {
        if self.total <= self.height {
            return 100;
        }
        let last = (self.top + self.height).min(self.total);
        last * 100 / self.total
    }

    /// Apply a navigation action. Returns `false` when the pager should exit.
    pub fn apply(&mut self, action: Action) -> bool {
        // Any deliberate movement retires the `--line` highlight.
        if !matches!(action, Action::Quit | Action::StartSearch) {
            self.initial_line = None;
        }
        self.status = None;
        match action {
            Action::Quit => return false,
            Action::LineDown => self.scroll_down(1),
            Action::LineUp => self.scroll_up(1),
            Action::HalfPageDown => self.scroll_down(self.height.div_ceil(2)),
            Action::HalfPageUp => self.scroll_up(self.height.div_ceil(2)),
            Action::PageDown => self.scroll_down(self.height),
            Action::PageUp => self.scroll_up(self.height),
            Action::Top => self.top = 0,
            Action::Bottom => self.top = self.max_top(),
            Action::ScrollLeft => self.left = self.left.saturating_sub(HORIZONTAL_STEP),
            Action::ScrollRight => {
                self.left = (self.left + HORIZONTAL_STEP).min(self.max_left());
            }
            Action::ResetHorizontal => self.left = 0,
            Action::StartSearch => {
                self.mode = Mode::Search;
                self.input.clear();
            }
            Action::NextMatch => self.jump(true),
            Action::PreviousMatch => self.jump(false),
        }
        true
    }

    fn scroll_down(&mut self, amount: usize) {
        self.top = (self.top + amount).min(self.max_top());
    }

    fn scroll_up(&mut self, amount: usize) {
        self.top = self.top.saturating_sub(amount);
    }

    fn jump(&mut self, forward: bool) {
        if !self.search.is_active() {
            return;
        }
        let found = if forward {
            self.search.next()
        } else {
            self.search.previous()
        };
        match found {
            Some(m) => self.reveal(m.line),
            None => self.status = Some(format!("no match: {}", self.search.query())),
        }
    }

    /// Confirm the search prompt. Keeps the viewport still when nothing matches.
    pub fn confirm_search(&mut self, lines: &[String]) {
        let query = std::mem::take(&mut self.input);
        self.mode = Mode::Normal;
        self.search.set_query(&query, lines);
        if !self.search.is_active() {
            return;
        }
        self.search.select_from(self.top);
        match self.search.current() {
            Some(m) => {
                self.initial_line = None;
                self.reveal(m.line);
            }
            None => self.status = Some(format!("no match: {}", self.search.query())),
        }
    }

    pub fn cancel_search(&mut self) {
        self.mode = Mode::Normal;
        self.input.clear();
    }

    /// Bring `line` into view, centring it when it is off screen.
    pub fn reveal(&mut self, line: usize) {
        if line < self.top || line >= self.top + self.height {
            self.top = line.saturating_sub(self.height / 2).min(self.max_top());
        }
    }

    /// Re-fit the viewport after a resize or re-layout, keeping `anchor` at the
    /// top of the screen.
    pub fn resize(&mut self, total: usize, widest: usize, height: usize, width: usize) {
        self.total = total;
        self.widest = widest;
        self.height = height.max(1);
        self.width = width.max(1);
        self.clamp();
    }
}

/// The source line shown at the top of the viewport, used as a resize anchor.
pub fn source_line_at(lines: &[RenderedLine], index: usize) -> Option<usize> {
    lines
        .iter()
        .skip(index)
        .find_map(|l| l.source_range.map(|r| r.line_start))
        .or_else(|| {
            lines[..index.min(lines.len())]
                .iter()
                .rev()
                .find_map(|l| l.source_range.map(|r| r.line_start))
        })
}

/// Rendered line to open for a 1-based source line: the line covering it, else
/// the nearest following mapped line, else the nearest preceding one.
pub fn rendered_line_for_source(lines: &[RenderedLine], source_line: usize) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    if let Some(index) = lines.iter().position(|l| {
        l.source_range
            .is_some_and(|r| r.line_start <= source_line && source_line <= r.line_end)
    }) {
        return Some(index);
    }
    if let Some(index) = lines
        .iter()
        .position(|l| l.source_range.is_some_and(|r| r.line_start > source_line))
    {
        return Some(index);
    }
    lines
        .iter()
        .rposition(|l| l.source_range.is_some_and(|r| r.line_end < source_line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{RenderedLine, RenderedSpan, StyleRole};
    use crate::source::SourceRange;

    fn state() -> PagerState {
        PagerState::new(100, 200, 10, 40)
    }

    fn mapped(line_start: usize, line_end: usize) -> RenderedLine {
        RenderedLine {
            spans: vec![RenderedSpan::new("x", StyleRole::Normal)],
            source_range: Some(SourceRange {
                byte_start: 0,
                byte_end: 1,
                line_start,
                line_end,
            }),
            no_wrap: false,
        }
    }

    #[test]
    fn vertical_scrolling_clamps_at_both_ends() {
        let mut s = state();
        s.apply(Action::LineUp);
        assert_eq!(s.top, 0);
        s.apply(Action::Bottom);
        assert_eq!(s.top, 90);
        s.apply(Action::LineDown);
        assert_eq!(s.top, 90);
        s.apply(Action::Top);
        assert_eq!(s.top, 0);
    }

    #[test]
    fn page_and_half_page_steps_differ() {
        let mut s = state();
        s.apply(Action::HalfPageDown);
        assert_eq!(s.top, 5);
        s.apply(Action::PageDown);
        assert_eq!(s.top, 15);
        s.apply(Action::HalfPageUp);
        assert_eq!(s.top, 10);
        s.apply(Action::PageUp);
        assert_eq!(s.top, 0);
    }

    #[test]
    fn horizontal_scrolling_clamps_and_resets() {
        let mut s = state();
        s.apply(Action::ScrollLeft);
        assert_eq!(s.left, 0);
        s.apply(Action::ScrollRight);
        assert_eq!(s.left, 8);
        for _ in 0..100 {
            s.apply(Action::ScrollRight);
        }
        assert_eq!(s.left, s.max_left());
        s.apply(Action::ResetHorizontal);
        assert_eq!(s.left, 0);
    }

    #[test]
    fn a_document_shorter_than_the_screen_does_not_scroll() {
        let mut s = PagerState::new(3, 10, 20, 40);
        s.apply(Action::Bottom);
        assert_eq!(s.top, 0);
        assert_eq!(s.percentage(), 100);
    }

    #[test]
    fn quit_stops_the_loop() {
        assert!(!state().apply(Action::Quit));
        assert!(state().apply(Action::LineDown));
    }

    #[test]
    fn resize_clamps_offsets_into_the_new_bounds() {
        let mut s = state();
        s.apply(Action::Bottom);
        s.apply(Action::ScrollRight);
        s.resize(20, 10, 10, 40);
        assert_eq!(s.top, 10);
        assert_eq!(s.left, 0);
    }

    #[test]
    fn searching_moves_to_the_match_and_reports_misses() {
        let lines: Vec<String> = (0..100)
            .map(|i| if i == 42 { "needle".into() } else { "x".into() })
            .collect();
        let mut s = state();
        s.input = "needle".into();
        s.confirm_search(&lines);
        assert_eq!(s.mode, Mode::Normal);
        assert!(s.top <= 42 && 42 < s.top + s.height);

        let before = s.top;
        s.input = "absent".into();
        s.confirm_search(&lines);
        assert_eq!(s.top, before);
        assert!(s.status.as_deref().unwrap().contains("no match"));
    }

    #[test]
    fn cancelling_a_search_returns_to_normal_mode() {
        let mut s = state();
        s.apply(Action::StartSearch);
        assert_eq!(s.mode, Mode::Search);
        s.input.push('a');
        s.cancel_search();
        assert_eq!(s.mode, Mode::Normal);
        assert!(s.input.is_empty());
    }

    #[test]
    fn movement_retires_the_initial_line_highlight() {
        let mut s = state();
        s.initial_line = Some(5);
        s.apply(Action::LineDown);
        assert_eq!(s.initial_line, None);
    }

    #[test]
    fn source_line_maps_to_the_covering_rendered_line() {
        let lines = vec![mapped(1, 1), mapped(3, 5), mapped(9, 9)];
        assert_eq!(rendered_line_for_source(&lines, 1), Some(0));
        assert_eq!(rendered_line_for_source(&lines, 4), Some(1));
        assert_eq!(rendered_line_for_source(&lines, 9), Some(2));
    }

    #[test]
    fn an_unmapped_source_line_falls_forward_then_backward() {
        let lines = vec![mapped(1, 1), mapped(5, 5)];
        // Line 3 has no rendered line; the next mapped one wins.
        assert_eq!(rendered_line_for_source(&lines, 3), Some(1));
        // Past the end, the last mapped line wins.
        assert_eq!(rendered_line_for_source(&lines, 99), Some(1));
        assert_eq!(rendered_line_for_source(&[], 1), None);
    }

    #[test]
    fn the_anchor_skips_unmapped_lines() {
        let lines = vec![mapped(1, 1), RenderedLine::blank(), mapped(7, 7)];
        assert_eq!(source_line_at(&lines, 1), Some(7));
        assert_eq!(source_line_at(&lines, 0), Some(1));
    }
}
