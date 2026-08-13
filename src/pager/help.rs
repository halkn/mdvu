//! The key list shown by the `?` overlay.
//!
//! This table is the source of truth for the bindings: a reader in the pager
//! cannot open the README, so the overlay must stand on its own and the README
//! table is kept in step with this one.

/// One row of the overlay: the keys that trigger an action, and what it does.
pub struct Entry {
    /// Individual key names, joined for display and parsed by the test that
    /// checks every advertised key is actually bound.
    pub keys: &'static [&'static str],
    pub action: &'static str,
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        keys: &["j", "Down"],
        action: "Down one line",
    },
    Entry {
        keys: &["k", "Up"],
        action: "Up one line",
    },
    Entry {
        keys: &["Ctrl-d", "Ctrl-u"],
        action: "Half screen down / up",
    },
    Entry {
        keys: &["Space", "PageDown"],
        action: "One screen down",
    },
    Entry {
        keys: &["b", "PageUp"],
        action: "One screen up",
    },
    Entry {
        keys: &["g", "Home"],
        action: "Top of the document",
    },
    Entry {
        keys: &["G", "End"],
        action: "End of the document",
    },
    Entry {
        keys: &["h", "Left"],
        action: "Scroll left",
    },
    Entry {
        keys: &["l", "Right"],
        action: "Scroll right",
    },
    Entry {
        keys: &["0"],
        action: "Reset horizontal scroll",
    },
    Entry {
        keys: &["/"],
        action: "Search, Enter to confirm, Esc to cancel",
    },
    Entry {
        keys: &["n", "N"],
        action: "Next / previous match",
    },
    Entry {
        keys: &["t"],
        action: "Heading list",
    },
    Entry {
        keys: &["?"],
        action: "This help",
    },
    Entry {
        keys: &["q", "Esc"],
        action: "Quit",
    },
];

/// Display width of the key column, so the actions line up.
pub fn key_column() -> usize {
    ENTRIES
        .iter()
        .map(|entry| entry.keys.join(", ").chars().count())
        .max()
        .unwrap_or(0)
}

/// One row as `keys` padded to the key column, then the action.
pub fn row(entry: &Entry, column: usize) -> String {
    format!("{:<column$}  {}", entry.keys.join(", "), entry.action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pager::event::{Input, map};
    use crate::pager::state::Mode;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// The overlay must not advertise a key the pager ignores.
    fn parse(name: &str) -> KeyEvent {
        if let Some(letter) = name.strip_prefix("Ctrl-") {
            let c = letter.chars().next().expect("a letter after Ctrl-");
            return KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        }
        let code = match name {
            "Space" => KeyCode::Char(' '),
            "Down" => KeyCode::Down,
            "Up" => KeyCode::Up,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            "PageDown" => KeyCode::PageDown,
            "PageUp" => KeyCode::PageUp,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            "Esc" => KeyCode::Esc,
            other => {
                let mut chars = other.chars();
                let c = chars.next().expect("a key name");
                assert!(chars.next().is_none(), "unknown key name: {other}");
                KeyCode::Char(c)
            }
        };
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn every_advertised_key_is_bound() {
        for entry in ENTRIES {
            for name in entry.keys {
                assert_ne!(
                    map(parse(name), Mode::Normal),
                    Input::Ignored,
                    "{name} is listed in the help but does nothing"
                );
            }
        }
    }

    #[test]
    fn rows_line_up_under_one_key_column() {
        let column = key_column();
        for entry in ENTRIES {
            let row = row(entry, column);
            assert_eq!(&row[column + 2..], entry.action, "{row}");
        }
    }
}
