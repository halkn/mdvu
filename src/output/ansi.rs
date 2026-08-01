use std::io::Write;

use crate::layout::RenderedDocument;
use crate::layout::theme::{Color, Style, Theme};
use crate::output::trimmed;

const RESET: &str = "\x1b[0m";

pub fn write_document(
    out: &mut impl Write,
    document: &RenderedDocument,
    theme: &Theme,
) -> std::io::Result<()> {
    for line in &document.lines {
        for span in trimmed(line) {
            let style = theme.style(span.role);
            match sgr(style) {
                Some(prefix) => write!(out, "{prefix}{}{RESET}", span.text)?,
                None => out.write_all(span.text.as_bytes())?,
            }
        }
        out.write_all(b"\n")?;
    }
    out.flush()
}

/// SGR introducer for a style, or `None` when the style adds nothing.
pub fn sgr(style: Style) -> Option<String> {
    let mut codes: Vec<u8> = Vec::new();
    if style.bold {
        codes.push(1);
    }
    if style.dim {
        codes.push(2);
    }
    if style.italic {
        codes.push(3);
    }
    if style.underline {
        codes.push(4);
    }
    if style.reverse {
        codes.push(7);
    }
    if style.strikethrough {
        codes.push(9);
    }
    if let Some(fg) = style.fg {
        codes.push(foreground(fg));
    }
    if let Some(bg) = style.bg {
        codes.push(foreground(bg) + 10);
    }
    if codes.is_empty() {
        return None;
    }
    let body = codes
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(";");
    Some(format!("\x1b[{body}m"))
}

fn foreground(color: Color) -> u8 {
    match color {
        Color::Red => 31,
        Color::Green => 32,
        Color::Yellow => 33,
        Color::Blue => 34,
        Color::Magenta => 35,
        Color::BrightBlack => 90,
        Color::BrightGreen => 92,
        Color::BrightYellow => 93,
        Color::BrightBlue => 94,
        Color::BrightCyan => 96,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::StyleRole;
    use crate::layout::theme::Variant;

    #[test]
    fn plain_styles_emit_nothing() {
        assert_eq!(sgr(Style::default()), None);
    }

    #[test]
    fn attributes_and_colors_combine() {
        let style = Theme::new(Variant::Dark).style(StyleRole::Heading(1));
        let sequence = sgr(style).expect("heading should be styled");
        assert!(sequence.starts_with("\x1b["));
        assert!(sequence.ends_with('m'));
        assert!(sequence.contains('1'));
    }
}
