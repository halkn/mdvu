//! Pager event loop.
//!
//! The document is parsed once before the loop starts. Layout re-runs only when
//! the terminal size changes or the file is reloaded, never per frame.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::diagram::MermaidMode;
use crate::error::{AppError, Result};
use crate::flavor::Flavor;
use crate::input;
use crate::layout::inline::InlineContext;
use crate::layout::theme::Theme;
use crate::layout::{LayoutOptions, RenderedDocument, layout_document};
use crate::markdown::model::{Document, headings};
use crate::pager::TerminalGuard;
use crate::pager::event::{Input, map};
use crate::pager::help;
use crate::pager::images::{self, Placed};
use crate::pager::state::{OutlineItem, PagerState, rendered_line_for_source, source_line_at};
use crate::pager::view::{ViewContext, draw, help_rows, widest_line};
use crate::pager::watch::Watch;

/// How long a frame waits for input before looping again.
const POLL: Duration = Duration::from_millis(250);

/// The file to follow, and how to parse it again.
pub struct Watched {
    pub path: PathBuf,
    pub flavor: Flavor,
    pub mermaid: MermaidMode,
}

impl Watched {
    fn reload(&self) -> Result<Document> {
        let loaded = input::load(&input::InputSource::File(self.path.clone()))?;
        Ok(crate::document::build(
            loaded.text,
            self.flavor,
            self.mermaid,
        ))
    }
}

pub struct PagerInput {
    pub document: Document,
    pub inline: InlineContext,
    pub theme: Theme,
    pub title: String,
    pub flavor: String,
    /// Overrides the terminal width when the reader passed `--width`.
    pub width_override: Option<usize>,
    pub start_line: Option<usize>,
    /// Set by `--watch`.
    pub watched: Option<Watched>,
}

pub fn run(input: PagerInput) -> Result<()> {
    let mut guard = TerminalGuard::enter().map_err(|source| AppError::Terminal { source })?;
    let result = event_loop(input);
    guard.leave();
    result
}

fn event_loop(input: PagerInput) -> Result<()> {
    let PagerInput {
        mut document,
        inline,
        theme,
        title,
        flavor,
        width_override,
        start_line,
        watched,
    } = input;

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|source| AppError::Terminal { source })?;

    let mut watch = match &watched {
        Some(watched) => Some(Watch::new(&watched.path).map_err(|error| AppError::Watch {
            path: watched.path.clone(),
            message: error.to_string(),
        })?),
        None => None,
    };

    let area = terminal
        .size()
        .map_err(|source| AppError::Terminal { source })?;
    let content_width = |columns: u16| width_override.unwrap_or_else(|| (columns as usize).max(1));
    let mut width = content_width(area.width);
    let mut rendered = layout_document(&document, LayoutOptions::new(width), &inline);
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

    state.set_outline(outline(&document, &rendered));
    // What the terminal is currently showing, so an unchanged frame does not
    // re-send image payloads that can be megabytes each.
    let mut shown: Vec<Placed> = Vec::new();
    // Placements a kitty terminal already stores, so scrolling only re-places.
    let mut sent: HashSet<u32> = HashSet::new();
    let protocol = inline.images.map(|support| support.protocol);

    if let Some(source_line) = start_line
        && let Some(index) = rendered_line_for_source(&rendered.lines, source_line)
    {
        state.initial_line = Some(index);
        state.top = index.saturating_sub(state.height / 2).min(state.max_top());
    }

    loop {
        let wanted = images::visible(
            &rendered.lines,
            state.top,
            state.left,
            state.width,
            state.height,
        );
        let images_changed = wanted != shown;
        // Kitty removes its own placements. iTerm2 draws into the cells, so the
        // only way back is to make `ratatui` repaint the whole screen.
        if images_changed && images::needs_repaint(&shown) {
            terminal
                .clear()
                .map_err(|source| AppError::Terminal { source })?;
        }

        let ctx = ViewContext {
            title: &title,
            flavor: &flavor,
            source_lines: document.source.line_count(),
            diagnostics: rendered.diagnostics.len(),
            theme,
        };
        terminal
            .draw(|frame| draw(frame, &rendered.lines, &state, &ctx))
            .map_err(|source| AppError::Terminal { source })?;
        if images_changed {
            images::draw(&shown, &wanted, &mut sent)?;
            shown = wanted;
        }

        // Reloading happens between frames, never during one, so the frame loop
        // still does no parsing.
        if let (Some(watch), Some(watched)) = (watch.as_mut(), watched.as_ref())
            && watch.should_reload(Instant::now())
        {
            match watched.reload() {
                Ok(reloaded) => {
                    document = reloaded;
                    let anchor = source_line_at(&rendered.lines, state.top);
                    rendered = layout_document(&document, LayoutOptions::new(width), &inline);
                    images::release(protocol, &mut sent)?;
                    texts = line_texts(&rendered);
                    let viewport = (state.height, state.width);
                    restore(&mut state, &rendered, &texts, &document, anchor, viewport);
                    state.status = Some("reloaded".to_string());
                }
                // A file being rewritten can be briefly missing or invalid.
                // The previous rendering stays on screen and the next event
                // tries again.
                Err(error) => state.status = Some(format!("reload failed: {error}")),
            }
        }

        if !event::poll(POLL).map_err(|source| AppError::Terminal { source })? {
            continue;
        }
        match event::read().map_err(|source| AppError::Terminal { source })? {
            Event::Key(key) if key.is_press() => {
                if !handle_key(&mut state, key, &texts) {
                    // Leave no image on screen or stored in the terminal.
                    images::release(protocol, &mut sent)?;
                    return Ok(());
                }
            }
            Event::Resize(columns, rows) => {
                // Keep the reader's place across a re-layout by remembering the
                // source line at the top of the viewport.
                let anchor = source_line_at(&rendered.lines, state.top);
                width = content_width(columns);
                rendered = layout_document(&document, LayoutOptions::new(width), &inline);
                // Every placement is new after a re-layout.
                images::release(protocol, &mut sent)?;
                texts = line_texts(&rendered);
                let viewport = (rows.saturating_sub(1) as usize, columns as usize);
                restore(&mut state, &rendered, &texts, &document, anchor, viewport);
            }
            _ => {}
        }
    }
}

