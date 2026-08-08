//! Inline images: which terminals get them, which files may be read, and how
//! much of the grid an image occupies.
//!
//! Every rule that can turn an image back into a text placeholder lives here,
//! so `layout` and the backends only ever see a resolved `Placement`.

pub mod dimensions;
pub mod protocol;

use std::path::{Path, PathBuf};
use std::rc::Rc;

use dimensions::{Format, Pixels};
pub use protocol::Protocol;

/// Largest file that will be sent to the terminal. A payload is base64 encoded
/// and travels over the tty, so an unbounded read would stall the terminal.
const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// Tallest image, in cells. An image is useless once it is taller than the
/// viewport, and the layout has no height to measure against.
const MAX_ROWS: usize = 20;

/// Used when the terminal reports no pixel size. Cells are about twice as tall
/// as they are wide, which is enough to keep an image from looking stretched.
const ASSUMED_CELL: CellSize = CellSize {
    width: 10,
    height: 20,
};

/// Formats both protocols decode themselves. SVG is absent on purpose: no
/// terminal renders it, and rasterising would mean shipping a renderer.
const ALLOWED: &[(&str, Format)] = &[
    ("png", Format::Png),
    ("jpg", Format::Jpeg),
    ("jpeg", Format::Jpeg),
    ("gif", Format::Gif),
    ("webp", Format::WebP),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    pub width: u16,
    pub height: u16,
}

/// What the running terminal can draw. Held by `InlineContext`, so a change
/// here re-runs layout the same way a width change does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSupport {
    pub protocol: Protocol,
    pub cell: CellSize,
}

impl ImageSupport {
    /// Cell size comes from `TIOCGWINSZ`, which is a syscall rather than a
    /// terminal query, so nothing is written to the tty and nothing is awaited.
    pub fn detect(protocol: Protocol) -> Self {
        let cell = crossterm::terminal::window_size()
            .ok()
            .map_or(ASSUMED_CELL, |size| {
                cell_size(size.columns, size.rows, size.width, size.height)
            });
        Self { protocol, cell }
    }
}

/// A resolved image: the file's own bytes and the area they should cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub cols: usize,
    pub rows: usize,
    protocol: Protocol,
    bytes: Vec<u8>,
}

impl Placement {
    /// The escape sequence that draws this image at the cursor. It leaves the
    /// cursor where it was.
    pub fn escape(&self) -> String {
        protocol::place(self.protocol, &self.bytes, self.cols, self.rows)
    }

    pub fn clear(&self) -> Option<&'static str> {
        protocol::clear(self.protocol)
    }
}

/// Environment facts that decide whether images are drawn. Kept as data so the
/// decision stays a pure function and remains testable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub term: Option<String>,
    pub term_program: Option<String>,
    pub kitty_window_id: bool,
    pub konsole_version: bool,
    pub tmux: bool,
}

impl Env {
    pub fn detect() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        Self {
            term: var("TERM"),
            term_program: var("TERM_PROGRAM"),
            kitty_window_id: var("KITTY_WINDOW_ID").is_some(),
            konsole_version: var("KONSOLE_VERSION").is_some(),
            tmux: var("TMUX").is_some(),
        }
    }
}

/// The protocol a terminal is known to support, from environment variables
/// alone. The terminal is never asked, so an unknown terminal gets no images
/// rather than a stray escape sequence in its output.
pub fn protocol_from_env(env: &Env) -> Option<Protocol> {
    // Inside a multiplexer the sequence has to be wrapped for passthrough, and
    // whether that works depends on the outer terminal and the tmux version.
    // Neither is visible here, so `auto` stays off and `--images` opts in.
    if env.tmux {
        return None;
    }
    if env.kitty_window_id
        || env.konsole_version
        || env.term.as_deref() == Some("xterm-kitty")
        || env.term.as_deref() == Some("xterm-ghostty")
    {
        return Some(Protocol::Kitty);
    }
    match env.term_program.as_deref() {
        Some("ghostty") | Some("WezTerm") => Some(Protocol::Kitty),
        Some("iTerm.app") => Some(Protocol::Iterm2),
        _ => None,
    }
}

/// Reads the image `dest` points at and works out its area, or `None` when any
/// rule below is not met. A rejected image keeps its text placeholder; it is
/// never an error and never changes the exit code.
pub fn resolve(
    dest: &str,
    base_dir: Option<&Path>,
    support: ImageSupport,
    max_cols: usize,
) -> Option<Rc<Placement>> {
    if max_cols == 0 {
        return None;
    }
    // A document read from stdin has no directory to confine reads to.
    let base = base_dir?;
    let path = local_path(dest)?;
    let expected = extension_format(&path)?;
    let path = confined(base, &path)?;

    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    // A file whose contents disagree with its name is not sent on: the
    // terminal, not `mdvu`, would be the one to decode it.
    if dimensions::format_of(&bytes) != Some(expected) {
        return None;
    }
    let (cols, rows) = fit(dimensions::dimensions(&bytes)?, support.cell, max_cols);
    Some(Rc::new(Placement {
        cols,
        rows,
        protocol: support.protocol,
        bytes,
    }))
}

