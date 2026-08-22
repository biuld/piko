//! Agent todo list dock strip (F-27 / TUI todo-list feature).
//!
//! Live checklist truth for the viewed agent. Height is granted only via
//! Dock Stack offers — this module never allocates a plane sibling itself.

mod project;
mod render;
mod state;

/// Wheel rows per gesture over the Todos strip (same step as Timeline).
pub(crate) const WHEEL_STEP: usize = 3;

pub use project::strip_height_offer;
#[allow(unused_imports)] // public API + tests
pub use project::{TodoStripView, max_item_rows_for_grant, project_strip};
pub use state::TodoListsState;

use crate::app::AppState;
use crate::features::dock_stack::{BandId, DockBandOffer, TODOS_MIN_HEIGHT};

/// Build a Dock Stack offer for the Todos band (or inactive if hidden).
pub fn dock_band_offer(app: &AppState) -> Option<DockBandOffer> {
    let list = app.todo_lists.viewed_list(
        app.agent_panel.active_agent_instance_id.as_deref(),
        app.todo_feature_enabled(),
    )?;
    let preferred = strip_height_offer(list, app.todo_lists.is_collapsed());
    if preferred == 0 {
        return None;
    }
    Some(DockBandOffer::active(
        BandId::Todos,
        preferred,
        TODOS_MIN_HEIGHT,
    ))
}

