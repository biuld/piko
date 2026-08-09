//! Product composition: shell + plane + modal stack via `piko-tui-layout`.

use crate::{
    app::{AppState, HitId},
    navigation::{SelectBandBudget, compose_modals, compose_plane},
};
use piko_tui_layout::{
    FramePlan, HitMap, HitRegion, ShellChrome, ShellSplit, SurfacePanel, build_hitmap,
    cells_from_percent, solve, split_shell,
};

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
    app.modal_surface()
}

pub fn plane_metrics(app: &AppState, body: ratatui::layout::Rect) -> PlaneMetrics {
    let modal = resolve_modal_surface(app);
    let suggest = has_visible_suggestions(app) && modal.is_none();
    let centered_size = match modal {
        Some(SurfaceId::Settings) => Some(settings_centered_size(app, body)),
        Some(SurfaceId::Status) => Some(status_centered_size(app, body)),
        _ => None,
    };

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
        centered_size,
    }
}

/// Centered size for the compact read-only status dialog.
fn status_centered_size(app: &AppState, body: ratatui::layout::Rect) -> (u16, u16) {
    let preview_rows = u16::from(app.queue_status.steer_preview.is_some())
        .saturating_add(u16::from(app.queue_status.follow_up_preview.is_some()))
        .saturating_mul(3);
    let content_rows = 6u16.saturating_add(preview_rows);
    let width = cells_from_percent(body.width, 76)
        .clamp(40, 96)
        .min(body.width);
    let height = content_rows
        .saturating_add(5)
        .min(body.height.saturating_sub(2));
    (width, height)
}

/// Centered size for the settings dialog: width 88% of the body, height from
/// the menu content budget, capped below the body so it never fills the frame.
fn settings_centered_size(app: &AppState, body: ratatui::layout::Rect) -> (u16, u16) {
    let budget = app.settings.select_band_budget();
    let width = cells_from_percent(body.width, 88).max(40).min(body.width);
    let height = budget
        .preferred_band_rows()
        .min(body.height.saturating_sub(2));
    (width, height)
}

