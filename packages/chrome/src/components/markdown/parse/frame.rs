//! Parser stack frames.

use super::super::model::{
    InlineStyle, MarkdownAlignment, MarkdownBlock, MarkdownInline, MarkdownListItem, MarkdownNode,
    MarkdownTableCell,
};

pub(super) enum Frame {
    Document(Vec<MarkdownNode<MarkdownBlock>>),
    Paragraph {
        start: usize,
        content: Vec<MarkdownInline>,
    },
    Heading {
        start: usize,
        level: u8,
        content: Vec<MarkdownInline>,
    },
    BlockQuote {
        start: usize,
        blocks: Vec<MarkdownNode<MarkdownBlock>>,
    },
    List {
        start: usize,
        ordered_start: Option<u64>,
        items: Vec<MarkdownListItem>,
    },
    Item {
        start: usize,
        checked: Option<bool>,
        blocks: Vec<MarkdownNode<MarkdownBlock>>,
    },
    CodeBlock {
        start: usize,
        language: Option<String>,
        code: String,
    },
    HtmlBlock {
        start: usize,
        literal: String,
    },
    Inline {
        kind: InlineFrame,
        children: Vec<MarkdownInline>,
    },
    Table {
        start: usize,
        alignments: Vec<MarkdownAlignment>,
        head: Vec<MarkdownTableCell>,
        rows: Vec<Vec<MarkdownTableCell>>,
    },
    TableRow {
        head: bool,
        cells: Vec<MarkdownTableCell>,
    },
    TableCell {
        content: Vec<MarkdownInline>,
    },
}

pub(super) enum InlineFrame {
    Styled(InlineStyle),
    Link(String),
    Image,
}

impl Default for Frame {
    fn default() -> Self {
        Self::Document(Vec::new())
    }
}
