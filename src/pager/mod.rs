pub mod app;
pub mod event;
pub mod search;
pub mod state;
pub mod view;

pub use app::{PagerInput, run};

use std::io::{Stdout, Write, stdout};

use crossterm::cursor::{Hide, Show};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Owns every terminal mode change the pager makes and undoes them on drop, so
/// a normal exit, an error and a panic all leave the terminal usable.
pub struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    pub fn enter() -> std::io::Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        // Mouse capture is never enabled; the pager does not handle mouse input.
        execute!(out, EnterAlternateScreen, Hide)?;
        out.flush()?;
        install_panic_hook();
        Ok(Self { active: true })
    }

    pub fn leave(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        restore();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.leave();
    }
}

fn restore() {
    let mut out: Stdout = stdout();
    let _ = execute!(out, LeaveAlternateScreen, Show);
    let _ = disable_raw_mode();
    let _ = out.flush();
}

/// Restore the terminal before the default panic handler prints, then let the
/// panic continue as usual.
fn install_panic_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
    });
}
