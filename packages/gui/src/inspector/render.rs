//! Inspector panel and sheet rendering.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::resizable::{resizable_panel, v_resizable};
use gpui_component::scroll::ScrollableElement;

use crate::app::layout_state::InspectorTab;
use crate::theme::{RoleAccent, island, metrics, tokens};
use crate::workbench::AgentTreeViewModel;
use crate::workbench::render_agent_tree_panel;
use crate::workbench::tree_render::tree_guides;

use super::{ConversationMapViewModel, MapEntryKind, MapNode};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type IdClickFactory = Box<dyn Fn(String) -> ClickHandler>;

pub struct InspectorHandlers {
    pub on_select_agent: IdClickFactory,
    pub on_map_activate: IdClickFactory,
    pub on_map_toggle_expand: IdClickFactory,
    pub on_switch_branch: ClickHandler,
}

pub struct InspectorSheetHandlers {
    pub on_tab_agents: ClickHandler,
    pub on_tab_map: ClickHandler,
    pub panel: InspectorHandlers,
}

pub fn render_inspector_panel(
    agent_tree: &AgentTreeViewModel,
    map: &ConversationMapViewModel,
    _inspector_width: f32,
    handlers: InspectorHandlers,
) -> impl IntoElement {
    let m = metrics();
    let InspectorHandlers {
        on_select_agent,
        on_map_activate,
        on_map_toggle_expand,
        on_switch_branch,
    } = handlers;

    div()
        .id("inspector-panel")
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div().size_full().child(
                v_resizable("inspector-split")
                    .child(
                        resizable_panel()
                            .size(px(220.))
                            .size_range(px(160.)..px(2000.))
                            .child(vertical_island_slot(
                                render_agent_tree_panel(agent_tree, move |id| on_select_agent(id)),
                                px(0.),
                                m.space_xs,
                            )),
                    )
                    .child(resizable_panel().size_range(px(180.)..px(2000.)).child(
                        vertical_island_slot(
                            render_map_section(
                                map,
                                on_map_activate,
                                on_map_toggle_expand,
                                on_switch_branch,
                            ),
                            m.space_xs,
                            px(0.),
                        ),
                    )),
            ),
        )
}

fn vertical_island_slot(
    child: impl IntoElement,
    top_padding: Pixels,
    bottom_padding: Pixels,
) -> impl IntoElement {
    div()
        .size_full()
        .pt(top_padding)
        .pb(bottom_padding)
        .child(child)
}

pub fn render_inspector_sheet_body(
    tab: InspectorTab,
    agent_tree: &AgentTreeViewModel,
    map: &ConversationMapViewModel,
    handlers: InspectorSheetHandlers,
) -> impl IntoElement {
    let m = metrics();
    let InspectorSheetHandlers {
        on_tab_agents,
        on_tab_map,
        panel,
    } = handlers;
    let InspectorHandlers {
        on_select_agent,
        on_map_activate,
        on_map_toggle_expand,
        on_switch_branch,
    } = panel;

    div()
        .id("inspector-sheet")
        .size_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .gap(m.space_sm)
                .p(m.space_sm)
                .border_b_1()
                .border_color(tokens().border_rgba())
                .child(
                    Button::new("insp-tab-agents")
                        .label("Agents")
                        .when(tab == InspectorTab::Agents, |b| b.primary())
                        .on_click(move |ev, w, cx| on_tab_agents(ev, w, cx)),
                )
                .child(
                    Button::new("insp-tab-map")
                        .label("Map")
                        .when(tab == InspectorTab::Map, |b| b.primary())
                        .on_click(move |ev, w, cx| on_tab_map(ev, w, cx)),
                ),
        )
        .child(match tab {
            InspectorTab::Agents => {
                render_agent_tree_panel(agent_tree, move |id| on_select_agent(id))
                    .into_any_element()
            }
            InspectorTab::Map => {
                render_map_section(map, on_map_activate, on_map_toggle_expand, on_switch_branch)
                    .into_any_element()
            }
        })
}

