//! Backend-neutral styling for semantic roles.

use crate::cli::Theme as ThemeOption;
use crate::layout::{StyleRole, SyntaxKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,

    BrightBlack,

    BrightGreen,
    BrightYellow,
    BrightBlue,

    BrightCyan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub strikethrough: bool,
    pub reverse: bool,
}

impl Style {
    const fn fg(color: Color) -> Self {
        Self {
            fg: Some(color),
            ..Self::plain()
        }
    }

    const fn plain() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            dim: false,
            strikethrough: false,
            reverse: false,
        }
    }

    const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    const fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    const fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    #[cfg(test)]
    pub fn is_plain(&self) -> bool {
        *self == Self::plain()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Dark,
    Light,
}

impl Variant {
    /// Theme `auto` never issues a blocking terminal query. It uses the
    /// conventional `COLORFGBG` hint when present and falls back to dark.
    pub fn resolve(option: ThemeOption) -> Self {
        match option {
            ThemeOption::Dark => Variant::Dark,
            ThemeOption::Light => Variant::Light,
            ThemeOption::Auto => Self::from_env().unwrap_or(Variant::Dark),
        }
    }

    fn from_env() -> Option<Self> {
        // COLORFGBG is "fg;bg"; a low background index means a dark terminal.
        let value = std::env::var("COLORFGBG").ok()?;
        let bg = value.rsplit(';').next()?.trim().parse::<u8>().ok()?;
        Some(if (7..=15).contains(&bg) {
            Variant::Light
        } else {
            Variant::Dark
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub variant: Variant,
}

impl Theme {
    pub fn new(variant: Variant) -> Self {
        Self { variant }
    }

    pub fn style(&self, role: StyleRole) -> Style {
        let accent = match self.variant {
            Variant::Dark => Color::BrightCyan,
            Variant::Light => Color::Blue,
        };
        let subtle = match self.variant {
            Variant::Dark => Color::BrightBlack,
            Variant::Light => Color::BrightBlack,
        };
        match role {
            StyleRole::Normal => Style::plain(),
            StyleRole::Muted => Style::fg(subtle).dim(),
            StyleRole::Heading(1) => Style::fg(accent).bold().underline(),
            StyleRole::Heading(2) => Style::fg(accent).bold(),
            StyleRole::Heading(_) => Style::fg(accent),
            StyleRole::Strong => Style::plain().bold(),
            StyleRole::Emphasis => Style::plain().italic(),
            StyleRole::Strike => Style::plain().strikethrough(),
            StyleRole::InlineCode | StyleRole::Code => Style::fg(match self.variant {
                Variant::Dark => Color::BrightYellow,
                Variant::Light => Color::Magenta,
            }),
            StyleRole::CodeBorder | StyleRole::TableBorder | StyleRole::DiagramBorder => {
                Style::fg(subtle)
            }
            // Colour only. The underline is reserved for runs a backend has
            // actually made clickable, so it means one thing everywhere.
            StyleRole::Link => Style::fg(match self.variant {
                Variant::Dark => Color::BrightBlue,
                Variant::Light => Color::Blue,
            }),
            StyleRole::LinkTarget => Style::fg(subtle).dim(),
            StyleRole::Quote => Style::fg(match self.variant {
                Variant::Dark => Color::BrightGreen,
                Variant::Light => Color::Green,
            }),
            StyleRole::ListMarker => Style::fg(accent),
            StyleRole::TaskChecked => Style::fg(Color::Green),
            StyleRole::TaskUnchecked => Style::fg(subtle),
            StyleRole::Diagram => Style::plain(),
            StyleRole::Warning => Style::fg(Color::Yellow),
            StyleRole::Error => Style::fg(Color::Red),
            StyleRole::SearchMatch => Style::plain().reverse(),
            StyleRole::InitialLine => Style::fg(Color::Yellow).dim(),
            StyleRole::Status => Style::plain().reverse(),
            StyleRole::Syntax(kind) => self.syntax(kind),
        }
    }

    /// Token colours stay inside the same 16-colour palette as everything else,
    /// and stay clear of the border and link colours so a code block still
    /// reads as one region.
    fn syntax(&self, kind: SyntaxKind) -> Style {
        let dark = self.variant == Variant::Dark;
        match kind {
            SyntaxKind::Keyword => Style::fg(Color::Magenta),
            SyntaxKind::String => Style::fg(if dark {
                Color::BrightGreen
            } else {
                Color::Green
            }),
            SyntaxKind::Number => Style::fg(Color::Yellow),
            SyntaxKind::Comment => Style::fg(Color::BrightBlack).dim(),
            SyntaxKind::Type => Style::fg(if dark { Color::BrightCyan } else { Color::Blue }),
            SyntaxKind::Function => Style::fg(if dark { Color::BrightBlue } else { Color::Blue }),
            // Punctuation is only nudged away from plain text, never coloured
            // strongly enough to compete with the tokens around it.
            SyntaxKind::Punctuation => Style::fg(Color::BrightBlack),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_theme_options_win() {
        assert_eq!(Variant::resolve(ThemeOption::Dark), Variant::Dark);
        assert_eq!(Variant::resolve(ThemeOption::Light), Variant::Light);
    }

    #[test]
    fn normal_text_has_no_styling() {
        assert!(
            Theme::new(Variant::Dark)
                .style(StyleRole::Normal)
                .is_plain()
        );
    }

    #[test]
    fn heading_levels_are_distinguishable() {
        let t = Theme::new(Variant::Dark);
        assert_ne!(
            t.style(StyleRole::Heading(1)),
            t.style(StyleRole::Heading(2))
        );
        assert_ne!(
            t.style(StyleRole::Heading(2)),
            t.style(StyleRole::Heading(3))
        );
    }

    #[test]
    fn both_variants_style_every_role() {
        let roles = [
            StyleRole::Muted,
            StyleRole::Strong,
            StyleRole::Emphasis,
            StyleRole::Strike,
            StyleRole::InlineCode,
            StyleRole::Link,
            StyleRole::Quote,
            StyleRole::ListMarker,
            StyleRole::Warning,
            StyleRole::Error,
        ];
        for variant in [Variant::Dark, Variant::Light] {
            let theme = Theme::new(variant);
            for role in roles {
                assert!(!theme.style(role).is_plain(), "{role:?} in {variant:?}");
            }
        }
    }
}
