//! Product composition: shell + plane + modal stack via `piko-tui-layout`.

use crate::{
    app::AppState,
    navigation::{FocusManagerExt, SelectBandBudget, compose_modals, compose_plane},
};
use piko_tui_layout::{FramePlan, ShellChrome, ShellSplit, solve, split_shell};

pub use crate::navigation::{PlaneMetrics, Region, SurfaceId};
pub use piko_tui_layout::{DEFAULT_HORIZONTAL_INSET, inset_horizontal};

pub const SHELL_CHROME: ShellChrome = ShellChrome::Bottom { height: 1 };

#[derive(Clone, Debug)]
pub struct ProductFrame {
    pub modal_surface: Option<SurfaceId>,
    pub shell: ShellSplit,
    pub plan: FramePlan<Region>,
}

pub fn resolve_modal_surface(app: &AppState) -> Option<SurfaceId> {
    if !app.approvals.is_empty() || app.focus_manager.active_surface() == Some(SurfaceId::Approval)
    {
        return Some(SurfaceId::Approval);
    }
    if !app.interactions.is_empty()
        || app.focus_manager.active_surface() == Some(SurfaceId::ToolInteraction)
    {
        return Some(SurfaceId::ToolInteraction);
    }
    app.focus_manager.active().as_surface()
}

pub fn plane_metrics(app: &AppState, body: ratatui::layout::Rect) -> PlaneMetrics {
    let modal = resolve_modal_surface(app);
    let suggest = has_visible_suggestions(app) && modal.is_none();

    PlaneMetrics {
        notice: app.notifications.has_visible(),
        suggest,
        suggestion_count: if suggest {
            app.editor.auto_complete.len()
        } else {
            0
        },
        composer_height: app
            .editor
            .visible_height(&app.tui_config.editor, body.width),
        body_height: body.height,
        select_band: modal.and_then(|s| select_band_budget(app, s)),
    }
}

/// Feature-declared content-row budget for Select / ComposerBand only.
fn select_band_budget(app: &AppState, surface: SurfaceId) -> Option<SelectBandBudget> {
    use crate::navigation::SurfaceIntent;
    if surface.intent() != SurfaceIntent::Select {
        return None;
    }
    Some(match surface {
        SurfaceId::Models => app.models.select_band_budget(),
        SurfaceId::Agents => app.agent_panel.select_band_budget(),
        SurfaceId::AuthSelector => app.auth_selector.select_band_budget(),
        SurfaceId::Mcp => app.mcp.select_band_budget(),
        _ => SelectBandBudget::minimal_stacked_list(0),
    })
}

pub fn compose_frame(app: &AppState, terminal: ratatui::layout::Rect) -> ProductFrame {
    let shell = split_shell(terminal, SHELL_CHROME);
    let modal_surface = resolve_modal_surface(app);
    let metrics = plane_metrics(app, shell.body);
    let plane = compose_plane(metrics);
    let modals = compose_modals(modal_surface, metrics, shell.body);
    let plan = solve(shell.body, &plane, &modals);
    ProductFrame {
        modal_surface,
        shell,
        plan,
    }
}

pub fn has_visible_suggestions(app: &AppState) -> bool {
    app.mode.is_editor_base() && app.editor.auto_complete.is_active()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, InitialOptions};
    use ratatui::layout::Rect;
    use std::path::PathBuf;

    fn app_state() -> AppState {
        AppState::new(
            PathBuf::from("/tmp/piko-layout-test"),
            None,
            false,
            InitialOptions::default(),
        )
    }

    #[test]
    fn idle_workspace_only() {
        let frame = compose_frame(&app_state(), Rect::new(0, 0, 80, 24));
        assert_eq!(frame.modal_surface, None);
        assert!(frame.plan.rects.contains_key(&Region::Stream));
        assert!(frame.plan.rects.contains_key(&Region::Composer));
        assert_eq!(frame.shell.chrome.height, 1);
    }

    #[test]
    fn agents_surface_uses_composer_band() {
        let mut app = app_state();
        app.push_surface(SurfaceId::Agents);
        let frame = compose_frame(&app, Rect::new(0, 0, 80, 24));
        assert_eq!(frame.modal_surface, Some(SurfaceId::Agents));
        assert_eq!(frame.plan.layers.len(), 1);
        // Stream remains visible under the select band (not CoverBody).
        assert!(frame.plan.rects.contains_key(&Region::Stream));
    }
}
