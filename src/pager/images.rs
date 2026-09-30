//! Images on the pager's screen.
//!
//! `ratatui` cells hold text only, so images are written to stdout after the
//! frame has been drawn. What is on screen is worked out here as a pure
//! function; `app.rs` only compares the result with the previous frame and
//! sends the bytes.

use std::collections::HashSet;
use std::io::Write;
use std::rc::Rc;

use crate::error::{AppError, Result};
use crate::image::{MAX_ROWS, Placement, Protocol, protocol};
use crate::layout::RenderedLine;
use crate::layout::wrap::display_width;

/// The rows of an image on screen and the cell their top left corner sits in.
#[derive(Debug, Clone)]
pub struct Placed {
    pub placement: Rc<Placement>,
    pub col: u16,
    pub row: u16,
    /// Image rows scrolled off the top.
    pub skip: usize,
    /// Image rows on screen.
    pub rows: usize,
}

/// Compared by identity rather than by content: two frames showing the same
/// image must not re-send its payload, and the payload can be megabytes.
impl PartialEq for Placed {
    fn eq(&self, other: &Self) -> bool {
        self.col == other.col
            && self.row == other.row
            && self.skip == other.skip
            && self.rows == other.rows
            && Rc::ptr_eq(&self.placement, &other.placement)
    }
}

/// Images inside the viewport, with their screen positions.
///
/// An image crossing the top or bottom edge is cropped to the rows on screen
/// where the terminal keeps it stored. Otherwise it is left out: cropping would
/// mean sending it again, and half an image drawn over the status bar is worse
/// than none.
pub fn visible(
    lines: &[RenderedLine],
    top: usize,
    left: usize,
    width: usize,
    height: usize,
) -> Vec<Placed> {
    let mut out = Vec::new();
    // An image starting above the viewport can still reach into it.
    let first = top.saturating_sub(MAX_ROWS);
    for (index, line) in lines
        .iter()
        .enumerate()
        .skip(first)
        .take(top - first + height)
    {
        let Some(placement) = &line.image else {
            continue;
        };
        // Whatever the line already carries is the indent: a quote border or a
        // list marker put there by the enclosing container.
        let indent = display_width(&line.text());
        if indent < left {
            continue;
        }
        let col = indent - left;
        if col + placement.cols > width {
            continue;
        }
        let skip = top.saturating_sub(index);
        let row = index.saturating_sub(top);
        let rows = placement
            .rows
            .saturating_sub(skip)
            .min(height.saturating_sub(row));
        if rows == 0 || (rows < placement.rows && !placement.stored()) {
            continue;
        }
        out.push(Placed {
            placement: Rc::clone(placement),
            col: col as u16,
            row: row as u16,
            skip,
            rows,
        });
    }
    out
}

/// Whether the screen must be repainted before a frame with different images.
/// Kitty moves and removes its own images; iTerm2 draws into the cells, so the
/// only way back is to repaint them.
pub fn needs_repaint(shown: &[Placed]) -> bool {
    shown.iter().any(|placed| !placed.placement.stored())
}

/// Removes every image and frees what the terminal stores, including images
/// scrolled off screen. Used when the layout is rebuilt, since its placements
/// are new, and on the way out.
pub fn release(protocol: Option<Protocol>, sent: &mut HashSet<u32>) -> Result<()> {
    let bytes = release_bytes(protocol, sent);
    sent.clear();
    match bytes.is_empty() {
        true => Ok(()),
        false => write(&bytes),
    }
}

fn release_bytes(protocol: Option<Protocol>, sent: &HashSet<u32>) -> String {
    if protocol != Some(Protocol::Kitty) {
        return String::new();
    }
    let mut ids: Vec<u32> = sent.iter().copied().collect();
    ids.sort_unstable();
    ids.into_iter().map(protocol::free).collect()
}

/// Replaces the images in `before` with those in `wanted`. Called after the
/// frame, so nothing `ratatui` writes lands on top.
pub fn draw(before: &[Placed], wanted: &[Placed], sent: &mut HashSet<u32>) -> Result<()> {
    let bytes = frame(before, wanted, sent);
    match bytes.is_empty() {
        true => Ok(()),
        false => write(&bytes),
    }
}

