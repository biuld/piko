//! Agent todo list dock strip (F-27 / TUI todo-list feature).
//!
//! Live checklist truth for the viewed agent. Height is granted only via
//! Dock Stack offers — this module never allocates a plane sibling itself.

mod project;
mod render;
mod state;

pub use project::strip_height_offer;
#[allow(unused_imports)] // public API + tests
pub use project::{TodoStripView, project_strip};
pub use state::TodoListsState;

use crate::app::AppState;
use crate::features::dock_stack::{BandId, DockBandOffer, TODOS_MIN_HEIGHT};

/// Build a Dock Stack offer for the Todos band (or inactive if hidden).
pub fn dock_band_offer(app: &AppState) -> Option<DockBandOffer> {
    let list = app.todo_lists.viewed_list(
        app.agent_panel.active_agent_instance_id.as_deref(),
        app.todo_feature_enabled(),
    )?;
    let preferred = strip_height_offer(list);
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
) {
    let Some(list) = app.todo_lists.viewed_list(
        app.agent_panel.active_agent_instance_id.as_deref(),
        app.todo_feature_enabled(),
    ) else {
        return;
    };
    render::paint_strip(frame, area, list, theme);
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
        assert_eq!(strip_height_offer(&list), 0);
    }

    #[test]
    fn few_items_header_plus_rows() {
        let list = sample_list(3);
        // header + 3 items, no overflow
        assert_eq!(strip_height_offer(&list), 4);
    }

    #[test]
    fn over_cap_adds_overflow_row() {
        let list = sample_list(10);
        // header + 6 items + overflow
        assert_eq!(strip_height_offer(&list), 8);
    }

    #[test]
    fn project_strip_counts_and_marks() {
        let list = sample_list(3);
        let view = project_strip(&list, 80, 6);
        assert!(view.header.contains("Todos"));
        assert!(view.header.contains("1/3 done"));
        assert_eq!(view.rows.len(), 3);
        assert!(view.overflow.is_none());
    }

    #[test]
    fn project_strip_overflow() {
        let list = sample_list(8);
        let view = project_strip(&list, 80, 3);
        assert_eq!(view.rows.len(), 3);
        assert_eq!(view.overflow.as_deref(), Some("+5 more"));
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
    fn paint_respects_grant_height() {
        let list = sample_list(10);
        let grant_h = 3u16; // header + 1 item + maybe overflow forced by grant
        let view = project_strip(&list, 40, grant_h.saturating_sub(1).max(1) as usize);
        // projector never emits more content rows than max_item_rows
        assert!(view.rows.len() <= grant_h.saturating_sub(1) as usize);
    }
}
