//! Public document renderer.

use gpui::*;

use crate::theme::metrics;

mod blocks;
mod inline;
mod table;

use self::blocks::render_blocks;
use super::model::MarkdownDocument;

pub fn render_markdown(document: &MarkdownDocument) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(metrics().space_md)
        .children(render_blocks(&document.blocks))
        .into_any_element()
}
