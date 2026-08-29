//! Shared presentation model for timeline tool cards.
//!
//! Design language:
//! - Title row: `▾ tool  <title_meta>` — scannable primary fact on one line
//! - Body: typed lines (meta chips, code, terminal, checklist, diagrams)
//! - Prefer density + symbols over key:value JSON dumps

use super::diff::DiffView;

/// One render-ready tool card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPresentation {
    /// Text after the tool name on the title row (path, cmd, stats, …).
    pub title_meta: Option<String>,
    /// Collapsed secondary line when `title_meta` is empty.
    pub collapsed_preview: String,
    /// Optional right-zone badge replacing the default tool-status glyph
    /// (e.g. shell `exit 127`). When set, also drives card background tone.
    pub title_badge: Option<TitleBadge>,
    pub body: ToolBody,
}

/// Right-title chrome for command-like tools (exit code, running, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleBadge {
    /// Short label, e.g. `exit 0`, `exit 127`, `running`.
    pub text: String,
    pub tone: BadgeTone,
    /// Optional wall-clock duration, e.g. `0.06s`, shown after the badge.
    pub duration: Option<String>,
}

impl TitleBadge {
    pub fn new(text: impl Into<String>, tone: BadgeTone) -> Self {
        Self {
            text: text.into(),
            tone,
            duration: None,
        }
    }

    pub fn with_duration(mut self, duration: Option<String>) -> Self {
        self.duration = duration;
        self
    }
}

/// Semantic tone for card background and badge color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeTone {
    Success,
    Error,
    Warning,
    Running,
    Neutral,
}

impl ToolPresentation {
    pub fn with_meta(meta: impl Into<String>, body: ToolBody) -> Self {
        Self {
            title_meta: Some(meta.into()),
            collapsed_preview: String::new(),
            title_badge: None,
            body,
        }
    }

    pub fn with_meta_badge(meta: impl Into<String>, badge: TitleBadge, body: ToolBody) -> Self {
        Self {
            title_meta: Some(meta.into()),
            collapsed_preview: String::new(),
            title_badge: Some(badge),
            body,
        }
    }

    pub fn with_preview(preview: impl Into<String>, body: ToolBody) -> Self {
        Self {
            title_meta: None,
            collapsed_preview: preview.into(),
            title_badge: None,
            body,
        }
    }

    /// Flatten body to plain strings (tests / simple consumers).
    #[allow(dead_code)]
    pub fn plain_body_lines(&self) -> Vec<String> {
        match &self.body {
            ToolBody::Empty => Vec::new(),
            ToolBody::Diff(diff) => diff.to_plain_lines(),
            ToolBody::Code(code) => code.to_plain_lines(),
            ToolBody::Blocks(lines) => lines.iter().map(BodyLine::to_plain).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolBody {
    Empty,
    /// IDE-style file change (edit / rewrite).
    Diff(DiffView),
    /// File viewer with line numbers (read).
    Code(CodeView),
    /// Typed layout rows (shell, todo, agents, generic, …).
    Blocks(Vec<BodyLine>),
}

/// Numbered source listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeView {
    pub start_line: usize,
    pub lines: Vec<String>,
    /// Syntax token inferred from the workspace path (usually an extension).
    pub language: Option<String>,
    pub footer: Option<String>,
}

impl CodeView {
    #[allow(dead_code)]
    pub fn to_plain_lines(&self) -> Vec<String> {
        let w = self.gutter_width();
        let mut out: Vec<String> = self
            .lines
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let no = self.start_line.saturating_add(i);
                format!("{no:>w$}  {text}")
            })
            .collect();
        if let Some(footer) = &self.footer {
            out.push(footer.clone());
        }
        out
    }

    pub fn gutter_width(&self) -> usize {
        let last = self
            .start_line
            .saturating_add(self.lines.len().saturating_sub(1))
            .max(1);
        last.to_string().len().max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyLine {
    Gap,
    /// `key  value` chip row.
    Meta {
        key: String,
        value: String,
    },
    /// Free text with a semantic kind for paint.
    Text {
        kind: LineKind,
        text: String,
    },
}

impl BodyLine {
    pub fn plain(text: impl Into<String>) -> Self {
        Self::Text {
            kind: LineKind::Plain,
            text: text.into(),
        }
    }

    pub fn dim(text: impl Into<String>) -> Self {
        Self::Text {
            kind: LineKind::Dim,
            text: text.into(),
        }
    }

    pub fn prompt(text: impl Into<String>) -> Self {
        Self::Text {
            kind: LineKind::Prompt,
            text: text.into(),
        }
    }

    pub fn terminal(text: impl Into<String>) -> Self {
        Self::Text {
            kind: LineKind::Terminal,
            text: text.into(),
        }
    }

    #[allow(dead_code)]
    pub fn success(text: impl Into<String>) -> Self {
        Self::Text {
            kind: LineKind::Success,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self::Text {
            kind: LineKind::Error,
            text: text.into(),
        }
    }

    pub fn quote(text: impl Into<String>) -> Self {
        Self::Text {
            kind: LineKind::Quote,
            text: text.into(),
        }
    }

    pub fn todo(kind: LineKind, text: impl Into<String>) -> Self {
        Self::Text {
            kind,
            text: text.into(),
        }
    }

    pub fn meta(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Meta {
            key: key.into(),
            value: value.into(),
        }
    }

    #[allow(dead_code)]
    pub fn to_plain(&self) -> String {
        match self {
            Self::Gap => String::new(),
            Self::Meta { key, value } => {
                if value.is_empty() {
                    key.clone()
                } else {
                    format!("{key}  {value}")
                }
            }
            Self::Text { text, .. } => text.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Plain,
    Dim,
    Prompt,
    Terminal,
    Success,
    Error,
    Quote,
    TodoDone,
    TodoActive,
    TodoPending,
}
