pub mod app;
pub mod event;
pub mod help;
pub mod images;
pub mod search;
mod search_session;
pub mod state;
pub mod view;
pub mod watch;

pub use app::{PagerInput, Watched, run};

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
    restore: fn(),
}

impl TerminalGuard {
    pub fn enter() -> std::io::Result<Self> {
        install_panic_hook();
        enable_raw_mode()?;
        Self::finish_enter(
            || {
                let mut out = stdout();
                execute!(out, EnterAlternateScreen, Hide)?;
                out.flush()
            },
            restore,
        )
    }

    fn finish_enter(
        initialize: impl FnOnce() -> std::io::Result<()>,
        restore: fn(),
    ) -> std::io::Result<Self> {
        let guard = Self {
            active: true,
            restore,
        };
        initialize()?;
        Ok(guard)
    }

    pub fn leave(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        (self.restore)();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn restoration_runs_once_on_partial_failure_normal_exit_and_panic() {
        static RESTORED: AtomicUsize = AtomicUsize::new(0);
        fn restore_test() {
            RESTORED.fetch_add(1, Ordering::SeqCst);
        }
        let result = TerminalGuard::finish_enter(
            || Err(std::io::Error::other("screen failed")),
            restore_test,
        );
        assert!(result.is_err());
        assert_eq!(RESTORED.load(Ordering::SeqCst), 1);
        {
            let mut guard = TerminalGuard::finish_enter(|| Ok(()), restore_test).unwrap();
            guard.leave();
            guard.leave();
        }
        assert_eq!(RESTORED.load(Ordering::SeqCst), 2);
        let _ = std::panic::catch_unwind(|| {
            let _guard = TerminalGuard::finish_enter(|| Ok(()), restore_test).unwrap();
            panic!("draw failed");
        });
        assert_eq!(RESTORED.load(Ordering::SeqCst), 3);
    }
}
