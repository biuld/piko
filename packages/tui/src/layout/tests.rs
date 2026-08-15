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
fn prepared_frame_reuses_timeline_plan_for_tool_hits() {
    use crate::app::ToolStatus;
    use crate::features::timeline::{TimelineEntry, ToolEntry};

    let mut app = app_state();
    app.timeline_mut().push(TimelineEntry::Tool(ToolEntry::new(
        "call-1".into(),
        "read".into(),
        ToolStatus::Completed,
        r#"{"path":"README.md"}"#.into(),
        Some("contents".into()),
        None,
    )));

    let prepared = prepare_frame(&app, Rect::new(0, 0, 80, 24));
    let plan_hit = prepared
        .timeline
        .as_ref()
        .and_then(|plan| plan.tool_regions.first())
        .copied()
        .expect("prepared timeline tool hit");
    let map_hit = prepared
        .hit_map
        .hits
        .iter()
        .find(|hit| hit.element == Some(plan_hit.1))
        .expect("frame hit map tool hit");

    assert_eq!(map_hit.rect, plan_hit.0);
    assert_eq!(map_hit.region, Region::Stream);
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
