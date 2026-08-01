//! Terminal layout: document IR to a rendered surface carrying semantic style
//! roles. No ratatui or ANSI types appear here; the output backends map roles.

pub mod document;
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSpan {
    pub text: String,
    pub role: StyleRole,
}

impl RenderedSpan {
    pub fn new(text: impl Into<String>, role: StyleRole) -> Self {
        Self {
            text: text.into(),
            role,
        }
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
