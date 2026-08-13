//! Pager view state and the pure transitions the key map drives.
//!
//! The rendered document is owned by the app, not by this state, so scrolling
//! and searching never clone the surface.

use unicode_segmentation::UnicodeSegmentation;

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
    ToggleOutline,
    ToggleHelp,
    Quit,
}

/// An edit to the search prompt. The bindings match the readline keys a shell
/// already gives the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchEdit {
    Backspace,
    Delete,
    DeleteWordBefore,
    KillToStart,
    KillToEnd,
    CaretLeft,
    CaretRight,
    CaretStart,
    CaretEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Outline,
    Help,
}

/// One heading in the outline overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub level: u8,
    pub text: String,
    /// Rendered line the heading starts on. Resolved again after a re-layout.
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    pub items: Vec<OutlineItem>,
    pub selected: usize,
}

impl Outline {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn move_by(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let last = self.items.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    /// Select the last heading at or above `line`, so opening the overlay lands
    /// on the section currently being read.
    fn select_for_line(&mut self, line: usize) {
        self.selected = self
            .items
            .iter()
            .rposition(|item| item.line <= line)
            .unwrap_or(0);
    }
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
    /// Byte offset of the caret in `input`, always on a grapheme boundary.
    pub caret: usize,
    pub search: Search,
    /// Matches for the query being typed. Kept apart from `search` so that
    /// cancelling the prompt leaves the confirmed search untouched.
    preview: Search,
    /// Viewport when the prompt opened, so an incremental search starts from
    /// the reader's place and cancelling returns them to it.
    search_origin: Option<(usize, usize)>,
    pub status: Option<String>,
    /// Rendered line opened by `--line`, highlighted until the reader moves.
    pub initial_line: Option<usize>,
    pub outline: Outline,
    /// First help entry on screen. A terminal too short for the whole list must
    /// still be able to reach the last line of it.
    pub help_top: usize,
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
            caret: 0,
            search: Search::default(),
            preview: Search::default(),
            search_origin: None,
            status: None,
            initial_line: None,
            outline: Outline::default(),
            help_top: 0,
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
        if !matches!(
            action,
            Action::Quit | Action::StartSearch | Action::ToggleOutline | Action::ToggleHelp
        ) {
            self.initial_line = None;
        }
        self.status = None;
        match action {
            Action::Quit => return false,
            Action::ToggleOutline => self.toggle_outline(),
            Action::ToggleHelp => self.toggle_help(),
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
                self.caret = 0;
                self.preview = Search::default();
                self.search_origin = Some((self.top, self.left));
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
        let before = self.search.position();
        let found = if forward {
            self.search.next()
        } else {
            self.search.previous()
        };
        match found {
            Some(m) => {
                self.reveal(m.line);
                // Cycling past either end is easy to mistake for "no more
                // matches", so say that it happened.
                if let (Some(before), Some(after)) = (before, self.search.position())
                    && self.search.count() > 1
                    && (forward && after <= before || !forward && after >= before)
                {
                    self.status = Some("wrapped".to_string());
                }
            }
            None => self.status = Some(format!("no match: {}", self.search.query())),
        }
    }

    /// Matches to highlight: the query being typed while the prompt is open,
    /// the confirmed one otherwise.
    pub fn visible_search(&self) -> &Search {
        match self.mode {
            Mode::Search => &self.preview,
            _ => &self.search,
        }
    }

    #[cfg(test)]
    pub fn preview_search(&self) -> &Search {
        &self.preview
    }

    pub fn insert_search_char(&mut self, c: char, lines: &[String]) {
        self.input.insert(self.caret, c);
        self.caret += c.len_utf8();
        self.update_preview(lines);
    }

    /// Apply one prompt edit. Caret movement never re-runs the query; only a
    /// change to the text does.
    pub fn edit_search(&mut self, edit: SearchEdit, lines: &[String]) {
        match edit {
            SearchEdit::CaretLeft => self.caret = self.previous_boundary(),
            SearchEdit::CaretRight => self.caret = self.next_boundary(),
            SearchEdit::CaretStart => self.caret = 0,
            SearchEdit::CaretEnd => self.caret = self.input.len(),
            SearchEdit::Backspace => {
                let from = self.previous_boundary();
                self.input.replace_range(from..self.caret, "");
                self.caret = from;
                self.update_preview(lines);
            }
            SearchEdit::Delete => {
                let to = self.next_boundary();
                self.input.replace_range(self.caret..to, "");
                self.update_preview(lines);
            }
            SearchEdit::DeleteWordBefore => {
                let from = self.word_start();
                self.input.replace_range(from..self.caret, "");
                self.caret = from;
                self.update_preview(lines);
            }
            SearchEdit::KillToStart => {
                self.input.replace_range(..self.caret, "");
                self.caret = 0;
                self.update_preview(lines);
            }
            SearchEdit::KillToEnd => {
                self.input.truncate(self.caret);
                self.update_preview(lines);
            }
        }
    }

    /// Byte offset one grapheme before the caret, so a wide character or a
    /// combining sequence is never cut in half.
    fn previous_boundary(&self) -> usize {
        self.input[..self.caret]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(at, _)| at)
    }

