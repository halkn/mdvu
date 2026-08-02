//! Terminal layout: document IR to a rendered surface carrying semantic style
//! roles. No ratatui or ANSI types appear here; the output backends map roles.

pub mod document;
pub mod highlight;
pub mod inline;
pub mod table;
pub mod theme;
pub mod wrap;

pub use document::layout_document;

use crate::diagnostic::Diagnostic;
use crate::source::SourceRange;

/// Semantic role of a run of text. Backends decide the concrete styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleRole {
    Normal,
    Muted,
    Heading(u8),
    Strong,
    Emphasis,
    Strike,
    InlineCode,
    Code,
    CodeBorder,
    Link,
    LinkTarget,
    Quote,
    ListMarker,
    TaskChecked,
    TaskUnchecked,
    TableBorder,
    Diagram,
    DiagramBorder,
    Warning,
    Error,
    SearchMatch,
    InitialLine,
    Status,
    /// A token inside a code block. Highlighting classifies tokens; the theme
    /// still chooses the colours.
    Syntax(SyntaxKind),
}

/// Token classes a code block is split into. Deliberately coarse: these are the
/// distinctions that survive a 16-colour terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxKind {
    Keyword,
    String,
    Number,
    Comment,
    Type,
    Function,
    Punctuation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSpan {
    pub text: String,
    pub role: StyleRole,
    /// Absolute URL this run points at, when a backend can make it clickable.
    /// Only the ANSI stdout backend uses it; the pager renders plain styled text.
    pub link: Option<String>,
}

impl RenderedSpan {
    pub fn new(text: impl Into<String>, role: StyleRole) -> Self {
        Self {
            text: text.into(),
            role,
            link: None,
        }
    }

    pub fn with_link(mut self, link: impl Into<String>) -> Self {
        self.link = Some(link.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderedLine {
    pub spans: Vec<RenderedSpan>,
    pub source_range: Option<SourceRange>,
    /// Set for content that must not be re-wrapped, such as code and diagrams.
    /// The pager scrolls horizontally instead.
    pub no_wrap: bool,
}

impl RenderedLine {
    pub fn blank() -> Self {
        Self::default()
    }

    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct RenderedDocument {
    pub lines: Vec<RenderedLine>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Inputs that change the rendered surface. Layout is re-run when these change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutOptions {
    pub width: usize,
}

impl LayoutOptions {
    pub const MIN_WIDTH: usize = 20;

    pub fn new(width: usize) -> Self {
        Self {
            width: width.max(Self::MIN_WIDTH),
        }
    }
}
