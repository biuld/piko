//! Pointer input routing over the per-frame hit map.
//!
//! crossterm reports mouse coordinates **0-based** (same space as ratatui
//! cells), so `(column, row)` maps directly into the hit map. Clicks resolve
//! to the same actions the keyboard router produces; wheel scrolls the
//! stream; composer clicks place the text cursor; hover is tracked.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
#[cfg(test)]
use ratatui::layout::Rect;

use crate::app::{
    AppState, HitId,
    command::{Action, PointerAction, PointerTarget, TimelineAction},
};
use crate::features::timeline::WHEEL_STEP;
use crate::layout::PreparedFrame;
use crate::navigation::Region;
use crate::ui::interaction::{ComponentHit, PointerGesture};

/// Route one mouse event to keyboard-equivalent actions and immediately reduce
/// pointer actions for focused integration tests.
#[cfg(test)]
pub fn route_pointer(app: &mut AppState, terminal: Rect, event: MouseEvent) -> Vec<Action> {
    let prepared = crate::layout::prepare_frame(app, terminal);
    let actions = route_pointer_with_hitmap(app, &prepared, event);
    let mut reduced = Vec::new();
    for action in actions {
        match action {
            Action::Pointer(action) => reduced.extend(app.reduce_pointer_action(action)),
            action => reduced.push(action),
        }
    }
    reduced
}

/// Production pointer path: resolve against the geometry retained by the last
/// painted frame. Scrollable regions resolve against live state (content-space
/// row map + current viewport offset), so scroll batches can never make a hit
/// stale; the per-frame map remains authoritative for static regions, z-order,
/// and modal barriers.
pub fn route_pointer_with_hitmap(
    app: &AppState,
    prepared: &PreparedFrame,
    event: MouseEvent,
) -> Vec<Action> {
    let (x, y) = (event.column, event.row);
    let target = resolve_target(app, prepared, x, y);
    if matches!(
        target,
        PointerTarget::Component {
            region: Region::Stream,
            ..
        }
    ) {
        match event.kind {
            MouseEventKind::ScrollUp => {
                return vec![TimelineAction::ScrollUp(WHEEL_STEP).into()];
            }
            MouseEventKind::ScrollDown => {
                return vec![TimelineAction::ScrollDown(WHEEL_STEP).into()];
            }
            _ => {}
        }
    }
    let action = match event.kind {
        MouseEventKind::Down(MouseButton::Left) => PointerAction::LeftDown(target),
        MouseEventKind::Up(MouseButton::Left) => PointerAction::LeftUp(target),
        MouseEventKind::Moved => PointerAction::Move(top_modal_hit(app, prepared, x, y)),
        MouseEventKind::ScrollUp => PointerAction::Gesture {
            target,
            gesture: PointerGesture::ScrollUp,
        },
        MouseEventKind::ScrollDown => PointerAction::Gesture {
            target,
            gesture: PointerGesture::ScrollDown,
        },
        _ => return Vec::new(),
    };
    vec![action.into()]
}

fn resolve_target(app: &AppState, prepared: &PreparedFrame, x: u16, y: u16) -> PointerTarget {
    let map = &prepared.hit_map;
    let hit = map.hit_test(x, y).copied();
    if let Some(surface) = app.modal_surface()
        && !map.is_top_layer_hit(hit.as_ref())
    {
        return PointerTarget::OutsideModal(surface);
    }

    let Some(hit) = hit else {
        return PointerTarget::None;
    };
    if hit.region == Region::Stream {
        return stream_target(app, prepared, hit, x, y);
    }
    PointerTarget::Component {
        region: hit.region,
        hit: ComponentHit {
            element: hit.element,
            rect: hit.rect,
            x,
            y,
        },
    }
}

/// Resolve a plane hit inside the Stream against the live timeline plan.
/// Tool title rows resolve to their stable interned id; every other row falls
/// back to the Stream default (wheel scrolls, click is a no-op).
fn stream_target(
    app: &AppState,
    prepared: &PreparedFrame,
    hit: piko_tui_layout::Hit<Region, HitId>,
    x: u16,
    y: u16,
) -> PointerTarget {
    let resolved = prepared
        .timeline
        .as_ref()
        .and_then(|plan| plan.resolve(x, y, app.timeline().viewport.top_offset()));
    let (element, rect) = match resolved {
        Some((element, rect)) => (Some(element), rect),
        None => (hit.element, hit.rect),
    };
    PointerTarget::Component {
        region: Region::Stream,
        hit: ComponentHit {
            element,
            rect,
            x,
            y,
        },
    }
}

fn top_modal_hit(
    app: &AppState,
    prepared: &PreparedFrame,
    x: u16,
    y: u16,
) -> Option<(Region, Option<HitId>)> {
    let map = &prepared.hit_map;
    let hit = if app.modal_surface().is_some() {
        map.hit_test_top_layer(x, y)
    } else {
        map.hit_test(x, y)
    };
    let hit = hit?;
    if hit.region == Region::Stream {
        let element = prepared.timeline.as_ref().and_then(|plan| {
            plan.resolve(x, y, app.timeline().viewport.top_offset())
                .map(|(element, _)| element)
        });
        return Some((hit.region, element.or(hit.element)));
    }
    Some((hit.region, hit.element))
}
