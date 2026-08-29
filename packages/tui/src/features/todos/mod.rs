//! Agent todo list overlay (F-27 / TUI todo-list feature).
//!
//! Live checklist truth for the viewed agent, opened explicitly with `/todo`.

mod project;
mod render;
mod state;

#[allow(unused_imports)]
pub use project::{TodoStripView, max_item_rows_for_grant, project_strip};
pub use state::TodoListsState;

use piko_tui_layout::{Component, SurfacePanel};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{AppState, HitId},
    navigation::SurfaceId,
    theme::Theme,
    ui::components::pane::{PaneSpec, render_pane},
};

pub(crate) const WHEEL_STEP: usize = 3;
pub(crate) const TODO_MAX_ITEM_ROWS: u16 = 16;

pub struct TodoPanel;

pub struct TodoCtx<'a> {
    pub app: &'a AppState,
    pub theme: &'a Theme,
    pub hints: Option<&'a str>,
}

impl Component<HitId, TodoCtx<'_>> for TodoPanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &TodoCtx<'_>) {
        let spec = PaneSpec::new("todos")
            .hints(ctx.hints.unwrap_or_default())
            .focused(true);
        let Some(areas) = render_pane(frame, area, &spec, ctx.theme) else {
            return;
        };
        let Some(list) = ctx.app.todo_lists.viewed_list(
            ctx.app.agent_panel.active_agent_instance_id.as_deref(),
            ctx.app.todo_feature_enabled(),
        ) else {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "No todos for the viewed agent.",
                    Style::default().fg(ctx.theme.dim),
                ))),
                areas.content,
            );
            return;
        };
        let max_scroll = list.items.len().saturating_sub(max_item_rows_for_grant(
            areas.content.height,
            list.items.len(),
        ));
        ctx.app.todo_lists.set_max_scroll(max_scroll);
        render::paint_overlay(
            frame,
            areas.content,
            list,
            ctx.app.todo_lists.scroll_offset(),
            ctx.theme,
        );
    }

    fn component_regions(&self, _area: Rect) -> Vec<(Rect, HitId)> {
        Vec::new()
    }
}

impl SurfacePanel<SurfaceId, HitId, TodoCtx<'_>> for TodoPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Todos
    }
}