/// A local path, or `None` for anything with a URL scheme. Remote images are
/// not fetched: `mdvu` makes no network requests.
fn local_path(dest: &str) -> Option<PathBuf> {
    if dest.is_empty() || has_scheme(dest) {
        return None;
    }
    Some(PathBuf::from(percent_decoded(dest)))
}

/// `scheme:` per RFC 3986, requiring at least two characters so a Windows drive
/// letter is not mistaken for one.
fn has_scheme(dest: &str) -> bool {
    let Some(colon) = dest.find(':') else {
        return false;
    };
    let scheme = &dest[..colon];
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn percent_decoded(dest: &str) -> String {
    let bytes = dest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match (bytes[at], bytes.get(at + 1), bytes.get(at + 2)) {
            (b'%', Some(high), Some(low)) => {
                match (
                    char::from(*high).to_digit(16),
                    char::from(*low).to_digit(16),
                ) {
                    (Some(high), Some(low)) => {
                        out.push((high * 16 + low) as u8);
                        at += 3;
                    }
                    _ => {
                        out.push(bytes[at]);
                        at += 1;
                    }
                }
            }
            _ => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| dest.to_string())
}

fn extension_format(path: &Path) -> Option<Format> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    ALLOWED
        .iter()
        .find(|(name, _)| *name == extension)
        .map(|(_, format)| *format)
}

/// The absolute path, if and only if it stays inside `base`. Both sides are
/// canonicalised, so `../` and a symlink pointing outside are caught the same
/// way. The reader opened one document; a document cannot make `mdvu` read
/// files elsewhere on the machine.
fn confined(base: &Path, path: &Path) -> Option<PathBuf> {
    let base = base.canonicalize().ok()?;
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let resolved = joined.canonicalize().ok()?;
    resolved.starts_with(&base).then_some(resolved)
}

/// Cell size in pixels, falling back when the terminal reports none.
fn cell_size(columns: u16, rows: u16, width: u16, height: u16) -> CellSize {
    if columns == 0 || rows == 0 || width == 0 || height == 0 {
        return ASSUMED_CELL;
    }
    CellSize {
        width: (width / columns).max(1),
        height: (height / rows).max(1),
    }
}