/// Feature-declared content-row budget for Select / ComposerBand only.
fn select_band_budget(app: &AppState, surface: SurfaceId) -> Option<SelectBandBudget> {
    use crate::navigation::SurfaceIntent;
    if !matches!(
        surface.intent(),
        SurfaceIntent::Select | SurfaceIntent::Dock
    ) {
        return None;
    }
    Some(match surface {
        SurfaceId::Models => app.models.select_band_budget(),
        SurfaceId::Mcp => app.mcp.select_band_budget(),
        SurfaceId::Processes => app.processes.select_band_budget(),
        SurfaceId::Thinking => app.thinking.select_band_budget(),
        SurfaceId::Agents => app.agent_panel.select_band_budget(),
        SurfaceId::AuthSelector => app.auth_selector.select_band_budget(),
        // Tool interaction replaces the composer: the dock is the workflow
        // body + Standard pane chrome.
        SurfaceId::ToolInteraction => {
            let rows = app
                .interactions
                .front()
                .map(|i| i.workflow.dock_content_rows(&app.theme))
                .unwrap_or(3);
            SelectBandBudget::standard_info(rows)
        }
        SurfaceId::Approval => {
            let rows = app
                .approvals
                .workflow()
                .map(|w| w.dock_content_rows(&app.theme))
                .unwrap_or(7);
            SelectBandBudget::standard_info(rows)
        }
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

/// Build the per-frame hit map for the current composition: plane regions at
/// `z = 0`, modal layers above. Surfaces that implement [`SurfacePanel`]
/// contribute element hit regions; the rest are non-interactive for now.
#[allow(dead_code)] // exercised by layout tests; consumed by pointer-input PRD
pub fn build_surface_hitmap(
    app: &AppState,
    terminal: ratatui::layout::Rect,
) -> HitMap<Region, HitId> {
    let composed = compose_frame(app, terminal);
    let stamp = |hrs: Vec<HitRegion<SurfaceId, HitId>>| -> Vec<HitRegion<Region, HitId>> {
        hrs.into_iter()
            .map(|hr| HitRegion {
                region: Region::Surface(hr.region),
                rect: hr.rect,
                element: hr.element,
            })
            .collect()
    };
    build_hitmap(&composed.plan, |region, rect| match region {
        Region::Stream => vec![HitRegion {
            region: Region::Stream,
            rect,
            element: Some(HitId::Stream),
        }],
        Region::Notice => vec![HitRegion {
            region: Region::Notice,
            rect,
            element: Some(HitId::Notice),
        }],
        Region::Suggest => (0..app.editor.auto_complete.len().min(6))
            .map(|i| HitRegion {
                region: Region::Suggest,
                rect: ratatui::layout::Rect::new(
                    rect.x,
                    rect.y.saturating_add(1).saturating_add(i as u16),
                    rect.width,
                    1,
                ),
                element: Some(HitId::Suggest(i)),
            })
            .collect(),
        Region::Composer => vec![HitRegion {
            region: Region::Composer,
            rect,
            element: Some(HitId::Composer),
        }],
        Region::Surface(SurfaceId::Agents) => stamp(app.agent_panel.hit_regions(rect)),
        Region::Surface(SurfaceId::Sessions) => stamp(app.sessions.hit_regions(rect)),
        Region::Surface(SurfaceId::Tree) | Region::Surface(SurfaceId::SummaryPrompt) => {
            stamp(app.tree.hit_regions(rect))
        }
        Region::Surface(SurfaceId::Status) => {
            stamp(<crate::features::status::StatusPanel as SurfacePanel<
                SurfaceId,
                HitId,
                crate::features::status::StatusCtx<'_>,
            >>::hit_regions(
                &crate::features::status::StatusPanel, rect
            ))
        }
        Region::Surface(SurfaceId::Diagnostics) => stamp(app.diagnostics.hit_regions(rect)),
        Region::Surface(SurfaceId::Settings) => stamp(app.settings.hit_regions(rect)),
        Region::Surface(SurfaceId::Models) => stamp(app.models.hit_regions(rect)),
        Region::Surface(SurfaceId::Thinking) => stamp(app.thinking.hit_regions(rect)),
        Region::Surface(SurfaceId::Approval) => stamp(app.approvals.hit_regions(rect)),
        Region::Surface(SurfaceId::ToolInteraction) => stamp(app.interactions.hit_regions(rect)),
        Region::Surface(SurfaceId::AuthSelector) => stamp(app.auth_selector.hit_regions(rect)),
        Region::Surface(SurfaceId::Mcp) => stamp(app.mcp.hit_regions(rect)),
        Region::Surface(SurfaceId::Processes) => stamp(app.processes.hit_regions(rect)),
    })
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
    fn approval_modal_hitmap_has_z_order_and_choice_rows() {
        use crate::features::approval::PendingApproval;

        let mut app = app_state();
        app.session.id = Some("s1".into());
        app.approvals.push(PendingApproval {
            id: "a1".into(),
            agent_instance_id: "agent-1".into(),
            tool_name: "bash".into(),
            args: serde_json::json!({ "command": "cargo test" }),
            prompt: None,
        });
        app.push_surface(SurfaceId::Approval);

        let terminal = Rect::new(0, 0, 80, 24);
        let composed = compose_frame(&app, terminal);
        let host = composed
            .plan
            .layers
            .first()
            .and_then(|layer| layer.rects.get(&Region::Surface(SurfaceId::Approval)))
            .copied()
            .expect("approval layer host");
        let map = build_surface_hitmap(&app, terminal);
        assert!(!map.hits.is_empty());

        // Standard pane chrome: border(1) + padding(1) → content.y = host.y+2;
        // single-question layout → prompt(0), blank(1), choices at +2.
        let hit = map
            .hit_test(host.x.saturating_add(2), host.y.saturating_add(4))
            .expect("choice row cell");
        assert_eq!(hit.region, Region::Surface(SurfaceId::Approval));
        assert_eq!(hit.z, 1);
        assert!(matches!(
            hit.element,
            Some(HitId::Choice {
                question: 0,
                choice: 0
            })
        ));

        // Below the choices the surface-default entry still owns the cell —
        // never a fall-through to the plane.
        let bottom = map
            .hit_test(
                host.x.saturating_add(2),
                host.y.saturating_add(host.height.saturating_sub(1)),
            )
            .expect("surface default cell");
        assert_eq!(bottom.region, Region::Surface(SurfaceId::Approval));
        assert_eq!(bottom.element, None);
    }

    #[test]
    fn approval_dock_height_follows_workflow_content() {
        use crate::features::approval::PendingApproval;

        let mut app = app_state();
        app.session.id = Some("s1".into());
        app.approvals.push(PendingApproval {
            id: "a1".into(),
            agent_instance_id: "agent-1".into(),
            tool_name: "bash".into(),
            args: serde_json::json!({ "command": "cargo test" }),
            prompt: None,
        });
        app.push_surface(SurfaceId::Approval);

        let terminal = Rect::new(0, 0, 80, 24);
        let composed = compose_frame(&app, terminal);
        let host = composed
            .plan
            .layers
            .first()
            .and_then(|layer| layer.rects.get(&Region::Surface(SurfaceId::Approval)))
            .copied()
            .expect("approval layer host");
        let rows = app
            .approvals
            .workflow()
            .unwrap()
            .dock_content_rows(&app.theme);
        // Dock = workflow rows + Standard pane chrome (5), bottom-anchored.
        assert_eq!(host.height, rows + 5);
        assert_eq!(host.y + host.height, 23);
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

    #[test]
    fn status_surface_uses_content_sized_centered_modal() {
        let mut app = app_state();
        app.push_surface(SurfaceId::Status);
        let frame = compose_frame(&app, Rect::new(0, 0, 100, 30));
        let layer = frame.plan.layers.first().expect("status layer");
        assert!(matches!(
            layer.placement,
            piko_tui_layout::ModalPlacement::Centered {
                max_width: 76,
                max_height: 11
            }
        ));
        let status = layer
            .rects
            .get(&Region::Surface(SurfaceId::Status))
            .expect("status rect");
        assert_eq!((status.width, status.height), (76, 11));
    }
}
