//! Pointer input routing over the per-frame hit map.
//!
//! crossterm reports mouse coordinates **0-based** (same space as ratatui
//! cells), so `(column, row)` maps directly into the hit map. Clicks resolve
//! to the same actions the keyboard router produces; wheel scrolls the
//! stream; composer clicks place the text cursor; hover is tracked.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use piko_tui_layout::HitMap;
#[cfg(test)]
use ratatui::layout::Rect;

use crate::app::{
    AppState, HitId,
    command::{Action, PointerAction, PointerTarget, TimelineAction},
};
use crate::features::timeline::WHEEL_STEP;
use crate::navigation::Region;
use crate::ui::interaction::{ComponentHit, PointerGesture};

/// Route one mouse event to keyboard-equivalent actions and immediately reduce
/// pointer actions for focused integration tests.
#[cfg(test)]
pub fn route_pointer(app: &mut AppState, terminal: Rect, event: MouseEvent) -> Vec<Action> {
    let map = crate::layout::build_surface_hitmap(app, terminal);
    let actions = route_pointer_with_hitmap(app, &map, event);
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
/// painted frame. This function never composes layout or rebuilds a hit map.
pub fn route_pointer_with_hitmap(
    app: &AppState,
    map: &HitMap<Region, HitId>,
    event: MouseEvent,
) -> Vec<Action> {
    let (x, y) = (event.column, event.row);
    let target = resolve_target(app, map, x, y);
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
        MouseEventKind::Moved => {
            PointerAction::Move(top_modal_hit(app, map, x, y).map(|hit| (hit.region, hit.element)))
        }
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

fn resolve_target(app: &AppState, map: &HitMap<Region, HitId>, x: u16, y: u16) -> PointerTarget {
    let hit = map.hit_test(x, y).copied();
    if let Some(surface) = app.modal_surface()
        && !map.is_top_layer_hit(hit.as_ref())
    {
        return PointerTarget::OutsideModal(surface);
    }

    let Some(hit) = hit else {
        return PointerTarget::None;
    };
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
