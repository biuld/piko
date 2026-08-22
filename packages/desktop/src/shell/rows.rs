//! Timeline row presentation: user chips and assistant turn flows
//! (F-44 / F-46). Activity details open through the chip-detail overlay.

use super::canvas::{toggle_expand, user_pref_open};
use super::*;
use crate::focus::{ChipDetail, LayerKind};
use gpui::{Entity, Window, relative};
use island::components::activity_chip::{ActivityChip, ActivityStatus};
use island::components::conversation::{
    BlockAlign, BlockSurface, CollapsePolicy, ConversationBlock, user_body_max_height,
};
use island::components::markdown::{MarkdownRenderOptions, parse_markdown, render_markdown_with};
use island::components::selection::SelectionState;
use island::theme::{IslandIcon, metrics};
use piko_client_core::timeline::ToolStatus;

/// Fixed width so streaming growth never jitters the thinking capsule.
const THINKING_CHIP_WIDTH: f32 = 84.0;
const TOOL_CHIP_MAX_WIDTH: f32 = 280.0;

impl Shell {
    pub(super) fn render_timeline_row(
        &mut self,
        row: &timeline::TimelineRow,
        gap_before: gpui::Pixels,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = row.id().to_string();
        let owner = cx.entity().entity_id();
        let entity = cx.entity().downgrade();
        let material = self.material;
        let selection = self.selection_for(&id, cx);

        let block = match row {
            timeline::TimelineRow::User { text, .. } => {
                let expanded = self
                    .view_local()
                    .block_expand
                    .get(&id)
                    .copied()
                    .map(user_pref_open)
                    .unwrap_or(false);
                let doc = self.cached_markdown(&id, text);
                let toggle = {
                    let entity = entity.clone();
                    let id = id.clone();
                    move |window: &mut Window, app: &mut App| {
                        if let Some(shell) = entity.upgrade() {
                            shell.update(app, |shell, cx| {
                                let pref = shell
                                    .view_local()
                                    .block_expand
                                    .get(&id)
                                    .copied()
                                    .unwrap_or_default();
                                let open = user_pref_open(pref);
                                shell
                                    .view_local()
                                    .block_expand
                                    .insert(id.clone(), toggle_expand(pref, open));
                                let _ = window;
                                cx.notify();
                            });
                        }
                    }
                };
                ConversationBlock::new(id.clone())
                    .align(BlockAlign::Trailing)
                    .surface(BlockSurface::ElevatedChip)
                    .material(material)
                    .collapse(CollapsePolicy::IfOverflow {
                        max_height: user_body_max_height(),
                    })
                    .expanded(expanded)
                    .selection(selection.clone())
                    .plaintext(text.clone())
                    .notify_owner(owner)
                    .on_toggle(toggle)
                    .extra_menu(quote_menu(entity))
                    .body(render_markdown_with(
                        id.clone(),
                        &doc,
                        Some(selection),
                        MarkdownRenderOptions::default(),
                    ))
                    .into_any_element()
            }
            timeline::TimelineRow::Assistant { segments, .. } => {
                let mut caret_tail = false;
                let mut flow: Vec<AnyElement> = Vec::new();
                let mut chips: Vec<AnyElement> = Vec::new();
                let m = metrics();
                for segment in segments {
                    match segment {
                        timeline::TurnSegment::Thinking { id, active, .. } => {
                            let detail = ChipDetail::thinking_for(segment);
                            chips.push(
                                ActivityChip::new(id.clone(), IslandIcon::Brain, "Thinking")
                                    .status(if *active {
                                        ActivityStatus::Active
                                    } else {
                                        ActivityStatus::Done
                                    })
                                    .material(material)
                                    .fixed_width(px(THINKING_CHIP_WIDTH))
                                    .on_click(chip_detail_opener(entity.clone(), detail))
                                    .into_any_element(),
                            );
                        }
                        timeline::TurnSegment::Tool { id, name, status } => {
                            let (activity, icon) = match status {
                                ToolStatus::Running => (ActivityStatus::Active, IslandIcon::Wrench),
                                ToolStatus::Completed => (ActivityStatus::Done, IslandIcon::Wrench),
                                ToolStatus::Failed => {
                                    (ActivityStatus::Failed, IslandIcon::TriangleAlert)
                                }
                                ToolStatus::Cancelled => {
                                    (ActivityStatus::Cancelled, IslandIcon::CircleStop)
                                }
                            };
                            let label = tool_chip_label(&self.state.core, name, id);
                            chips.push(
                                ActivityChip::new(id.clone(), icon, label)
                                    .status(activity)
                                    .material(material)
                                    .max_width(px(TOOL_CHIP_MAX_WIDTH))
                                    .on_click(chip_detail_opener(
                                        entity.clone(),
                                        ChipDetail::Tool {
                                            call_id: id.clone(),
                                            name: name.clone(),
                                            status: *status,
                                        },
                                    ))
                                    .into_any_element(),
                            );
                        }
                        timeline::TurnSegment::Text { id, text, caret } => {
                            if !chips.is_empty() {
                                flow.push(flush_chip_run(chips.drain(..), m.space_xs));
                            }
                            caret_tail |= *caret;
                            let doc = self.cached_markdown(id, text);
                            flow.push(render_markdown_with(
                                id.clone(),
                                &doc,
                                Some(selection.clone()),
                                MarkdownRenderOptions {
                                    caret: *caret,
                                    ..Default::default()
                                },
                            ));
                        }
                    }
                }
                if !chips.is_empty() {
                    flow.push(flush_chip_run(chips.drain(..), m.space_xs));
                }
                let plaintext = segments
                    .iter()
                    .filter_map(|segment| segment.text())
                    .collect::<Vec<_>>()
                    .join("\n");
                ConversationBlock::new(id.clone())
                    .align(BlockAlign::Leading)
                    .leading_icon(IslandIcon::Bot, tokens().fg_rgba())
                    .surface(BlockSurface::ElevatedChip)
                    .material(material)
                    .collapse(CollapsePolicy::Never)
                    .expanded(true)
                    .streaming(caret_tail)
                    .selection(selection.clone())
                    .plaintext(plaintext)
                    .notify_owner(owner)
                    .extra_menu(quote_menu(entity))
                    .body(
                        div()
                            .flex()
                            .flex_col()
                            .gap(m.space_sm)
                            .min_w_0()
                            .children(flow),
                    )
                    .into_any_element()
            }
            timeline::TimelineRow::System { label, .. } => ConversationBlock::new(id.clone())
                .align(BlockAlign::Center)
                .hug_max(relative(1.0))
                .surface(BlockSurface::None)
                .material(material)
                .collapse(CollapsePolicy::Never)
                .expanded(true)
                .selection(selection)
                .plaintext(label.clone())
                .notify_owner(owner)
                .body(
                    text(TextRole::Meta)
                        .text_color(tokens().muted_fg_rgba())
                        .child(label.clone()),
                )
                .into_any_element(),
        };

        div()
            .id(gpui::SharedString::from(id))
            .mt(gap_before)
            .child(block)
            .into_any_element()
    }