/// Paint the Todos strip inside the granted rect.
pub fn render_todos_strip(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    app: &AppState,
    theme: &crate::theme::Theme,
    interaction: piko_tui_layout::InteractionState<crate::app::HitId>,
) {
    let Some(list) = app.todo_lists.viewed_list(
        app.agent_panel.active_agent_instance_id.as_deref(),
        app.todo_feature_enabled(),
    ) else {
        return;
    };
    let collapsed = app.todo_lists.is_collapsed();
    // Record the painted viewport so wheel events clamp to what is visible.
    // A collapsed strip is never scrollable.
    let max_scroll = if collapsed {
        0
    } else {
        list.items
            .len()
            .saturating_sub(max_item_rows_for_grant(area.height, list.items.len()))
    };
    app.todo_lists.set_max_scroll(max_scroll);
    render::paint_strip(
        frame,
        area,
        list,
        collapsed,
        app.todo_lists.scroll_offset(),
        theme,
        interaction.hovered == Some(crate::app::HitId::TodosToggle),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::{TodoItem, TodoList, TodoStatus};

    fn sample_list(n: usize) -> TodoList {
        TodoList {
            agent_instance_id: "agent-a".into(),
            items: (0..n)
                .map(|i| TodoItem {
                    id: i.to_string(),
                    status: if i == 0 {
                        TodoStatus::InProgress
                    } else if i == 1 {
                        TodoStatus::Completed
                    } else {
                        TodoStatus::Pending
                    },
                    content: format!("item {i}"),
                    detail: None,
                })
                .collect(),
            updated_at: 1,
            revision: 1,
        }
    }

    #[test]
    fn empty_list_height_zero() {
        let list = TodoList {
            agent_instance_id: "a".into(),
            items: vec![],
            updated_at: 0,
            revision: 0,
        };
        assert_eq!(strip_height_offer(&list, false), 0);
    }

    #[test]
    fn few_items_header_plus_rows() {
        let list = sample_list(3);
        // header + 3 items + Dock Stack separator, no overflow
        assert_eq!(strip_height_offer(&list, false), 5);
    }

    #[test]
    fn over_cap_reserves_scroll_hint_row() {
        let list = sample_list(10);
        // header + 6 items + scroll hint row + Dock Stack separator
        assert_eq!(strip_height_offer(&list, false), 9);
    }

    #[test]
    fn collapsed_list_is_header_plus_separator() {
        let list = sample_list(10);
        assert_eq!(strip_height_offer(&list, true), TODOS_MIN_HEIGHT);
    }

    #[test]
    fn project_strip_counts_and_marks() {
        let list = sample_list(3);
        let view = project_strip(&list, 80, 6, false, 0);
        assert!(view.header.starts_with("▾ Todos"));
        assert!(view.header.contains("Todos"));
        assert!(view.header.contains("1/3 done"));
        assert_eq!(view.rows.len(), 3);
        assert!(view.scroll_hint.is_none());
    }

    #[test]
    fn project_strip_scroll_hint_points_down_at_top() {
        let list = sample_list(8);
        let view = project_strip(&list, 80, 3, false, 0);
        assert_eq!(view.rows.len(), 3);
        assert_eq!(view.scroll_hint.as_deref(), Some("↓5"));
    }

    #[test]
    fn collapsed_projection_keeps_only_summary() {
        let list = sample_list(8);
        let view = project_strip(&list, 80, 6, true, 2);
        assert!(view.header.starts_with("▸ Todos"));
        assert!(view.rows.is_empty());
        assert!(view.scroll_hint.is_none());
    }

    #[test]
    fn scroll_window_moves_and_hint_tracks_both_directions() {
        let list = sample_list(10);
        // Window of 4 rows starting at item 3: items 3..=6 visible.
        let view = project_strip(&list, 80, 4, false, 3);
        assert_eq!(view.rows.len(), 4);
        assert_eq!(view.rows[0].content, "item 3");
        assert_eq!(view.rows[3].content, "item 6");
        assert_eq!(view.scroll_hint.as_deref(), Some("↑3 · ↓3"));

        // Bottom: only items above remain.
        let bottom = project_strip(&list, 80, 4, false, 6);
        assert_eq!(bottom.rows[0].content, "item 6");
        assert_eq!(bottom.rows[3].content, "item 9");
        assert_eq!(bottom.scroll_hint.as_deref(), Some("↑6"));

        // Over-scroll clamps to the last full window.
        let clamped = project_strip(&list, 80, 4, false, 999);
        assert_eq!(clamped.rows[0].content, "item 6");
        assert_eq!(clamped.scroll_hint.as_deref(), Some("↑6"));
    }

    #[test]
    fn state_viewed_agent_switch() {
        let mut state = TodoListsState::default();
        let a = sample_list(2);
        let mut b = sample_list(0);
        b.agent_instance_id = "agent-b".into();
        state.upsert(a);
        state.upsert(b);
        assert!(state.viewed_list(Some("agent-a"), true).is_some());
        assert!(state.viewed_list(Some("agent-b"), true).is_none());
        assert!(state.viewed_list(Some("agent-a"), false).is_none());
        assert!(state.viewed_list(None, true).is_none());
    }

    #[test]
    fn clearing_session_state_also_resets_collapse() {
        let mut state = TodoListsState::default();
        state.upsert(sample_list(1));
        state.toggle_collapsed();
        state.clear();
        assert!(
            state.is_collapsed(),
            "clear() restores the collapsed default"
        );
        assert!(state.get("agent-a").is_none());
    }

    #[test]
    fn default_is_collapsed() {
        let state = TodoListsState::default();
        assert!(state.is_collapsed());
        assert_eq!(state.scroll_offset(), 0);
    }

    #[test]
    fn scroll_state_clamps_to_painted_viewport() {
        let mut state = TodoListsState::default();
        state.upsert(sample_list(10));
        state.toggle_collapsed();
        state.set_max_scroll(4);
        state.scroll_down(3);
        assert_eq!(state.scroll_offset(), 3);
        state.scroll_down(10);
        assert_eq!(state.scroll_offset(), 4, "clamped at the viewport max");
        state.scroll_up(2);
        assert_eq!(state.scroll_offset(), 2);
        state.scroll_up(10);
        assert_eq!(state.scroll_offset(), 0);
    }

    #[test]
    fn expanding_resets_scroll_and_collapsing_disables_it() {
        let mut state = TodoListsState::default();
        state.upsert(sample_list(10));
        state.toggle_collapsed(); // expand
        state.set_max_scroll(4);
        state.scroll_down(3);
        assert_eq!(state.scroll_offset(), 3);
        state.toggle_collapsed(); // collapse
        state.scroll_down(3);
        assert_eq!(state.scroll_offset(), 0, "collapsed strip never scrolls");
        state.toggle_collapsed(); // expand again → back to top
        assert_eq!(state.scroll_offset(), 0);
    }

    #[test]
    fn paint_respects_grant_height() {
        let list = sample_list(10);
        let grant_h = 3u16; // header + 1 item + maybe scroll hint forced by grant
        let view = project_strip(
            &list,
            40,
            grant_h.saturating_sub(1).max(1) as usize,
            false,
            0,
        );
        // projector never emits more content rows than max_item_rows
        assert!(view.rows.len() <= grant_h.saturating_sub(1) as usize);
    }
}