/// The bytes that turn `before` into `wanted`. A kitty image is stored the
/// first time it is seen and only placed after that: re-sending a diagram on
/// every scroll step is hundreds of kilobytes per line moved. Placing it again
/// replaces its previous placement, so only an image leaving the screen is
/// removed.
fn frame(before: &[Placed], wanted: &[Placed], sent: &mut HashSet<u32>) -> String {
    let mut out = String::new();
    for gone in before.iter().filter(|old| {
        !wanted
            .iter()
            .any(|new| new.placement.id == old.placement.id)
    }) {
        if let Some(remove) = gone.placement.remove() {
            out.push_str(&remove);
        }
    }
    for placed in wanted {
        if sent.insert(placed.placement.id)
            && let Some(transmit) = placed.placement.transmit()
        {
            out.push_str(&transmit);
        }
        out.push_str(&format!("\x1b[{};{}H", placed.row + 1, placed.col + 1));
        out.push_str(&placed.placement.show(placed.skip, placed.rows));
    }
    out
}

fn write(bytes: &str) -> Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(bytes.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|source| AppError::Output { source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{CellSize, ImageSupport, Protocol};
    use crate::layout::RenderedSpan;
    use crate::layout::StyleRole;

    fn placement(cols: usize, rows: usize) -> Rc<Placement> {
        placement_for(Protocol::Kitty, cols, rows)
    }

    fn placement_for(protocol: Protocol, cols: usize, rows: usize) -> Rc<Placement> {
        let dir = tempfile::tempdir().expect("temporary directory");
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&((cols * 10) as u32).to_be_bytes());
        png.extend_from_slice(&((rows * 20) as u32).to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        let path = dir.path().join("a.png");
        std::fs::File::create(&path)
            .expect("creating the fixture")
            .write_all(&png)
            .expect("writing the fixture");
        let support = ImageSupport {
            protocol,
            cell: CellSize {
                width: 10,
                height: 20,
            },
        };
        crate::image::resolve("a.png", Some(dir.path()), None, support, cols).expect("resolvable")
    }

    fn line(image: Option<Rc<Placement>>, indent: &str) -> RenderedLine {
        RenderedLine {
            spans: match indent.is_empty() {
                true => Vec::new(),
                false => vec![RenderedSpan::new(indent, StyleRole::Quote)],
            },
            source_range: None,
            no_wrap: true,
            image,
        }
    }

    fn document(image: Rc<Placement>, before: usize, indent: &str) -> Vec<RenderedLine> {
        let mut lines: Vec<RenderedLine> = (0..before).map(|_| line(None, "")).collect();
        lines.push(line(Some(image), indent));
        lines.extend((0..20).map(|_| line(None, "")));
        lines
    }

    #[test]
    fn an_image_in_view_gets_its_screen_position() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 3, "");
        let shown = visible(&lines, 1, 0, 80, 24);
        assert_eq!(shown.len(), 1);
        assert_eq!((shown[0].col, shown[0].row), (0, 2));
    }

    #[test]
    fn an_indent_moves_the_image_right() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 0, "│ ");
        assert_eq!(visible(&lines, 0, 0, 80, 24)[0].col, 2);
    }

    #[test]
    fn horizontal_scrolling_moves_the_image_left_and_then_off() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 0, "│ ");
        assert_eq!(visible(&lines, 0, 1, 80, 24)[0].col, 1);
        assert!(visible(&lines, 0, 3, 80, 24).is_empty());
    }

    #[test]
    fn an_image_scrolled_wholly_off_the_top_is_not_drawn() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 3, "");
        assert!(visible(&lines, 7, 0, 80, 24).is_empty());
    }

    /// Kitty keeps the image stored, so a partly scrolled one is cropped to
    /// the rows on screen instead of vanishing.
    #[test]
    fn a_kitty_image_crossing_the_top_edge_shows_its_lower_rows() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 3, "");
        let shown = visible(&lines, 4, 0, 80, 24);
        assert_eq!(shown.len(), 1);
        assert_eq!((shown[0].row, shown[0].skip, shown[0].rows), (0, 1, 3));
    }

    #[test]
    fn a_kitty_image_crossing_the_bottom_edge_shows_its_upper_rows() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 0, "");
        let shown = visible(&lines, 0, 0, 80, 3);
        assert_eq!((shown[0].row, shown[0].skip, shown[0].rows), (0, 0, 3));
    }

    /// iTerm2 cannot crop without sending the image again, and half a picture
    /// over the status bar is worse than none.
    #[test]
    fn an_iterm2_image_is_drawn_only_whole() {
        let placement = placement_for(Protocol::Iterm2, 10, 4);
        let lines = document(Rc::clone(&placement), 3, "");
        assert!(visible(&lines, 4, 0, 80, 24).is_empty());
        let lines = document(Rc::clone(&placement), 0, "");
        assert_eq!(visible(&lines, 0, 0, 80, 4).len(), 1);
        assert!(visible(&lines, 0, 0, 80, 3).is_empty());
    }

    #[test]
    fn a_cropped_frame_names_the_source_rows_in_pixels() {
        // 10 x 4 cells from a 100 x 80 px image: 20 px per row.
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 3, "");
        let out = frame(&[], &visible(&lines, 4, 0, 80, 24), &mut HashSet::new());
        assert!(out.contains("a=p,i="), "{out:?}");
        assert!(out.contains(",x=0,y=20,w=100,h=60,c=10,r=3,"), "{out:?}");
    }

    #[test]
    fn an_image_wider_than_the_viewport_is_not_drawn() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 0, "");
        assert_eq!(visible(&lines, 0, 0, 10, 24).len(), 1);
        assert!(visible(&lines, 0, 0, 9, 24).is_empty());
    }

    /// The same image at the same place is the same placement, so `app.rs`
    /// leaves it alone instead of re-sending its bytes every frame.
    #[test]
    fn identical_frames_compare_equal() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 2, "");
        assert_eq!(visible(&lines, 0, 0, 80, 24), visible(&lines, 0, 0, 80, 24));
        assert_ne!(visible(&lines, 0, 0, 80, 24), visible(&lines, 1, 0, 80, 24));
    }

    #[test]
    fn a_kitty_image_is_sent_once_and_then_only_placed() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 0, "");
        let mut sent = HashSet::new();

        let before = visible(&lines, 0, 0, 80, 24);
        let first = frame(&[], &before, &mut sent);
        assert!(first.contains("a=t,"), "first frame transmits");
        assert!(first.contains("a=p,"));

        let after = visible(&lines, 1, 0, 80, 24);
        let second = frame(&before, &after, &mut sent);
        assert!(!second.contains("a=t,"), "later frames only place");
        assert!(second.contains("a=p,"));
    }

    /// An image placed again after `d=a` may not be drawn, so moving relies on
    /// the placement id replacing the old one (`docs/mermaid-image.md`).
    #[test]
    fn moving_a_kitty_image_never_deletes_every_placement() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 2, "");
        let mut sent = HashSet::new();
        let before = visible(&lines, 0, 0, 80, 24);
        frame(&[], &before, &mut sent);
        let out = frame(&before, &visible(&lines, 1, 0, 80, 24), &mut sent);
        assert!(!out.contains("a=d"), "{out:?}");
    }

    #[test]
    fn a_kitty_image_leaving_the_screen_removes_only_its_placement() {
        let placement = placement(10, 4);
        let id = placement.id;
        let lines = document(Rc::clone(&placement), 0, "");
        let mut sent = HashSet::new();
        let before = visible(&lines, 0, 0, 80, 24);
        frame(&[], &before, &mut sent);
        let out = frame(&before, &visible(&lines, 10, 0, 80, 24), &mut sent);
        assert_eq!(out, format!("\x1b_Ga=d,d=i,i={id},p=1,q=2\x1b\\"));
    }

    #[test]
    fn a_frame_positions_the_cursor_before_each_image() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 3, "");
        let out = frame(&[], &visible(&lines, 1, 0, 80, 24), &mut HashSet::new());
        // Row 2, column 0 in 1-based CUP.
        assert!(out.contains("\x1b[3;1H"), "{out:?}");
    }

    /// An image scrolled off screen has no placement, so only naming it frees
    /// what the terminal stores.
    #[test]
    fn releasing_frees_every_stored_image_by_id() {
        let sent = HashSet::from([4, 2]);
        assert_eq!(
            release_bytes(Some(Protocol::Kitty), &sent),
            "\x1b_Ga=d,d=I,i=2,q=2\x1b\\\x1b_Ga=d,d=I,i=4,q=2\x1b\\"
        );
        assert_eq!(release_bytes(Some(Protocol::Iterm2), &sent), "");
    }
}
