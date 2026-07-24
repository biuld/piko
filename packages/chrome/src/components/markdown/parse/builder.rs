//! Event-stack builder for the owned Markdown model.

use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};

use super::super::model::{
    InlineStyle, MarkdownAlignment, MarkdownBlock, MarkdownDocument, MarkdownInline, MarkdownList,
    MarkdownListItem, MarkdownNode, MarkdownTable, MarkdownTableCell,
};
use super::ParseError;
use super::frame::{Frame, InlineFrame};

const MAX_NESTING: usize = 128;

pub(super) struct Builder {
    frames: Vec<Frame>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            frames: vec![Frame::default()],
        }
    }
}

impl Builder {
    pub(super) fn event(
        &mut self,
        event: Event<'_>,
        range: Range<usize>,
    ) -> Result<(), ParseError> {
        match event {
            Event::Start(tag) => self.start(tag, range.start),
            Event::End(tag) => self.end(tag, range.end),
            Event::Text(text) => self.text(text.as_ref()),
            Event::Code(code) => self.inline(MarkdownInline::Code(code.into_string())),
            Event::Html(html) | Event::InlineHtml(html) => self.text(html.as_ref()),
            Event::SoftBreak => self.inline(MarkdownInline::SoftBreak),
            Event::HardBreak => self.inline(MarkdownInline::HardBreak),
            Event::Rule => self.block(MarkdownNode {
                source_range: range,
                value: MarkdownBlock::ThematicBreak,
            }),
            Event::TaskListMarker(checked) => {
                let item = self.frames.iter_mut().rev().find_map(|frame| match frame {
                    Frame::Item { checked, .. } => Some(checked),
                    _ => None,
                });
                *item.ok_or(ParseError)? = Some(checked);
                Ok(())
            }
            Event::InlineMath(math) => self.text(&format!("${math}$")),
            Event::DisplayMath(math) => self.text(&format!("$${math}$$")),
            Event::FootnoteReference(label) => self.text(&format!("[^{label}]")),
        }
    }

    fn start(&mut self, tag: Tag<'_>, start: usize) -> Result<(), ParseError> {
        if self.frames.len() >= MAX_NESTING {
            return Err(ParseError);
        }
        let frame = match tag {
            Tag::Paragraph => Frame::Paragraph {
                start,
                content: Vec::new(),
            },
            Tag::Heading { level, .. } => Frame::Heading {
                start,
                level: heading_level(level),
                content: Vec::new(),
            },
            Tag::BlockQuote(_) => Frame::BlockQuote {
                start,
                blocks: Vec::new(),
            },
            Tag::List(ordered_start) => Frame::List {
                start,
                ordered_start,
                items: Vec::new(),
            },
            Tag::Item => Frame::Item {
                start,
                checked: None,
                blocks: Vec::new(),
            },
            Tag::CodeBlock(kind) => Frame::CodeBlock {
                start,
                language: code_language(kind),
                code: String::new(),
            },
            Tag::HtmlBlock => Frame::HtmlBlock {
                start,
                literal: String::new(),
            },
            Tag::Emphasis => inline_frame(InlineFrame::Styled(InlineStyle::Emphasis)),
            Tag::Strong => inline_frame(InlineFrame::Styled(InlineStyle::Strong)),
            Tag::Strikethrough => inline_frame(InlineFrame::Styled(InlineStyle::Strikethrough)),
            Tag::Link { dest_url, .. } => inline_frame(InlineFrame::Link(dest_url.into_string())),
            Tag::Image { .. } => inline_frame(InlineFrame::Image),
            Tag::Table(alignments) => Frame::Table {
                start,
                alignments: alignments.into_iter().map(table_alignment).collect(),
                head: Vec::new(),
                rows: Vec::new(),
            },
            Tag::TableHead => Frame::TableRow {
                head: true,
                cells: Vec::new(),
            },
            Tag::TableRow => Frame::TableRow {
                head: false,
                cells: Vec::new(),
            },
            Tag::TableCell => Frame::TableCell {
                content: Vec::new(),
            },
            Tag::FootnoteDefinition(_) | Tag::MetadataBlock(_) => return Err(ParseError),
        };
        self.frames.push(frame);
        Ok(())
    }

