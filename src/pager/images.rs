//! Images on the pager's screen.
//!
//! `ratatui` cells hold text only, so images are written to stdout after the
//! frame has been drawn. What is on screen is worked out here as a pure
//! function; `app.rs` only compares the result with the previous frame and
//! sends the bytes.

use std::io::Write;
use std::rc::Rc;

use crossterm::QueueableCommand;
use crossterm::cursor::MoveTo;

use crate::error::{AppError, Result};
use crate::image::Placement;
use crate::layout::RenderedLine;
use crate::layout::wrap::display_width;

/// An image and the screen cell its top left corner sits in.
#[derive(Debug, Clone)]
pub struct Placed {
    pub placement: Rc<Placement>,
    pub col: u16,
    pub row: u16,
}

/// Compared by identity rather than by content: two frames showing the same
/// image must not re-send its payload, and the payload can be megabytes.
impl PartialEq for Placed {
    fn eq(&self, other: &Self) -> bool {
        self.col == other.col
            && self.row == other.row
            && Rc::ptr_eq(&self.placement, &other.placement)
    }
}

/// Images fully inside the viewport, with their screen positions.
///
/// A partly scrolled image is left out rather than clipped: neither protocol
/// can crop a placement without re-sending it, and half an image drawn over the
/// status bar is worse than none.
pub fn visible(
    lines: &[RenderedLine],
    top: usize,
    left: usize,
    width: usize,
    height: usize,
) -> Vec<Placed> {
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate().skip(top).take(height) {
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
        let row = index - top;
        if col + placement.cols > width || row + placement.rows > height {
            continue;
        }
        out.push(Placed {
            placement: Rc::clone(placement),
            col: col as u16,
            row: row as u16,
        });
    }
    out
}

/// Removes what was drawn before. Kitty deletes its own placements; iTerm2 has
/// no such command, so the caller is told to repaint the cells instead.
#[must_use = "iTerm2 needs a full repaint when this returns false"]
pub fn erase(shown: &[Placed]) -> Result<bool> {
    let Some(first) = shown.first() else {
        return Ok(true);
    };
    let Some(sequence) = first.placement.clear() else {
        return Ok(false);
    };
    let mut out = std::io::stdout().lock();
    out.write_all(sequence.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|source| AppError::Output { source })?;
    Ok(true)
}

/// Draws each image at its cell. Called after the frame, so nothing `ratatui`
/// writes lands on top.
pub fn draw(shown: &[Placed]) -> Result<()> {
    if shown.is_empty() {
        return Ok(());
    }
    let mut out = std::io::stdout().lock();
    for placed in shown {
        out.queue(MoveTo(placed.col, placed.row))
            .map_err(|source| AppError::Output { source })?;
        out.write_all(placed.placement.escape().as_bytes())
            .map_err(|source| AppError::Output { source })?;
    }
    out.flush().map_err(|source| AppError::Output { source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{CellSize, ImageSupport, Protocol};
    use crate::layout::RenderedSpan;
    use crate::layout::StyleRole;

    fn placement(cols: usize, rows: usize) -> Rc<Placement> {
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
            protocol: Protocol::Kitty,
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
    fn an_image_scrolled_off_the_top_is_not_drawn() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 3, "");
        assert!(visible(&lines, 4, 0, 80, 24).is_empty());
    }

    #[test]
    fn an_image_that_would_cross_the_bottom_edge_is_not_drawn() {
        let placement = placement(10, 4);
        let lines = document(Rc::clone(&placement), 0, "");
        // Four rows fit exactly, three do not.
        assert_eq!(visible(&lines, 0, 0, 80, 4).len(), 1);
        assert!(visible(&lines, 0, 0, 80, 3).is_empty());
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
}