/// Cells the image should cover, never wider than `max_cols` or taller than
/// `MAX_ROWS`, with the aspect ratio kept.
fn fit(pixels: Pixels, cell: CellSize, max_cols: usize) -> (usize, usize) {
    let natural_cols = (pixels.width as usize)
        .div_ceil(usize::from(cell.width))
        .max(1);
    let natural_rows = (pixels.height as usize)
        .div_ceil(usize::from(cell.height))
        .max(1);

    let cols = natural_cols.min(max_cols).max(1);
    let rows = (natural_rows * cols).div_ceil(natural_cols).max(1);
    if rows <= MAX_ROWS {
        return (cols, rows);
    }
    let rows = MAX_ROWS;
    let cols = ((natural_cols * rows) / natural_rows).clamp(1, max_cols);
    (cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        out.extend_from_slice(&13u32.to_be_bytes());
        out.extend_from_slice(b"IHDR");
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&[8, 6, 0, 0, 0]);
        out
    }

    const SUPPORT: ImageSupport = ImageSupport {
        protocol: Protocol::Kitty,
        cell: CellSize {
            width: 10,
            height: 20,
        },
    };

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("creating the fixture directory");
        }
        let mut file = std::fs::File::create(&path).expect("creating the fixture");
        file.write_all(bytes).expect("writing the fixture");
        path
    }

    #[test]
    fn an_image_below_the_base_directory_resolves() {
        let dir = tempfile::tempdir().expect("temporary directory");
        write(dir.path(), ".attachments/a.png", &png(200, 100));
        let placement = resolve(".attachments/a.png", Some(dir.path()), SUPPORT, 80)
            .expect("the image should resolve");
        assert_eq!((placement.cols, placement.rows), (20, 5));
        assert!(placement.escape().starts_with("\x1b_G"));
    }

    #[test]
    fn a_path_escaping_the_base_directory_is_rejected() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let outside = write(dir.path(), "outside.png", &png(10, 10));
        let base = dir.path().join("doc");
        std::fs::create_dir_all(&base).expect("creating the base directory");
        assert!(resolve("../outside.png", Some(&base), SUPPORT, 80).is_none());
        assert!(resolve(outside.to_str().unwrap(), Some(&base), SUPPORT, 80).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_base_directory_is_rejected() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let outside = write(dir.path(), "outside.png", &png(10, 10));
        let base = dir.path().join("doc");
        std::fs::create_dir_all(&base).expect("creating the base directory");
        std::os::unix::fs::symlink(&outside, base.join("link.png")).expect("creating the symlink");
        assert!(resolve("link.png", Some(&base), SUPPORT, 80).is_none());
    }

    #[test]
    fn remote_and_data_destinations_are_never_fetched() {
        let dir = tempfile::tempdir().expect("temporary directory");
        assert!(resolve("https://example.com/a.png", Some(dir.path()), SUPPORT, 80).is_none());
        assert!(resolve("data:image/png;base64,AAAA", Some(dir.path()), SUPPORT, 80).is_none());
    }

    #[test]
    fn a_document_from_stdin_has_nothing_to_resolve_against() {
        assert!(resolve("a.png", None, SUPPORT, 80).is_none());
    }

    #[test]
    fn an_unsupported_extension_is_rejected() {
        let dir = tempfile::tempdir().expect("temporary directory");
        write(dir.path(), "a.svg", b"<svg></svg>");
        assert!(resolve("a.svg", Some(dir.path()), SUPPORT, 80).is_none());
    }

    #[test]
    fn contents_disagreeing_with_the_extension_are_rejected() {
        let dir = tempfile::tempdir().expect("temporary directory");
        write(dir.path(), "a.png", b"GIF89a\x01\x00\x01\x00\x00\x00\x00");
        assert!(resolve("a.png", Some(dir.path()), SUPPORT, 80).is_none());
    }

    #[test]
    fn an_oversized_file_is_rejected() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let mut bytes = png(10, 10);
        bytes.resize(MAX_BYTES as usize + 1, 0);
        write(dir.path(), "big.png", &bytes);
        assert!(resolve("big.png", Some(dir.path()), SUPPORT, 80).is_none());
    }

    #[test]
    fn a_percent_encoded_name_resolves() {
        let dir = tempfile::tempdir().expect("temporary directory");
        write(dir.path(), "a b.png", &png(10, 10));
        assert!(resolve("a%20b.png", Some(dir.path()), SUPPORT, 80).is_some());
    }

    #[test]
    fn a_missing_file_is_rejected() {
        let dir = tempfile::tempdir().expect("temporary directory");
        assert!(resolve("nope.png", Some(dir.path()), SUPPORT, 80).is_none());
    }

    #[test]
    fn an_image_wider_than_the_column_is_scaled_down() {
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        // 100x40 cells naturally, capped to 40 columns.
        assert_eq!(
            fit(
                Pixels {
                    width: 1000,
                    height: 800
                },
                cell,
                40
            ),
            (40, 16)
        );
    }

    #[test]
    fn a_tall_image_is_capped_in_rows_and_keeps_its_ratio() {
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        let (cols, rows) = fit(
            Pixels {
                width: 200,
                height: 2000,
            },
            cell,
            80,
        );
        assert_eq!(rows, MAX_ROWS);
        assert_eq!(cols, 4);
    }

    #[test]
    fn a_small_image_keeps_its_natural_size() {
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        assert_eq!(
            fit(
                Pixels {
                    width: 50,
                    height: 40
                },
                cell,
                80
            ),
            (5, 2)
        );
    }

    #[test]
    fn a_terminal_reporting_no_pixel_size_gets_the_assumed_cell() {
        assert_eq!(cell_size(80, 24, 0, 0), ASSUMED_CELL);
        assert_eq!(
            cell_size(80, 24, 800, 480),
            CellSize {
                width: 10,
                height: 20
            }
        );
    }

    #[test]
    fn protocols_are_recognised_from_the_environment() {
        let kitty = Env {
            term: Some("xterm-kitty".into()),
            ..Env::default()
        };
        assert_eq!(protocol_from_env(&kitty), Some(Protocol::Kitty));

        let ghostty = Env {
            term_program: Some("ghostty".into()),
            ..Env::default()
        };
        assert_eq!(protocol_from_env(&ghostty), Some(Protocol::Kitty));

        let iterm = Env {
            term_program: Some("iTerm.app".into()),
            ..Env::default()
        };
        assert_eq!(protocol_from_env(&iterm), Some(Protocol::Iterm2));

        let apple = Env {
            term_program: Some("Apple_Terminal".into()),
            term: Some("xterm-256color".into()),
            ..Env::default()
        };
        assert_eq!(protocol_from_env(&apple), None);
        assert_eq!(protocol_from_env(&Env::default()), None);
    }

    /// Passthrough depends on the outer terminal and the tmux version, neither
    /// of which is visible from the environment.
    #[test]
    fn a_multiplexer_turns_detection_off() {
        let env = Env {
            term: Some("xterm-kitty".into()),
            tmux: true,
            ..Env::default()
        };
        assert_eq!(protocol_from_env(&env), None);
    }
}