fn render_map_section(
    map: &ConversationMapViewModel,
    on_map_activate: IdClickFactory,
    on_map_toggle_expand: IdClickFactory,
    on_switch_branch: ClickHandler,
) -> impl IntoElement {
    let m = metrics();
    island()
        .id("conversation-map")
        .size_full()
        .flex()
        .flex_col()
        .child(
            div()
                .h(m.panel_header_height)
                .px(m.space_md)
                .flex()
                .items_center()
                .text_size(m.label_size)
                .line_height(m.label_line_height)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Conversation Map"),
        )
        .child(
            div()
                .flex_1()
                .overflow_y_scrollbar()
                .p(m.space_sm)
                .children(map.nodes.iter().enumerate().map(|(ix, node)| {
                    let activate = on_map_activate(node.id.clone());
                    let toggle = on_map_toggle_expand(node.id.clone());
                    let previewed = map.preview_entry_id.as_deref() == Some(node.id.as_str());
                    render_map_node(ix, node, previewed, activate, toggle)
                })),
        )
        .when(map.can_switch_branch, |d| {
            d.child(
                div()
                    .p(m.space_sm)
                    .border_t_1()
                    .border_color(tokens().border_rgba())
                    .child(
                        Button::new("switch-branch")
                            .primary()
                            .label("Switch Branch")
                            .w_full()
                            .on_click(move |ev, w, cx| on_switch_branch(ev, w, cx)),
                    ),
            )
        })
}

fn render_map_node(
    ix: usize,
    node: &MapNode,
    previewed: bool,
    on_click: ClickHandler,
    on_toggle: ClickHandler,
) -> impl IntoElement {
    let m = metrics();
    let kind = match node.kind {
        MapEntryKind::Message => "M",
        MapEntryKind::Tool => "T",
        MapEntryKind::System => "S",
        MapEntryKind::Other => "·",
    };
    let accent = if node.is_leaf {
        tokens().role_accent(RoleAccent::Accent)
    } else if previewed {
        tokens().role_accent(RoleAccent::Warning)
    } else if node.on_path {
        tokens().fg_rgba()
    } else {
        tokens().muted_fg_rgba()
    };
    let chevron = if node.expanded { "▾" } else { "▸" };
    let toggle_id = SharedString::from(format!("map-exp-{ix}"));

    div()
        .id(SharedString::from(format!("map-tree-{ix}")))
        .h(px(32.))
        .w_full()
        .px(m.space_sm)
        .flex()
        .items_center()
        .gap(m.space_xs)
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(tokens().elevated_rgba()))
        .when(node.is_leaf || previewed, |d| {
            d.bg(tokens().elevated_rgba())
        })
        .children(tree_guides(node.depth))
        .child(
            div()
                .w(px(16.))
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(if node.has_children {
                    div()
                        .id(toggle_id)
                        .text_size(m.meta_size)
                        .text_color(tokens().muted_fg_rgba())
                        .cursor_pointer()
                        .on_click(move |ev, w, cx| {
                            cx.stop_propagation();
                            on_toggle(ev, w, cx);
                        })
                        .child(chevron)
                        .into_any_element()
                } else {
                    div()
                        .text_size(m.meta_size)
                        .text_color(tokens().muted_fg_rgba())
                        .child("•")
                        .into_any_element()
                }),
        )
        .child(
            div()
                .w(px(16.))
                .flex_shrink_0()
                .text_center()
                .text_size(m.meta_size)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(accent)
                .child(kind),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(m.label_size)
                .line_height(m.label_line_height)
                .text_color(accent)
                .when(node.is_leaf || previewed, |d| {
                    d.font_weight(FontWeight::SEMIBOLD)
                })
                .child(node.label.clone()),
        )
        .when(node.on_path, |d| {
            d.child(
                div()
                    .flex_shrink_0()
                    .text_size(m.meta_size)
                    .text_color(tokens().muted_fg_rgba())
                    .child("path"),
            )
        })
        .on_click(move |ev, window, cx| on_click(ev, window, cx))
}