    fn end(&mut self, tag: TagEnd, end: usize) -> Result<(), ParseError> {
        let frame = self.frames.pop().ok_or(ParseError)?;
        match (tag, frame) {
            (TagEnd::Paragraph, Frame::Paragraph { start, content }) => {
                self.block(node(start, end, MarkdownBlock::Paragraph(content)))
            }
            (
                TagEnd::Heading(_),
                Frame::Heading {
                    start,
                    level,
                    content,
                },
            ) => self.block(node(start, end, MarkdownBlock::Heading { level, content })),
            (TagEnd::BlockQuote, Frame::BlockQuote { start, blocks }) => {
                self.block(node(start, end, MarkdownBlock::BlockQuote(blocks)))
            }
            (
                TagEnd::List(_),
                Frame::List {
                    start,
                    ordered_start,
                    items,
                },
            ) => self.block(node(
                start,
                end,
                MarkdownBlock::List(MarkdownList {
                    start: ordered_start,
                    items,
                }),
            )),
            (
                TagEnd::Item,
                Frame::Item {
                    start,
                    checked,
                    mut blocks,
                },
            ) => {
                for block in &mut blocks {
                    if block.source_range.is_empty() {
                        block.source_range = start..end;
                    }
                }
                self.item(MarkdownListItem { checked, blocks })
            }
            (
                TagEnd::CodeBlock,
                Frame::CodeBlock {
                    start,
                    language,
                    code,
                },
            ) => self.block(node(
                start,
                end,
                MarkdownBlock::CodeBlock { language, code },
            )),
            (TagEnd::HtmlBlock, Frame::HtmlBlock { start, literal }) => self.block(node(
                start,
                end,
                MarkdownBlock::Paragraph(vec![MarkdownInline::Text(literal)]),
            )),
            (
                TagEnd::Emphasis,
                Frame::Inline {
                    kind: InlineFrame::Styled(InlineStyle::Emphasis),
                    children,
                    ..
                },
            ) => self.inline(MarkdownInline::Styled {
                kind: InlineStyle::Emphasis,
                children,
            }),
            (
                TagEnd::Strong,
                Frame::Inline {
                    kind: InlineFrame::Styled(InlineStyle::Strong),
                    children,
                    ..
                },
            ) => self.inline(MarkdownInline::Styled {
                kind: InlineStyle::Strong,
                children,
            }),
            (
                TagEnd::Strikethrough,
                Frame::Inline {
                    kind: InlineFrame::Styled(InlineStyle::Strikethrough),
                    children,
                    ..
                },
            ) => self.inline(MarkdownInline::Styled {
                kind: InlineStyle::Strikethrough,
                children,
            }),
            (
                TagEnd::Link,
                Frame::Inline {
                    kind: InlineFrame::Link(destination),
                    children,
                    ..
                },
            ) => self.inline(MarkdownInline::Link {
                destination,
                children,
            }),
            (
                TagEnd::Image,
                Frame::Inline {
                    kind: InlineFrame::Image,
                    children,
                    ..
                },
            ) => self.inline(MarkdownInline::ImageAlt(children)),
            (TagEnd::TableCell, Frame::TableCell { content, .. }) => {
                self.table_cell(MarkdownTableCell { content })
            }
            (
                TagEnd::TableHead,
                Frame::TableRow {
                    head: true, cells, ..
                },
            ) => self.table_row(true, cells),
            (
                TagEnd::TableRow,
                Frame::TableRow {
                    head: false, cells, ..
                },
            ) => self.table_row(false, cells),
            (
                TagEnd::Table,
                Frame::Table {
                    start,
                    alignments,
                    head,
                    rows,
                },
            ) => self.block(node(
                start,
                end,
                MarkdownBlock::Table(MarkdownTable {
                    alignments,
                    head,
                    rows,
                }),
            )),
            _ => Err(ParseError),
        }
    }

