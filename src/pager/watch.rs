//! Following a file for changes.
//!
//! `notify` is confined to this module. Events arrive on a channel and are
//! drained from the existing event loop, so the pager stays synchronous and no
//! async runtime is involved.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

/// How long the file must sit still before it is read again. An editor writing
/// through a temporary file produces a burst of events, and reading in the
/// middle of one shows a half-written document.
const QUIET: Duration = Duration::from_millis(100);

/// Collapses a burst of events into a single reload.
#[derive(Debug, Default)]
pub struct Debounce {
    pending: Option<Instant>,
}

impl Debounce {
    /// Record that something happened, restarting the quiet period.
    pub fn touch(&mut self, at: Instant) {
        self.pending = Some(at);
    }

    /// Whether the quiet period has elapsed. Returns `true` once per burst.
    pub fn ready(&mut self, now: Instant, quiet: Duration) -> bool {
        match self.pending {
            Some(at) if now.duration_since(at) >= quiet => {
                self.pending = None;
                true
            }
            _ => false,
        }
    }
}

pub struct Watch {
    /// Dropping the watcher stops the background thread, so it is kept alive
    /// for as long as the pager runs.
    _watcher: RecommendedWatcher,
    events: Receiver<()>,
    debounce: Debounce,
}

impl Watch {
    /// Follow `path`.
    ///
    /// The parent directory is watched rather than the file itself: editors and
    /// coding agents save by writing a temporary file and renaming it over the
    /// target, which replaces the inode and would silently detach a watch on
    /// the file. Events for other names in the directory are ignored.
    pub fn new(path: &Path) -> Result<Self, notify::Error> {
        let directory = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let name = path.file_name().map(std::ffi::OsString::from);

        let (sender, events) = channel();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                let Ok(event) = result else {
                    return;
                };
                // Reading the file is not a change. Every other kind is taken at
                // face value: backends differ in how precisely they classify, and
                // a spurious reload is cheaper than a missed one.
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                let hit = event
                    .paths
                    .iter()
                    .any(|p| p.file_name().map(std::ffi::OsString::from) == name);
                if hit {
                    // The receiver is gone once the pager exits; nothing to do.
                    let _ = sender.send(());
                }
            })?;
        watcher.watch(&directory, RecursiveMode::NonRecursive)?;

        Ok(Self {
            _watcher: watcher,
            events,
            debounce: Debounce::default(),
        })
    }

    /// Whether the file should be read again now. Call once per loop iteration.
    pub fn should_reload(&mut self, now: Instant) -> bool {
        while self.events.try_recv().is_ok() {
            self.debounce.touch(now);
        }
        self.debounce.ready(now, QUIET)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_of_events_causes_one_reload() {
        let start = Instant::now();
        let mut debounce = Debounce::default();
        debounce.touch(start);
        debounce.touch(start + Duration::from_millis(20));
        debounce.touch(start + Duration::from_millis(40));

        // Still settling.
        assert!(!debounce.ready(start + Duration::from_millis(100), QUIET));
        assert!(debounce.ready(start + Duration::from_millis(141), QUIET));
        // And only once.
        assert!(!debounce.ready(start + Duration::from_millis(500), QUIET));
    }

    #[test]
    fn nothing_happens_without_an_event() {
        let mut debounce = Debounce::default();
        assert!(!debounce.ready(Instant::now(), QUIET));
    }

    /// The platform watcher starts asynchronously, so a write issued in the
    /// same instant as `Watch::new` can legitimately be missed. Real use never
    /// hits this: the reader opens the file long before saving it.
    const SETTLE: Duration = Duration::from_millis(1200);

    /// Poll like the pager does, up to `limit`.
    fn wait(watch: &mut Watch, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            if watch.should_reload(Instant::now()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn a_write_to_the_file_is_noticed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "# before\n").unwrap();

        let mut watch = Watch::new(&path).unwrap();
        std::thread::sleep(SETTLE);
        std::fs::write(&path, "# after\n").unwrap();
        assert!(wait(&mut watch, Duration::from_secs(5)));
    }

    #[test]
    fn an_atomic_save_is_noticed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "# before\n").unwrap();

        let mut watch = Watch::new(&path).unwrap();
        std::thread::sleep(SETTLE);
        // How editors save: write a sibling, then rename it over the target.
        let temp = dir.path().join("doc.md.tmp");
        std::fs::write(&temp, "# after\n").unwrap();
        std::fs::rename(&temp, &path).unwrap();
        assert!(wait(&mut watch, Duration::from_secs(5)));
    }

    #[test]
    fn another_file_in_the_directory_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "# doc\n").unwrap();

        let mut watch = Watch::new(&path).unwrap();
        std::thread::sleep(SETTLE);
        std::fs::write(dir.path().join("other.md"), "# other\n").unwrap();
        assert!(!wait(&mut watch, Duration::from_millis(600)));
    }

    #[test]
    fn watching_a_bare_file_name_uses_the_current_directory() {
        // A relative path with no parent must not try to watch "".
        let watch = Watch::new(Path::new("Cargo.toml"));
        assert!(watch.is_ok());
    }
}