/// Put the reader back where they were after the surface was rebuilt, keeping
/// `anchor`'s source line in view. `viewport` is the body's height and width.
fn restore(
    state: &mut PagerState,
    rendered: &RenderedDocument,
    texts: &[String],
    document: &Document,
    anchor: Option<usize>,
    (height, width): (usize, usize),
) {
    state.resize(
        rendered.lines.len(),
        widest_line(&rendered.lines),
        height,
        width,
    );
    state.set_outline(outline(document, rendered));
    state.initial_line = None;
    if let Some(line) = anchor
        && let Some(index) = rendered_line_for_source(&rendered.lines, line)
    {
        state.top = index.min(state.max_top());
    }
    // After the viewport is back where it was, so the current match is chosen
    // from the reader's place in the new layout.
    state.recompute_searches(texts);
}

/// Returns `false` when the pager should exit.
fn handle_key(state: &mut PagerState, key: event::KeyEvent, texts: &[String]) -> bool {
    match map(key, state.mode) {
        Input::Navigate(action) => return state.apply(action),
        Input::SearchChar(c) => state.insert_search_char(c, texts),
        Input::SearchEdit(edit) => state.edit_search(edit, texts),
        Input::SearchConfirm => state.confirm_search(texts),
        Input::SearchCancel => state.cancel_search(),
        Input::OutlineMove(delta) => state.move_outline(delta),
        Input::OutlineConfirm => state.confirm_outline(),
        Input::OutlineCancel => state.cancel_outline(),
        Input::HelpScroll(delta) => {
            state.scroll_help(delta, help::ENTRIES.len(), help_rows(state.height))
        }
        Input::HelpClose => state.close_help(),
        Input::Ignored => {}
    }
    true
}

fn line_texts(rendered: &RenderedDocument) -> Vec<String> {
    rendered.lines.iter().map(|l| l.text()).collect()
}

/// Headings paired with the rendered line they start on. Rebuilt whenever the
/// surface is laid out again, since the line numbers change with the width.
fn outline(document: &Document, rendered: &RenderedDocument) -> Vec<OutlineItem> {
    headings(&document.blocks)
        .into_iter()
        .filter_map(|heading| {
            rendered_line_for_source(&rendered.lines, heading.range.line_start).map(|line| {
                OutlineItem {
                    level: heading.level,
                    text: heading.plain.clone(),
                    line,
                }
            })
        })
        .collect()
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