    fn text(&mut self, text: &str) -> Result<(), ParseError> {
        match self.frames.last_mut().ok_or(ParseError)? {
            Frame::CodeBlock { code, .. } => code.push_str(text),
            Frame::HtmlBlock { literal, .. } => literal.push_str(text),
            _ => return self.inline(MarkdownInline::Text(text.to_owned())),
        }
        Ok(())
    }

    fn inline(&mut self, inline: MarkdownInline) -> Result<(), ParseError> {
        match self.frames.last_mut().ok_or(ParseError)? {
            Frame::Paragraph { content, .. }
            | Frame::Heading { content, .. }
            | Frame::TableCell { content, .. } => content.push(inline),
            Frame::Inline { children, .. } => children.push(inline),
            Frame::Item { start, blocks, .. } => {
                if let Some(MarkdownNode {
                    value: MarkdownBlock::Paragraph(content),
                    ..
                }) = blocks.last_mut()
                {
                    content.push(inline);
                } else {
                    blocks.push(MarkdownNode {
                        source_range: *start..*start,
                        value: MarkdownBlock::Paragraph(vec![inline]),
                    });
                }
            }
            _ => return Err(ParseError),
        }
        Ok(())
    }

    fn block(&mut self, block: MarkdownNode<MarkdownBlock>) -> Result<(), ParseError> {
        match self.frames.last_mut().ok_or(ParseError)? {
            Frame::Document(blocks)
            | Frame::BlockQuote { blocks, .. }
            | Frame::Item { blocks, .. } => blocks.push(block),
            _ => return Err(ParseError),
        }
        Ok(())
    }

    fn item(&mut self, item: MarkdownListItem) -> Result<(), ParseError> {
        match self.frames.last_mut().ok_or(ParseError)? {
            Frame::List { items, .. } => items.push(item),
            _ => return Err(ParseError),
        }
        Ok(())
    }

    fn table_cell(&mut self, cell: MarkdownTableCell) -> Result<(), ParseError> {
        match self.frames.last_mut().ok_or(ParseError)? {
            Frame::TableRow { cells, .. } => cells.push(cell),
            _ => return Err(ParseError),
        }
        Ok(())
    }

    fn table_row(&mut self, head: bool, cells: Vec<MarkdownTableCell>) -> Result<(), ParseError> {
        match self.frames.last_mut().ok_or(ParseError)? {
            Frame::Table {
                head: table_head,
                rows,
                ..
            } if head => *table_head = cells,
            Frame::Table { rows, .. } => rows.push(cells),
            _ => return Err(ParseError),
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<MarkdownDocument, ParseError> {
        if self.frames.is_empty() {
            self.frames.push(Frame::default());
        }
        match self.frames.pop() {
            Some(Frame::Document(blocks)) if self.frames.is_empty() => {
                Ok(MarkdownDocument { blocks })
            }
            _ => Err(ParseError),
        }
    }
}

fn node(start: usize, end: usize, value: MarkdownBlock) -> MarkdownNode<MarkdownBlock> {
    MarkdownNode {
        source_range: start..end,
        value,
    }
}

fn inline_frame(kind: InlineFrame) -> Frame {
    Frame::Inline {
        kind,
        children: Vec::new(),
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    level as u8
}

fn code_language(kind: CodeBlockKind<'_>) -> Option<String> {
    match kind {
        CodeBlockKind::Indented => None,
        CodeBlockKind::Fenced(info) => info
            .split([',', ' ', '\t'])
            .next()
            .filter(|language| !language.is_empty())
            .map(str::to_owned),
    }
}

fn table_alignment(alignment: Alignment) -> MarkdownAlignment {
    match alignment {
        Alignment::None => MarkdownAlignment::None,
        Alignment::Left => MarkdownAlignment::Left,
        Alignment::Center => MarkdownAlignment::Center,
        Alignment::Right => MarkdownAlignment::Right,
    }
}
