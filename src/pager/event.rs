//! Key map. Only the bindings documented in the README exist; no extra
//! defaults are added.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::pager::state::{Action, Mode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Navigate(Action),
    SearchChar(char),
    SearchBackspace,
    SearchConfirm,
    SearchCancel,
    Ignored,
}

pub fn map(key: KeyEvent, mode: Mode) -> Input {
    match mode {
        Mode::Search => search_mode(key),
        Mode::Normal => normal_mode(key),
    }
}

fn search_mode(key: KeyEvent) -> Input {
    match key.code {
        KeyCode::Enter => Input::SearchConfirm,
        KeyCode::Esc => Input::SearchCancel,
        KeyCode::Backspace => Input::SearchBackspace,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Input::SearchCancel,
        KeyCode::Char(_) if key.modifiers.contains(KeyModifiers::CONTROL) => Input::Ignored,
        KeyCode::Char(c) => Input::SearchChar(c),
        _ => Input::Ignored,
    }
}

fn normal_mode(key: KeyEvent) -> Input {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('d') => Input::Navigate(Action::HalfPageDown),
            KeyCode::Char('u') => Input::Navigate(Action::HalfPageUp),
            KeyCode::Char('c') => Input::Navigate(Action::Quit),
            _ => Input::Ignored,
        };
    }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Input::Navigate(Action::LineDown),
        KeyCode::Char('k') | KeyCode::Up => Input::Navigate(Action::LineUp),
        KeyCode::Char(' ') | KeyCode::PageDown => Input::Navigate(Action::PageDown),
        KeyCode::Char('b') | KeyCode::PageUp => Input::Navigate(Action::PageUp),
        KeyCode::Char('g') | KeyCode::Home => Input::Navigate(Action::Top),
        KeyCode::Char('G') | KeyCode::End => Input::Navigate(Action::Bottom),
        KeyCode::Char('h') | KeyCode::Left => Input::Navigate(Action::ScrollLeft),
        KeyCode::Char('l') | KeyCode::Right => Input::Navigate(Action::ScrollRight),
        KeyCode::Char('0') => Input::Navigate(Action::ResetHorizontal),
        KeyCode::Char('/') => Input::Navigate(Action::StartSearch),
        KeyCode::Char('n') => Input::Navigate(Action::NextMatch),
        KeyCode::Char('N') => Input::Navigate(Action::PreviousMatch),
        KeyCode::Char('q') | KeyCode::Esc => Input::Navigate(Action::Quit),
        _ => Input::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn normal(code: KeyCode) -> Input {
        map(key(code), Mode::Normal)
    }

    #[test]
    fn movement_keys_have_arrow_aliases() {
        assert_eq!(normal(KeyCode::Char('j')), normal(KeyCode::Down));
        assert_eq!(normal(KeyCode::Char('k')), normal(KeyCode::Up));
        assert_eq!(normal(KeyCode::Char('h')), normal(KeyCode::Left));
        assert_eq!(normal(KeyCode::Char('l')), normal(KeyCode::Right));
        assert_eq!(normal(KeyCode::Char(' ')), normal(KeyCode::PageDown));
        assert_eq!(normal(KeyCode::Char('b')), normal(KeyCode::PageUp));
        assert_eq!(normal(KeyCode::Char('g')), normal(KeyCode::Home));
        assert_eq!(normal(KeyCode::Char('G')), normal(KeyCode::End));
    }

    #[test]
    fn half_page_keys_require_control() {
        assert_eq!(
            map(ctrl('d'), Mode::Normal),
            Input::Navigate(Action::HalfPageDown)
        );
        assert_eq!(
            map(ctrl('u'), Mode::Normal),
            Input::Navigate(Action::HalfPageUp)
        );
        assert_eq!(normal(KeyCode::Char('d')), Input::Ignored);
        assert_eq!(normal(KeyCode::Char('u')), Input::Ignored);
    }

    #[test]
    fn q_and_esc_quit_in_normal_mode() {
        assert_eq!(normal(KeyCode::Char('q')), Input::Navigate(Action::Quit));
        assert_eq!(normal(KeyCode::Esc), Input::Navigate(Action::Quit));
    }

    #[test]
    fn search_keys_enter_and_drive_the_prompt() {
        assert_eq!(
            normal(KeyCode::Char('/')),
            Input::Navigate(Action::StartSearch)
        );
        assert_eq!(
            normal(KeyCode::Char('n')),
            Input::Navigate(Action::NextMatch)
        );
        assert_eq!(
            normal(KeyCode::Char('N')),
            Input::Navigate(Action::PreviousMatch)
        );
        assert_eq!(
            map(key(KeyCode::Char('x')), Mode::Search),
            Input::SearchChar('x')
        );
        assert_eq!(
            map(key(KeyCode::Backspace), Mode::Search),
            Input::SearchBackspace
        );
        assert_eq!(map(key(KeyCode::Enter), Mode::Search), Input::SearchConfirm);
    }

    #[test]
    fn esc_cancels_the_search_instead_of_quitting() {
        assert_eq!(map(key(KeyCode::Esc), Mode::Search), Input::SearchCancel);
    }

    #[test]
    fn navigation_letters_are_literal_text_while_searching() {
        for c in ['j', 'q', 'G', '/', '0'] {
            assert_eq!(
                map(key(KeyCode::Char(c)), Mode::Search),
                Input::SearchChar(c)
            );
        }
    }

    #[test]
    fn undocumented_keys_do_nothing() {
        for code in [
            KeyCode::Tab,
            KeyCode::Insert,
            KeyCode::F(5),
            KeyCode::Char('z'),
        ] {
            assert_eq!(normal(code), Input::Ignored);
        }
    }
}
