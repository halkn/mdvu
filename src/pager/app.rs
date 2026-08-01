//! Pager event loop.
//!
//! The document is parsed once before the loop starts. Layout re-runs only when
//! the terminal size changes, never per frame.

use std::time::Duration;

use crossterm::event::{self, Event};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::error::{AppError, Result};
use crate::layout::inline::InlineContext;
use crate::layout::theme::Theme;
use crate::layout::{LayoutOptions, RenderedDocument, layout_document};
use crate::markdown::model::Document;
use crate::pager::TerminalGuard;
use crate::pager::event::{Input, map};
use crate::pager::state::{PagerState, rendered_line_for_source, source_line_at};
use crate::pager::view::{ViewContext, draw, widest_line};

/// How long a frame waits for input before looping again.
const POLL: Duration = Duration::from_millis(250);

pub struct PagerInput<'a> {
    pub document: &'a Document,
    pub inline: InlineContext,
    pub theme: Theme,
    pub title: String,
    pub flavor: &'static str,
    /// Overrides the terminal width when the reader passed `--width`.
    pub width_override: Option<usize>,
    pub start_line: Option<usize>,
}

pub fn run(input: PagerInput<'_>) -> Result<()> {
    let mut guard = TerminalGuard::enter().map_err(|source| AppError::Terminal { source })?;
    let result = event_loop(input);
    guard.leave();
    result
}

fn event_loop(input: PagerInput<'_>) -> Result<()> {
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|source| AppError::Terminal { source })?;

    let area = terminal
        .size()
        .map_err(|source| AppError::Terminal { source })?;
    let mut width = content_width(&input, area.width);
    let mut rendered = layout(&input, width);
    let mut texts = line_texts(&rendered);

    let body_height = area.height.saturating_sub(1) as usize;
    // The viewport is always the real terminal width. Layout may be wider when
    // `--width` overrides it, and the difference is reached by scrolling.
    let mut state = PagerState::new(
        rendered.lines.len(),
        widest_line(&rendered.lines),
        body_height,
        area.width as usize,
    );

    if let Some(source_line) = input.start_line
        && let Some(index) = rendered_line_for_source(&rendered.lines, source_line)
    {
        state.initial_line = Some(index);
        state.top = index.saturating_sub(state.height / 2).min(state.max_top());
    }

    let ctx = ViewContext {
        title: &input.title,
        flavor: input.flavor,
        source_lines: input.document.source.line_count(),
        diagnostics: rendered.diagnostics.len(),
        theme: input.theme,
    };

    loop {
        terminal
            .draw(|frame| draw(frame, &rendered.lines, &state, &ctx))
            .map_err(|source| AppError::Terminal { source })?;

        if !event::poll(POLL).map_err(|source| AppError::Terminal { source })? {
            continue;
        }
        match event::read().map_err(|source| AppError::Terminal { source })? {
            Event::Key(key) if key.is_press() => {
                if !handle_key(&mut state, key, &texts) {
                    return Ok(());
                }
            }
            Event::Resize(columns, rows) => {
                // Keep the reader's place across a re-layout by remembering the
                // source line at the top of the viewport.
                let anchor = source_line_at(&rendered.lines, state.top);
                width = content_width(&input, columns);
                rendered = layout(&input, width);
                texts = line_texts(&rendered);
                state.resize(
                    rendered.lines.len(),
                    widest_line(&rendered.lines),
                    rows.saturating_sub(1) as usize,
                    columns as usize,
                );
                state.search.recompute(&texts);
                state.initial_line = None;
                if let Some(line) = anchor
                    && let Some(index) = rendered_line_for_source(&rendered.lines, line)
                {
                    state.top = index.min(state.max_top());
                }
            }
            _ => {}
        }
    }
}

/// Returns `false` when the pager should exit.
fn handle_key(state: &mut PagerState, key: event::KeyEvent, texts: &[String]) -> bool {
    match map(key, state.mode) {
        Input::Navigate(action) => return state.apply(action),
        Input::SearchChar(c) => state.input.push(c),
        Input::SearchBackspace => {
            state.input.pop();
        }
        Input::SearchConfirm => state.confirm_search(texts),
        Input::SearchCancel => state.cancel_search(),
        Input::Ignored => {}
    }
    true
}

fn content_width(input: &PagerInput<'_>, columns: u16) -> usize {
    input
        .width_override
        .unwrap_or_else(|| (columns as usize).max(1))
}

fn layout(input: &PagerInput<'_>, width: usize) -> RenderedDocument {
    layout_document(input.document, LayoutOptions::new(width), &input.inline)
}

fn line_texts(rendered: &RenderedDocument) -> Vec<String> {
    rendered.lines.iter().map(|l| l.text()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pager::state::{Action, Mode};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn texts() -> Vec<String> {
        (0..50)
            .map(|i| {
                if i == 30 {
                    "target".into()
                } else {
                    "filler".into()
                }
            })
            .collect()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_a_search_and_confirming_moves_the_viewport() {
        let mut state = PagerState::new(50, 10, 10, 40);
        assert!(handle_key(&mut state, key(KeyCode::Char('/')), &texts()));
        assert_eq!(state.mode, Mode::Search);
        for c in "target".chars() {
            handle_key(&mut state, key(KeyCode::Char(c)), &texts());
        }
        assert_eq!(state.input, "target");
        handle_key(&mut state, key(KeyCode::Enter), &texts());
        assert_eq!(state.mode, Mode::Normal);
        assert!(state.top <= 30 && 30 < state.top + state.height);
    }

    #[test]
    fn backspace_edits_the_pending_query() {
        let mut state = PagerState::new(50, 10, 10, 40);
        state.apply(Action::StartSearch);
        for c in "targetx".chars() {
            handle_key(&mut state, key(KeyCode::Char(c)), &texts());
        }
        handle_key(&mut state, key(KeyCode::Backspace), &texts());
        assert_eq!(state.input, "target");
    }

    #[test]
    fn q_ends_the_loop() {
        let mut state = PagerState::new(50, 10, 10, 40);
        assert!(!handle_key(&mut state, key(KeyCode::Char('q')), &texts()));
    }

    #[test]
    fn esc_cancels_a_search_without_ending_the_loop() {
        let mut state = PagerState::new(50, 10, 10, 40);
        state.apply(Action::StartSearch);
        assert!(handle_key(&mut state, key(KeyCode::Esc), &texts()));
        assert_eq!(state.mode, Mode::Normal);
        // Esc again, now in normal mode, quits.
        assert!(!handle_key(&mut state, key(KeyCode::Esc), &texts()));
    }
}
