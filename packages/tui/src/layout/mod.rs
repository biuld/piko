//! Product composition: shell + plane + modal stack via `piko-tui-layout`.
//!
//! Plane dock band heights are allocated only via Dock Stack offer → solve →
//! grant ([`crate::features::dock_stack`]).

use crate::{
    app::{AppState, HitId},
    features::dock_stack::{
        BandId, COMPOSER_MIN_HEIGHT, DOCK_BOUNDARY_HEIGHT, DockBandOffer, DockSolveInput,
        GUIDANCE_HEIGHT, SUGGEST_MIN_HEIGHT, solve, suggestion_preferred_height,
    },
    navigation::{SelectBandBudget, compose_modals, compose_plane},
};
use piko_tui_layout::{
    FramePlan, HitMap, HitRegion, ShellChrome, ShellSplit, SurfacePanel, build_hitmap,
    cells_from_percent, solve as layout_solve, split_shell,
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

/// Collect per-band offers and solve joint height under Stream floor.
pub fn plane_metrics(app: &AppState, body: ratatui::layout::Rect) -> PlaneMetrics {
    let modal = resolve_modal_surface(app);
    let centered_size = match modal {
        Some(SurfaceId::Settings) => Some(settings_centered_size(app, body)),
        Some(SurfaceId::Usage) => Some(usage_centered_size(app, body)),
        Some(SurfaceId::Notifications) => Some(notifications_centered_size(app, body)),
        _ => None,
    };

    let offers = collect_dock_offers(app, body, modal.is_some());
    let dock = solve(DockSolveInput {
        body_height: body.height,
        offers,
    });

    PlaneMetrics {
        dock,
        body_height: body.height,
        select_band: modal.and_then(|s| select_band_budget(app, s)),
        centered_size,
    }
}

/// Provider offers for the Dock Stack (domain rules stay with feature owners).
fn collect_dock_offers(
    app: &AppState,
    body: ratatui::layout::Rect,
    modal_open: bool,
) -> Vec<DockBandOffer> {
    // Todos strip: offer when projection path has a non-empty viewed list.
    let todos_offer = todos_band_offer(app);

    // Suggest forced inactive while any product modal is open.
    let suggest = has_visible_suggestions(app) && !modal_open;
    let suggest_offer = if suggest {
        let preferred = suggestion_preferred_height(app.editor.auto_complete.len());
        DockBandOffer::active(BandId::Suggest, preferred, SUGGEST_MIN_HEIGHT)
    } else {
        DockBandOffer::inactive(BandId::Suggest)
    };

    let composer_preferred = app
        .editor
        .visible_height(&app.tui_config.editor, body.width);
    let composer_offer =
        DockBandOffer::active(BandId::Composer, composer_preferred, COMPOSER_MIN_HEIGHT);

    vec![
        DockBandOffer::active(BandId::Boundary, DOCK_BOUNDARY_HEIGHT, DOCK_BOUNDARY_HEIGHT),
        todos_offer,
        suggest_offer,
        DockBandOffer::active(BandId::Guidance, GUIDANCE_HEIGHT, GUIDANCE_HEIGHT),
        composer_offer,
    ]
}

fn todos_band_offer(app: &AppState) -> DockBandOffer {
    // When the todos feature module lands full projection, this calls into it.
    // Until then (or when empty / feature off / no viewed agent), height 0.
    if let Some(offer) = crate::features::todos::dock_band_offer(app) {
        return offer;
    }
    DockBandOffer::inactive(BandId::Todos)
}

/// Centered size for the per-AgentInstance usage dialog.
fn usage_centered_size(app: &AppState, body: ratatui::layout::Rect) -> (u16, u16) {
    let row_count = app.agent_usage.len().max(1) as u16;
    let compact_multiplier = if body.width < 100 { 2 } else { 1 };
    let content_rows = row_count
        .saturating_mul(compact_multiplier)
        .saturating_add(3);
    let width = cells_from_percent(body.width, 90)
        .clamp(52, 132)
        .min(body.width);
    let height = content_rows
        .saturating_add(5)
        .min(body.height.saturating_sub(2));
    (width, height)
}

fn notifications_centered_size(app: &AppState, body: ratatui::layout::Rect) -> (u16, u16) {
    // Each notice has at least one message row and one metadata row. Longer
    // messages wrap inside the viewport and contribute to panel scrolling.
    let content_rows = (app.notifications.modal_len().max(1) as u16)
        .saturating_mul(2)
        .min(18);
    let width = cells_from_percent(body.width, 88)
        .clamp(52, 120)
        .min(body.width);
    let height = content_rows
        .saturating_add(5)
        .min(body.height.saturating_sub(2));
    (width, height)
}

/// Viewport-driven size for the settings dialog. Its frame stays stable while
/// navigating or filtering; menu content scrolls inside the available body.
fn settings_centered_size(_app: &AppState, body: ratatui::layout::Rect) -> (u16, u16) {
    let width = cells_from_percent(body.width, 88)
        .clamp(60, 120)
        .min(body.width);
    let height = cells_from_percent(body.height, 80)
        .max(18)
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
    let plane = compose_plane(&metrics);
    let modals = compose_modals(modal_surface, &metrics, shell.body);
    let plan = layout_solve(shell.body, &plane, &modals);
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
        Region::Stream => {
            let mut hits = vec![HitRegion {
                region: Region::Stream,
                rect,
                element: Some(HitId::Stream),
            }];
            hits.extend(
                app.timeline
                    .pointer_regions(rect, &app.theme)
                    .into_iter()
                    .map(|(rect, element)| HitRegion {
                        region: Region::Stream,
                        rect,
                        element: Some(element),
                    }),
            );
            hits
        }
        Region::DockBoundary => Vec::new(),
        // Only the summary header toggles disclosure; rows remain read-only.
        Region::Todos => vec![
            HitRegion {
                region: Region::Todos,
                rect,
                element: None,
            },
            HitRegion {
                region: Region::Todos,
                rect: ratatui::layout::Rect::new(rect.x, rect.y, rect.width, rect.height.min(1)),
                element: Some(HitId::TodosToggle),
            },
        ],
        Region::Suggest => app
            .editor
            .auto_complete
            .pointer_regions(rect)
            .into_iter()
            .map(|(row_rect, element)| HitRegion {
                region: Region::Suggest,
                rect: row_rect,
                element: Some(element),
            })
            .collect(),
        Region::Guidance if crate::features::guidance_row::resolve(app).is_notice() => {
            vec![HitRegion {
                region: Region::Guidance,
                rect,
                element: Some(HitId::Notice),
            }]
        }
        Region::Guidance => Vec::new(),
        Region::Composer => vec![HitRegion {
            region: Region::Composer,
            rect,
            element: Some(HitId::Composer),
        }],
        Region::Surface(SurfaceId::Agents) => stamp(app.agent_panel.hit_regions(rect)),
        Region::Surface(SurfaceId::Sessions) => stamp(app.sessions.hit_regions(rect)),
        Region::Surface(SurfaceId::Tree) => stamp(app.tree.hit_regions(rect)),
        Region::Surface(SurfaceId::SummaryPrompt) => {
            let mut hits: Vec<_> = app
                .summary_prompt
                .as_ref()
                .and_then(|workflow| {
                    app.tree.summary_footer_rect(rect).map(|footer| {
                        workflow
                            .component_regions_embedded(footer)
                            .into_iter()
                            .map(|(rect, element)| HitRegion {
                                region: Region::Surface(SurfaceId::SummaryPrompt),
                                rect,
                                element: Some(element),
                            })
                            .collect()
                    })
                })
                .unwrap_or_default();
            hits.push(HitRegion {
                region: Region::Surface(SurfaceId::SummaryPrompt),
                rect,
                element: None,
            });
            hits
        }
        Region::Surface(SurfaceId::Usage) => {
            stamp(<crate::features::usage::UsagePanel as SurfacePanel<
                SurfaceId,
                HitId,
                crate::features::usage::UsageCtx<'_>,
            >>::hit_regions(
                &crate::features::usage::UsagePanel, rect
            ))
        }
        Region::Surface(SurfaceId::Notifications) => {
            let mut regions =
                <crate::features::notifications::NotificationCenter as SurfacePanel<
                    SurfaceId,
                    HitId,
                    crate::features::notifications::NotificationPanelCtx<'_>,
                >>::hit_regions(&app.notifications, rect);
            regions.extend(
                app.notifications
                    .copy_regions(rect, app.session.id.as_deref())
                    .into_iter()
                    .map(|(rect, element)| HitRegion {
                        region: SurfaceId::Notifications,
                        rect,
                        element: Some(element),
                    }),
            );
            stamp(regions)
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
        assert!(frame.plan.rects.contains_key(&Region::DockBoundary));
        assert!(frame.plan.rects.contains_key(&Region::Composer));
        let guidance = frame.plan.rects[&Region::Guidance];
        let composer = frame.plan.rects[&Region::Composer];
        assert_eq!(guidance.height, 1);
        assert_eq!(guidance.y + guidance.height, composer.y);
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
            selected_idx: 0,
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
            selected_idx: 0,
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
        let guidance = composed.plan.layers[0].rects[&Region::Guidance];
        let rows = app
            .approvals
            .workflow()
            .unwrap()
            .dock_content_rows(&app.theme);
        // Surface = workflow rows + Standard pane chrome (4); Guidance is the
        // preceding row in the same bottom-anchored modal tree.
        assert_eq!(host.height, rows + 4);
        assert_eq!(guidance.height, 1);
        assert_eq!(guidance.y + guidance.height, host.y);
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
    fn usage_surface_uses_content_sized_centered_modal() {
        let mut app = app_state();
        app.push_surface(SurfaceId::Usage);
        let frame = compose_frame(&app, Rect::new(0, 0, 100, 30));
        let layer = frame.plan.layers.first().expect("usage layer");
        assert!(matches!(
            layer.placement,
            piko_tui_layout::ModalPlacement::Centered {
                max_width: 90,
                max_height: 9
            }
        ));
        let usage = layer
            .rects
            .get(&Region::Surface(SurfaceId::Usage))
            .expect("usage rect");
        assert_eq!((usage.width, usage.height), (90, 9));
    }

    #[test]
    fn settings_surface_uses_stable_viewport_sized_modal() {
        let mut app = app_state();
        app.push_surface(SurfaceId::Settings);

        let frame = compose_frame(&app, Rect::new(0, 0, 100, 40));
        let layer = frame.plan.layers.first().expect("settings layer");
        assert!(matches!(
            layer.placement,
            piko_tui_layout::ModalPlacement::Centered {
                max_width: 88,
                max_height: 31
            }
        ));
        let settings = layer
            .rects
            .get(&Region::Surface(SurfaceId::Settings))
            .expect("settings rect");
        assert_eq!((settings.width, settings.height), (88, 31));

        // Small terminals preserve one row of backdrop above and below.
        let compact = compose_frame(&app, Rect::new(0, 0, 50, 16));
        let settings = compact.plan.layers[0]
            .rects
            .get(&Region::Surface(SurfaceId::Settings))
            .expect("compact settings rect");
        assert_eq!((settings.width, settings.height), (50, 13));
    }
}
