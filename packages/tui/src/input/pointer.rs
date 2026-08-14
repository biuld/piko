//! Pointer input routing over the per-frame hit map.
//!
//! crossterm reports mouse coordinates **0-based** (same space as ratatui
//! cells), so `(column, row)` maps directly into the hit map. Clicks resolve
//! to the same actions the keyboard router produces; wheel scrolls the
//! stream; composer clicks place the text cursor; hover is tracked.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use piko_tui_layout::HitMap;
use ratatui::layout::Rect;

use crate::app::{AppState, HitId, command::Action};
use crate::layout::build_surface_hitmap;
use crate::navigation::{OutsideClickPolicy, Region, SurfaceId};
use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};

/// Route one mouse event to keyboard-equivalent actions (plus direct state
/// updates for cursor placement and hover tracking).
pub fn route_pointer(app: &mut AppState, terminal: Rect, event: MouseEvent) -> Vec<Action> {
    let map = build_surface_hitmap(app, terminal);
    let (x, y) = (event.column, event.row);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.pointer_left_down = true;
            route_component(app, &map, x, y, PointerGesture::Activate)
        }
        MouseEventKind::Up(MouseButton::Left) => {
            // Crossterm normally reports press + release, while some terminal
            // transports surface only release. Activate the latter, but never
            // run a paired click twice (critical for disclosure toggles).
            if std::mem::take(&mut app.pointer_left_down) {
                Vec::new()
            } else {
                route_component(app, &map, x, y, PointerGesture::Activate)
            }
        }
        MouseEventKind::Moved => {
            app.hovered = top_modal_hit(app, &map, x, y).map(|h| (h.region, h.element));
            Vec::new()
        }
        MouseEventKind::ScrollUp => route_component(app, &map, x, y, PointerGesture::ScrollUp),
        MouseEventKind::ScrollDown => route_component(app, &map, x, y, PointerGesture::ScrollDown),
        _ => Vec::new(),
    }
}

fn route_component(
    app: &mut AppState,
    map: &HitMap<Region, HitId>,
    x: u16,
    y: u16,
    gesture: PointerGesture,
) -> Vec<Action> {
    let hit = map.hit_test(x, y).copied();
    if let Some(surface) = app.modal_surface()
        && !map.is_top_layer_hit(hit.as_ref())
    {
        return match (gesture, surface.outside_click_policy()) {
            (PointerGesture::Activate, OutsideClickPolicy::Dismiss) => {
                vec![crate::app::command::SurfaceAction::Close.into()]
            }
            _ => Vec::new(),
        };
    }

    let Some(hit) = hit else {
        return Vec::new();
    };
    let component_hit = ComponentHit {
        element: hit.element,
        rect: hit.rect,
        x,
        y,
    };
    match hit.region {
        Region::Surface(SurfaceId::Approval) => app.approvals.pointer_event(component_hit, gesture),
        Region::Surface(SurfaceId::ToolInteraction) => {
            app.interactions.pointer_event(component_hit, gesture)
        }
        Region::Surface(SurfaceId::Agents) => app.agent_panel.pointer_event(component_hit, gesture),
        Region::Surface(SurfaceId::Sessions) => app.sessions.pointer_event(component_hit, gesture),
        Region::Surface(SurfaceId::Models) => app.models.pointer_event(component_hit, gesture),
        Region::Surface(SurfaceId::Thinking) => app.thinking.pointer_event(component_hit, gesture),
        Region::Surface(SurfaceId::Settings) => app.settings.pointer_event(component_hit, gesture),
        Region::Surface(SurfaceId::AuthSelector) => {
            app.auth_selector.pointer_event(component_hit, gesture)
        }
        Region::Surface(SurfaceId::Mcp) => app.mcp.pointer_event(component_hit, gesture),
        Region::Surface(SurfaceId::Processes) => {
            app.processes.pointer_event(component_hit, gesture)
        }
        Region::Surface(SurfaceId::Diagnostics) => {
            app.diagnostics.pointer_event(component_hit, gesture)
        }
        Region::Surface(SurfaceId::Notifications) => {
            app.notifications.pointer_event(component_hit, gesture)
        }
        Region::Surface(SurfaceId::Tree) => app.tree.pointer_event(component_hit, gesture),
        Region::Surface(SurfaceId::SummaryPrompt) => {
            if gesture != PointerGesture::Activate {
                return Vec::new();
            }
            let Some(workflow) = app.summary_prompt.as_mut() else {
                return Vec::new();
            };
            match component_hit.element {
                Some(HitId::Choice { choice, .. }) => {
                    workflow.select_choice(choice);
                    vec![crate::app::command::SurfaceAction::Confirm.into()]
                }
                Some(HitId::Tab(step)) => {
                    workflow.goto_step(step);
                    Vec::new()
                }
                Some(HitId::Submit) => vec![crate::app::command::SurfaceAction::Confirm.into()],
                Some(HitId::TextInput) => {
                    workflow.move_active_input_to_column(component_hit.local_x());
                    Vec::new()
                }
                _ => Vec::new(),
            }
        }
        Region::Guidance => app.notifications.pointer_event(component_hit, gesture),
        Region::Todos
            if gesture == PointerGesture::Activate
                && component_hit.element == Some(HitId::TodosToggle) =>
        {
            app.todo_lists.toggle_collapsed();
            Vec::new()
        }
        // Todo rows are read-only / non-focusable.
        Region::DockBoundary | Region::Todos => Vec::new(),
        Region::Suggest => app
            .editor
            .auto_complete
            .pointer_event(component_hit, gesture),
        Region::Composer => app.editor.pointer_event(component_hit, gesture),
        Region::Stream => app.timeline.pointer_event(component_hit, gesture),
        // Components without stable element actions consume their surface but
        // intentionally expose no pointer behavior yet.
        _ => Vec::new(),
    }
}

fn top_modal_hit<'a>(
    app: &AppState,
    map: &'a HitMap<Region, HitId>,
    x: u16,
    y: u16,
) -> Option<&'a piko_tui_layout::Hit<Region, HitId>> {
    let hit = map.hit_test(x, y);
    if app.modal_surface().is_some() {
        map.hit_test_top_layer(x, y)
    } else {
        hit
    }
}
