//! Public document renderer.

use gpui::*;

use crate::theme::metrics;

mod blocks;
mod inline;
mod table;

use self::blocks::{SelectionRenderContext, render_blocks};
use super::model::MarkdownDocument;
use crate::components::selection::SelectionState;

pub fn render_markdown(document: &MarkdownDocument) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(metrics().space_md)
        .children(render_blocks(&document.blocks, None))
        .into_any_element()
}

pub fn render_selectable_markdown(
    id: impl Into<SharedString>,
    document: &MarkdownDocument,
    selection: Entity<SelectionState>,
) -> AnyElement {
    let selection = SelectionRenderContext::new(id.into(), selection);
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(metrics().space_md)
        .children(render_blocks(&document.blocks, Some(&selection)))
        .into_any_element()
}