    fn next_boundary(&self) -> usize {
        self.input[self.caret..]
            .graphemes(true)
            .next()
            .map_or(self.caret, |g| self.caret + g.len())
    }

    /// Start of the word before the caret: the trailing spaces, then the run
    /// that precedes them.
    fn word_start(&self) -> usize {
        let head = &self.input[..self.caret];
        let trimmed = head.trim_end();
        match trimmed.rfind(char::is_whitespace) {
            Some(at) => at + head[at..].chars().next().map_or(1, char::len_utf8),
            None => 0,
        }
    }

    /// Re-run the pending query and show where it lands, without waiting for
    /// Enter. The search always restarts from where the prompt opened, so
    /// adding a character cannot walk the viewport down the document.
    fn update_preview(&mut self, lines: &[String]) {
        let (top, left) = self.search_origin.unwrap_or((self.top, self.left));
        self.top = top;
        self.left = left;
        self.status = None;
        self.preview.replace_query(&self.input, lines);
        if !self.preview.is_active() {
            return;
        }
        self.preview.select_from(top);
        if let Some(m) = self.preview.current() {
            self.initial_line = None;
            self.reveal(m.line);
        }
    }

    /// Confirm the search prompt. Keeps the viewport still when nothing matches.
    pub fn confirm_search(&mut self, lines: &[String]) {
        let query = std::mem::take(&mut self.input);
        self.mode = Mode::Normal;
        self.preview = Search::default();
        self.search_origin = None;
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

    /// Abandon the prompt: the confirmed search and the reading position are
    /// both as they were before `/` was pressed.
    pub fn cancel_search(&mut self) {
        self.mode = Mode::Normal;
        self.input.clear();
        self.caret = 0;
        self.preview = Search::default();
        if let Some((top, left)) = self.search_origin.take() {
            self.top = top;
            self.left = left;
        }
    }

    fn toggle_outline(&mut self) {
        if self.mode == Mode::Outline {
            self.mode = Mode::Normal;
            return;
        }
        if self.outline.is_empty() {
            self.status = Some("no headings".to_string());
            return;
        }
        self.outline.select_for_line(self.top);
        self.mode = Mode::Outline;
    }

    fn toggle_help(&mut self) {
        self.mode = match self.mode {
            Mode::Help => Mode::Normal,
            _ => {
                self.help_top = 0;
                Mode::Help
            }
        };
    }

    /// Scroll the key list. `visible` is how many entries fit in the overlay.
    pub fn scroll_help(&mut self, delta: isize, total: usize, visible: usize) {
        let last = total.saturating_sub(visible);
        self.help_top = self.help_top.saturating_add_signed(delta).min(last);
    }

    pub fn close_help(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn move_outline(&mut self, delta: isize) {
        self.outline.move_by(delta);
    }

    /// Jump to the selected heading and close the overlay.
    pub fn confirm_outline(&mut self) {
        self.mode = Mode::Normal;
        let Some(item) = self.outline.items.get(self.outline.selected) else {
            return;
        };
        let line = item.line;
        self.initial_line = None;
        // A heading is easier to read from the top of the screen than centred.
        self.top = line.min(self.max_top());
    }

    pub fn cancel_outline(&mut self) {
        self.mode = Mode::Normal;
    }

    /// Rebuild the outline after a re-layout, keeping the current selection.
    pub fn set_outline(&mut self, items: Vec<OutlineItem>) {
        let selected = self.outline.selected;
        self.outline = Outline {
            selected: selected.min(items.len().saturating_sub(1)),
            items,
        };
        if self.outline.is_empty() {
            self.mode = match self.mode {
                Mode::Outline => Mode::Normal,
                other => other,
            };
        }
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
            image: None,
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

    fn haystack() -> Vec<String> {
        (0..100)
            .map(|i| match i {
                20 | 42 | 80 => "needle".to_string(),
                _ => "x".to_string(),
            })
            .collect()
    }

    /// Typing moves the viewport as the query grows, without waiting for Enter.
    #[test]
    fn typing_previews_the_first_match_from_where_the_search_started() {
        let lines = haystack();
        let mut s = state();
        s.top = 30;
        s.apply(Action::StartSearch);
        for c in "needle".chars() {
            s.insert_search_char(c, &lines);
        }
        assert_eq!(s.mode, Mode::Search);
        assert!(s.top <= 42 && 42 < s.top + s.height, "top={}", s.top);
        assert_eq!(s.preview_search().count(), 3);

        // Deleting back to nothing puts the reader where they started.
        for _ in 0.."needle".len() {
            s.edit_search(SearchEdit::Backspace, &lines);
        }
        assert_eq!(s.top, 30);
        assert_eq!(s.preview_search().count(), 0);
    }

    #[test]
    fn a_query_with_no_match_leaves_the_viewport_alone() {
        let lines = haystack();
        let mut s = state();
        s.top = 30;
        s.apply(Action::StartSearch);
        for c in "absent".chars() {
            s.insert_search_char(c, &lines);
        }
        assert_eq!(s.top, 30);
        assert_eq!(s.preview_search().count(), 0);
    }

    #[test]
    fn cancelling_a_search_returns_to_where_it_started() {
        let lines = haystack();
        let mut s = state();
        s.top = 30;
        s.left = 8;
        s.apply(Action::StartSearch);
        for c in "needle".chars() {
            s.insert_search_char(c, &lines);
        }
        s.cancel_search();
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!((s.top, s.left), (30, 8));
        assert!(!s.search.is_active());
    }

    #[test]
    fn confirming_keeps_the_previewed_match() {
        let lines = haystack();
        let mut s = state();
        s.top = 30;
        s.apply(Action::StartSearch);
        for c in "needle".chars() {
            s.insert_search_char(c, &lines);
        }
        let previewed = s.top;
        s.confirm_search(&lines);
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(s.top, previewed);
        assert_eq!(s.search.current().unwrap().line, 42);
        assert_eq!(s.preview_search().count(), 0);
    }

    fn typed(text: &str, lines: &[String]) -> PagerState {
        let mut s = state();
        s.apply(Action::StartSearch);
        for c in text.chars() {
            s.insert_search_char(c, lines);
        }
        s
    }

    #[test]
    fn the_caret_edits_the_prompt_the_way_a_shell_does() {
        let lines = haystack();
        let mut s = typed("needle here", &lines);

        s.edit_search(SearchEdit::DeleteWordBefore, &lines);
        assert_eq!(s.input, "needle ");
        s.edit_search(SearchEdit::CaretStart, &lines);
        assert_eq!(s.caret, 0);
        s.insert_search_char('a', &lines);
        assert_eq!((s.input.as_str(), s.caret), ("aneedle ", 1));
        s.edit_search(SearchEdit::Delete, &lines);
        assert_eq!(s.input, "aeedle ");
        s.edit_search(SearchEdit::CaretEnd, &lines);
        s.edit_search(SearchEdit::KillToStart, &lines);
        assert_eq!((s.input.as_str(), s.caret), ("", 0));
    }

    #[test]
    fn caret_movement_respects_grapheme_boundaries() {
        let lines = haystack();
        let mut s = typed("日本語", &lines);
        assert_eq!(s.caret, 9);
        s.edit_search(SearchEdit::CaretLeft, &lines);
        assert_eq!(s.caret, 6);
        s.edit_search(SearchEdit::Backspace, &lines);
        assert_eq!((s.input.as_str(), s.caret), ("日語", 3));
        s.edit_search(SearchEdit::CaretRight, &lines);
        s.edit_search(SearchEdit::CaretRight, &lines);
        assert_eq!(s.caret, s.input.len());
        // Moving past either end stays on a boundary.
        s.edit_search(SearchEdit::CaretRight, &lines);
        assert_eq!(s.caret, s.input.len());
        s.edit_search(SearchEdit::CaretStart, &lines);
        s.edit_search(SearchEdit::CaretLeft, &lines);
        assert_eq!(s.caret, 0);
    }

    #[test]
    fn killing_to_the_end_keeps_what_is_before_the_caret() {
        let lines = haystack();
        let mut s = typed("needlex", &lines);
        s.edit_search(SearchEdit::CaretLeft, &lines);
        s.edit_search(SearchEdit::KillToEnd, &lines);
        assert_eq!(s.input, "needle");
        // The preview follows every edit, not just insertions.
        assert_eq!(s.preview_search().count(), 3);
    }

    #[test]
    fn cycling_past_the_last_match_says_it_wrapped() {
        let lines = haystack();
        let mut s = typed("needle", &lines);
        s.confirm_search(&lines);
        s.apply(Action::NextMatch);
        assert_eq!(s.status, None);
        s.apply(Action::NextMatch);
        assert_eq!(s.status, None);
        // Fourth match of three: back to the first.
        s.apply(Action::NextMatch);
        assert_eq!(s.status.as_deref(), Some("wrapped"));
        s.apply(Action::PreviousMatch);
        assert_eq!(s.status.as_deref(), Some("wrapped"));
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

    fn with_outline() -> PagerState {
        let mut s = state();
        s.set_outline(vec![
            OutlineItem {
                level: 1,
                text: "Title".into(),
                line: 0,
            },
            OutlineItem {
                level: 2,
                text: "First".into(),
                line: 20,
            },
            OutlineItem {
                level: 2,
                text: "Second".into(),
                line: 60,
            },
        ]);
        s
    }

    #[test]
    fn the_outline_opens_on_the_section_being_read() {
        let mut s = with_outline();
        s.top = 25;
        s.apply(Action::ToggleOutline);
        assert_eq!(s.mode, Mode::Outline);
        assert_eq!(s.outline.selected, 1);
    }

    #[test]
    fn toggling_twice_returns_to_the_document() {
        let mut s = with_outline();
        s.apply(Action::ToggleOutline);
        s.apply(Action::ToggleOutline);
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn a_document_without_headings_reports_instead_of_opening() {
        let mut s = state();
        s.apply(Action::ToggleOutline);
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(s.status.as_deref(), Some("no headings"));
    }

    #[test]
    fn outline_selection_clamps_at_both_ends() {
        let mut s = with_outline();
        s.apply(Action::ToggleOutline);
        s.move_outline(-5);
        assert_eq!(s.outline.selected, 0);
        s.move_outline(9);
        assert_eq!(s.outline.selected, 2);
    }

    #[test]
    fn confirming_moves_the_viewport_to_the_heading() {
        let mut s = with_outline();
        s.apply(Action::ToggleOutline);
        s.move_outline(1);
        s.confirm_outline();
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(s.top, 20);
    }

    #[test]
    fn cancelling_leaves_the_viewport_alone() {
        let mut s = with_outline();
        s.top = 40;
        s.apply(Action::ToggleOutline);
        s.move_outline(-2);
        s.cancel_outline();
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(s.top, 40);
    }

    #[test]
    fn a_relayout_keeps_the_selection_within_bounds() {
        let mut s = with_outline();
        s.apply(Action::ToggleOutline);
        s.move_outline(2);
        s.set_outline(vec![OutlineItem {
            level: 1,
            text: "Title".into(),
            line: 3,
        }]);
        assert_eq!(s.outline.selected, 0);
        // Losing every heading must not leave the overlay open.
        s.set_outline(Vec::new());
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn the_help_overlay_toggles_and_scrolls_within_its_list() {
        let mut s = state();
        s.apply(Action::ToggleHelp);
        assert_eq!(s.mode, Mode::Help);
        s.scroll_help(-1, 15, 8);
        assert_eq!(s.help_top, 0);
        s.scroll_help(20, 15, 8);
        assert_eq!(s.help_top, 7);
        s.apply(Action::ToggleHelp);
        assert_eq!(s.mode, Mode::Normal);
        // Reopening starts from the top of the list.
        s.apply(Action::ToggleHelp);
        assert_eq!(s.help_top, 0);
    }

    #[test]
    fn opening_the_help_does_not_move_the_document() {
        let mut s = state();
        s.top = 30;
        s.initial_line = Some(30);
        s.apply(Action::ToggleHelp);
        assert_eq!(s.top, 30);
        assert_eq!(s.initial_line, Some(30));
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
