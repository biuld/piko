//! GPUI rendering for Markdown block semantics.

use gpui::*;

use crate::theme::{ChromeTokens, TextRole, metrics, text, tokens};

use super::super::model::{MarkdownBlock, MarkdownList, MarkdownNode};
use super::inline::styled_inline;
use super::table::render_table;

pub(crate) fn render_blocks(blocks: &[MarkdownNode<MarkdownBlock>]) -> Vec<AnyElement> {
    blocks.iter().map(render_block).collect()
}

fn render_block(node: &MarkdownNode<MarkdownBlock>) -> AnyElement {
    let m = metrics();
    let t = tokens();
    match &node.value {
        MarkdownBlock::Paragraph(content) => text(TextRole::Body)
            .w_full()
            .min_w_0()
            .text_color(t.fg_rgba())
            .child(styled_inline(content, false))
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
                .child(styled_inline(content, true))
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
                    .children(render_blocks(blocks)),
            )
            .into_any_element(),
        MarkdownBlock::List(list) => render_list(list),
        MarkdownBlock::CodeBlock { language, code } => render_code(language.as_deref(), code),
        MarkdownBlock::Table(table) => render_table(table),
        MarkdownBlock::ThematicBreak => div()
            .w_full()
            .h(px(1.))
            .my(m.space_xs)
            .bg(t.border_rgba())
            .into_any_element(),
    }
}

fn render_list(list: &MarkdownList) -> AnyElement {
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
                        .child(marker),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(m.space_sm)
                        .children(render_blocks(&item.blocks)),
                ),
        );
    }
    root.into_any_element()
}

fn render_code(language: Option<&str>, code: &str) -> AnyElement {
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
            .child(code.to_owned()),
    )
    .into_any_element()
}