    fn cached_markdown(
        &mut self,
        id: &str,
        source: &str,
    ) -> island::components::markdown::MarkdownDocument {
        if let Some((prev, doc)) = self.markdown_cache.get(id)
            && prev == source
        {
            return doc.clone();
        }
        let doc = parse_markdown(source);
        self.markdown_cache
            .insert(id.to_string(), (source.to_string(), doc.clone()));
        doc
    }

    fn selection_for(&mut self, id: &str, cx: &mut App) -> Entity<SelectionState> {
        if let Some(existing) = self.selections.get(id) {
            return existing.clone();
        }
        let group = self.selection_group.clone();
        let owned = id.to_string();
        let entity = cx.new(|cx| SelectionState::new(owned, group, cx));
        self.selections.insert(id.to_string(), entity.clone());
        entity
    }

    /// Structured tool body for the detail overlay; resolved from the live
    /// projection so an opened card never shows stale payloads.
    pub(super) fn overlay_tool_sections(
        &self,
        call_id: &str,
    ) -> Option<Vec<(String, tool_body::ToolBodyKind)>> {
        let tool = timeline::find_tool(&self.state.core, call_id)?;
        Some(tool_body::format_tool_body(
            &tool.tool_name,
            tool.status,
            &tool.args,
            tool.result.as_ref(),
            &timeline::tool_result_text(tool),
            tool.partial_json.as_deref(),
        ))
    }
}

fn tool_chip_label(core: &piko_client_core::ClientState, name: &str, call_id: &str) -> String {
    match timeline::find_tool(core, call_id) {
        Some(tool) => {
            tool_body::tool_primary_line(&tool.tool_name, &tool.args, tool.partial_json.as_deref())
                .unwrap_or_else(|| tool.tool_name.clone())
        }
        None => name.to_string(),
    }
}

fn chip_detail_opener(
    entity: gpui::WeakEntity<Shell>,
    detail: ChipDetail,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    move |_, _window, app| {
        if let Some(shell) = entity.upgrade() {
            let detail = detail.clone();
            shell.update(app, |shell, cx| {
                shell.chip_detail = Some(detail);
                shell.open_layer(LayerKind::ChipDetail, FocusOwner::Timeline, cx);
            });
        }
    }
}

fn flush_chip_run(chips: impl Iterator<Item = AnyElement>, gap: gpui::Pixels) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(gap)
        .min_w_0()
        .children(chips)
        .into_any_element()
}

impl ChipDetail {
    fn thinking_for(segment: &timeline::TurnSegment) -> Self {
        match segment {
            timeline::TurnSegment::Thinking { id, text, .. } => Self::Thinking {
                segment_id: id.clone(),
                text: text.clone(),
            },
            _ => Self::Thinking {
                segment_id: String::new(),
                text: String::new(),
            },
        }
    }
}

fn quote_menu(
    entity: gpui::WeakEntity<Shell>,
) -> impl Fn(
    &island::components::selection::SelectableMenuContext,
    &mut Window,
    &mut App,
) -> Vec<island::components::menu::ContextMenuItem>
+ 'static {
    move |ctx, _, _| {
        let text = ctx.menu_text.clone();
        let entity = entity.clone();
        vec![island::components::menu::ContextMenuItem::action(
            "Quote",
            move |window, app| {
                if let Some(shell) = entity.upgrade() {
                    let text = text.clone();
                    shell.update(app, |shell, cx| {
                        shell.quote_into_composer(text, window, cx);
                    });
                }
            },
        )]
    }
}
