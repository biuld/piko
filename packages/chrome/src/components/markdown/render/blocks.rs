//! GPUI rendering for Markdown block semantics.

use std::{cell::Cell, rc::Rc};

use gpui::*;

use crate::components::selection::{SelectableText, SelectionState};
use crate::theme::{ChromeTokens, TextRole, metrics, text, tokens};

use super::super::model::{MarkdownBlock, MarkdownList, MarkdownNode};
use super::inline::{styled_inline, styled_inline_with_text};
use super::table::render_table;

#[derive(Clone)]
pub(crate) struct SelectionRenderContext {
    prefix: SharedString,
    selection: Entity<SelectionState>,
    next: Rc<Cell<usize>>,
}

impl SelectionRenderContext {
    pub(crate) fn new(prefix: SharedString, selection: Entity<SelectionState>) -> Self {
        Self {
            prefix,
            selection,
            next: Rc::new(Cell::new(0)),
        }
    }

    pub(crate) fn leaf(&self, value: SharedString, styled: StyledText) -> AnyElement {
        let index = self.next.get();
        self.next.set(index + 1);
        let id = SharedString::from(format!("{}-selection-{index}", self.prefix));
        SelectableText::new(id.clone(), id, value, styled, self.selection.clone())
            .into_any_element()
    }
}

pub(crate) fn render_blocks(
    blocks: &[MarkdownNode<MarkdownBlock>],
    selection: Option<&SelectionRenderContext>,
) -> Vec<AnyElement> {
    blocks
        .iter()
        .map(|block| render_block(block, selection))
        .collect()
}

fn inline_leaf(
    content: &[super::super::model::MarkdownInline],
    strong: bool,
    selection: Option<&SelectionRenderContext>,
) -> AnyElement {
    if let Some(selection) = selection {
        let (value, styled) = styled_inline_with_text(content, strong);
        selection.leaf(value, styled)
    } else {
        styled_inline(content, strong).into_any_element()
    }
}

fn plain_leaf(
    value: impl Into<SharedString>,
    selection: Option<&SelectionRenderContext>,
) -> AnyElement {
    let value = value.into();
    if let Some(selection) = selection {
        selection.leaf(value.clone(), StyledText::new(value))
    } else {
        value.into_any_element()
    }
}

fn render_block(
    node: &MarkdownNode<MarkdownBlock>,
    selection: Option<&SelectionRenderContext>,
) -> AnyElement {
    let m = metrics();
    let t = tokens();
    match &node.value {
        MarkdownBlock::Paragraph(content) => text(TextRole::Body)
            .w_full()
            .min_w_0()
            .text_color(t.fg_rgba())
            .child(inline_leaf(content, false, selection))
            .into_any_element(),
        MarkdownBlock::Heading { level, content } => {
            let scale = match level {
                1 => 1.2,
                2 => 1.12,
                3 => 1.06,
                _ => 1.0,
            };
            text(TextRole::Body)
                .w_full()
                .min_w_0()
                .text_size(m.body_size * scale)
                .line_height(m.body_line_height * scale)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.fg_rgba())
                .child(inline_leaf(content, true, selection))
                .into_any_element()
        }
        MarkdownBlock::BlockQuote(blocks) => div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .border_l_2()
            .border_color(ChromeTokens::hsla(t.accent))
            .pl(m.space_sm)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(m.space_sm)
                    .children(render_blocks(blocks, selection)),
            )
            .into_any_element(),
        MarkdownBlock::List(list) => render_list(list, selection),
        MarkdownBlock::CodeBlock { language, code } => {
            render_code(language.as_deref(), code, selection)
        }
        MarkdownBlock::Table(table) => render_table(table, selection),
        MarkdownBlock::ThematicBreak => div()
            .w_full()
            .h(px(1.))
            .my(m.space_xs)
            .bg(t.border_rgba())
            .into_any_element(),
    }
}

fn render_list(list: &MarkdownList, selection: Option<&SelectionRenderContext>) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let marker_width = if list.start.is_some() {
        px(32.)
    } else {
        px(20.)
    };
    let mut root = div().w_full().min_w_0().flex().flex_col().gap(m.space_xs);

    for (index, item) in list.items.iter().enumerate() {
        let marker = match item.checked {
            Some(true) => "[x]".to_owned(),
            Some(false) => "[ ]".to_owned(),
            None => list
                .start
                .map(|start| format!("{}.", start + index as u64))
                .unwrap_or_else(|| "•".to_owned()),
        };
        root = root.child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_row()
                .items_start()
                .gap(m.space_xs)
                .child(
                    text(TextRole::Body)
                        .w(marker_width)
                        .flex_shrink_0()
                        .text_right()
                        .text_color(t.muted_fg_rgba())
                        .child(plain_leaf(marker, selection)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(m.space_sm)
                        .children(render_blocks(&item.blocks, selection)),
                ),
        );
    }
    root.into_any_element()
}

fn render_code(
    language: Option<&str>,
    code: &str,
    selection: Option<&SelectionRenderContext>,
) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let mut root = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .rounded_md()
        .bg(t.elevated_rgba())
        .overflow_hidden();

    if let Some(language) = language.filter(|language| !language.is_empty()) {
        root = root.child(
            text(TextRole::Meta)
                .w_full()
                .px(m.space_sm)
                .pt(m.space_sm)
                .text_color(t.muted_fg_rgba())
                .child(language.to_owned()),
        );
    }
    root.child(
        text(TextRole::BodyMono)
            .w_full()
            .min_w_0()
            .p(m.space_sm)
            .text_color(t.fg_rgba())
            .child(plain_leaf(code.to_owned(), selection)),
    )
    .into_any_element()
}
