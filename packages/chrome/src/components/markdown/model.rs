//! Owned, renderer-facing Markdown semantics.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarkdownDocument {
    pub(crate) blocks: Vec<MarkdownNode<MarkdownBlock>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkdownNode<T> {
    pub(crate) source_range: Range<usize>,
    pub(crate) value: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MarkdownBlock {
    Paragraph(Vec<MarkdownInline>),
    Heading {
        level: u8,
        content: Vec<MarkdownInline>,
    },
    BlockQuote(Vec<MarkdownNode<MarkdownBlock>>),
    List(MarkdownList),
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    Table(MarkdownTable),
    ThematicBreak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkdownList {
    pub(crate) start: Option<u64>,
    pub(crate) items: Vec<MarkdownListItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkdownListItem {
    pub(crate) checked: Option<bool>,
    pub(crate) blocks: Vec<MarkdownNode<MarkdownBlock>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkdownTable {
    pub(crate) alignments: Vec<MarkdownAlignment>,
    pub(crate) head: Vec<MarkdownTableCell>,
    pub(crate) rows: Vec<Vec<MarkdownTableCell>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkdownAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkdownTableCell {
    pub(crate) content: Vec<MarkdownInline>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MarkdownInline {
    Text(String),
    Styled {
        kind: InlineStyle,
        children: Vec<MarkdownInline>,
    },
    Code(String),
    Link {
        destination: String,
        children: Vec<MarkdownInline>,
    },
    ImageAlt(Vec<MarkdownInline>),
    SoftBreak,
    HardBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InlineStyle {
    Emphasis,
    Strong,
    Strikethrough,
}

impl MarkdownDocument {
    pub(crate) fn plain(source: &str) -> Self {
        if source.is_empty() {
            return Self::default();
        }
        Self {
            blocks: vec![MarkdownNode {
                source_range: 0..source.len(),
                value: MarkdownBlock::Paragraph(vec![MarkdownInline::Text(source.to_owned())]),
            }],
        }
    }
}
