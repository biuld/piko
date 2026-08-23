//! Product composition: shell + plane + modal stack via `piko-tui-layout`.
//!
//! Plane dock band heights are allocated only via Dock Stack offer → solve →
//! grant ([`crate::features::dock_stack`]).

use crate::{
    app::{AppState, HitId},
    features::{
        dock_stack::{
            BandId, COMPOSER_MIN_HEIGHT, DOCK_BOUNDARY_HEIGHT, DockBandOffer, DockSolveInput,
            GUIDANCE_HEIGHT, SUGGEST_MIN_HEIGHT, solve, suggestion_preferred_height,
        },
        timeline::TimelineRenderPlan,
    },
    navigation::{
        CenteredSizePolicy, SelectBandBudget, SurfaceIntent, SurfaceSizing, compose_modals,
        compose_plane,
    },
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

/// Geometry and expensive feature projections shared by one paint and all
/// pointer events routed before the next paint.
pub struct PreparedFrame {
    pub product: ProductFrame,
    pub hit_map: HitMap<Region, HitId>,
    pub(crate) timeline: Option<TimelineRenderPlan>,
}

impl PreparedFrame {
    /// Rebuild the retained timeline plan when content changed since it was
    /// painted (layout epoch mismatch). Pure scroll never bumps the epoch, so
    /// the common path stays a no-op and hit-testing reads the live offset.
    ///
    /// Paint consumes the plan while drawing (see `render_prepared`), so a
    /// freshly painted frame has no plan to route input against. Rebuild it on
    /// demand; a plan that is still present and at the current epoch is left
    /// untouched.
    pub(crate) fn refresh_timeline(&mut self, app: &AppState) {
        if self.timeline.is_some()
            && self
                .timeline
                .as_ref()
                .is_some_and(|plan| plan.epoch == app.timeline().layout_epoch())
        {
            return;
        }
        let Some(area) = self.product.plan.rects.get(&Region::Stream).copied() else {
            self.timeline = None;
            return;
        };
        if self
            .product
            .modal_surface
            .is_some_and(SurfaceId::covers_body)
        {
            self.timeline = None;
            return;
        }
        let hovered_tool = app
            .hovered
            .and_then(|(region, element)| (region == Region::Stream).then_some(element).flatten())
            .and_then(|element| match element {
                HitId::TimelineTool(hit_id) => Some(hit_id),
                _ => None,
            });
        self.timeline = Some(app.timeline().render_plan(area, &app.theme, hovered_tool));
    }
}

pub fn resolve_modal_surface(app: &AppState) -> Option<SurfaceId> {
    app.modal_surface()
}

/// Collect per-band offers and solve joint height under Stream floor.
pub fn plane_metrics(app: &AppState, body: ratatui::layout::Rect) -> PlaneMetrics {
    let modal = resolve_modal_surface(app);
    let centered_size = match modal.map(|surface| surface.spec().sizing) {
        Some(SurfaceSizing::Centered(CenteredSizePolicy::SettingsViewport)) => {
            Some(settings_centered_size(app, body))
        }
        Some(SurfaceSizing::Centered(CenteredSizePolicy::UsageContent)) => {
            Some(usage_centered_size(app, body))
        }
        Some(SurfaceSizing::Centered(CenteredSizePolicy::NotificationContent)) => {
            Some(notifications_centered_size(app, body))
        }
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

pub fn prepare_frame(app: &AppState, terminal: ratatui::layout::Rect) -> PreparedFrame {
    let product = compose_frame(app, terminal);
    let plane_is_blocked = product.modal_surface.is_some();
    let hovered_tool = (!plane_is_blocked)
        .then_some(app.hovered)
        .flatten()
        .and_then(|(region, element)| (region == Region::Stream).then_some(element).flatten())
        .and_then(|element| match element {
            HitId::TimelineTool(hit_id) => Some(hit_id),
            _ => None,
        });
    let timeline = product
        .plan
        .rects
        .get(&Region::Stream)
        .copied()
        .filter(|_| !product.modal_surface.is_some_and(SurfaceId::covers_body))
        .map(|area| app.timeline().render_plan(area, &app.theme, hovered_tool));
    let hit_map = build_surface_hitmap_for_frame(app, &product);
    PreparedFrame {
        product,
        hit_map,
        timeline,
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
    prepare_frame(app, terminal).hit_map
}

fn build_surface_hitmap_for_frame(
    app: &AppState,
    composed: &ProductFrame,
) -> HitMap<Region, HitId> {
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
            // Tool hits are resolved live in content space at event time
            // (scroll must never invalidate them); the map only carries the
            // Stream default action over the whole region.
            vec![HitRegion {
                region: Region::Stream,
                rect,
                element: Some(HitId::Stream),
            }]
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
    app.mode().is_editor_base() && app.editor.auto_complete.is_active()
}

#[cfg(test)]
mod tests;
