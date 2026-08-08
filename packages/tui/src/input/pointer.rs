//! Pointer input routing over the per-frame hit map.
//!
//! crossterm reports mouse coordinates **0-based** (same space as ratatui
//! cells), so `(column, row)` maps directly into the hit map. Clicks resolve
//! to the same actions the keyboard router produces; wheel scrolls the
//! stream; composer clicks place the text cursor; hover is tracked.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use piko_protocol::ApprovalDecision;
use piko_tui_layout::HitMap;
use ratatui::layout::Rect;

use crate::app::{
    AppState, HitId,
    command::{
        Action, ApprovalAction, EditorAction, NotificationAction, TimelineAction,
        ToolInteractionAction,
    },
};
use crate::layout::build_surface_hitmap;
use crate::navigation::{Region, SurfaceId};

/// Wheel step in rows.
const WHEEL_STEP: usize = 3;

/// Route one mouse event to keyboard-equivalent actions (plus direct state
/// updates for cursor placement and hover tracking).
pub fn route_pointer(app: &mut AppState, terminal: Rect, event: MouseEvent) -> Vec<Action> {
    let map = build_surface_hitmap(app, terminal);
    let (x, y) = (event.column, event.row);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => click(app, &map, x, y),
        MouseEventKind::Moved => {
            app.hovered = map.hit_test(x, y).map(|h| (h.region, h.element));
            Vec::new()
        }
        MouseEventKind::ScrollUp => wheel(&map, x, y, true),
        MouseEventKind::ScrollDown => wheel(&map, x, y, false),
        _ => Vec::new(),
    }
}

fn click(app: &mut AppState, map: &HitMap<Region, HitId>, x: u16, y: u16) -> Vec<Action> {
    let Some(hit) = map.hit_test(x, y) else {
        return Vec::new();
    };
    match hit.region {
        Region::Surface(SurfaceId::Approval) => match hit.element {
            Some(HitId::Choice { choice, .. }) => vec![Action::Approval(ApprovalAction::Respond(
                approval_decision(choice),
            ))],
            // Dialog background / border: blocking modal, no-op.
            _ => Vec::new(),
        },
        Region::Surface(SurfaceId::ToolInteraction) => match hit.element {
            Some(HitId::Choice { choice, .. }) => vec![
                Action::ToolInteraction(ToolInteractionAction::Choice(choice)),
                Action::ToolInteraction(ToolInteractionAction::Submit),
            ],
            Some(HitId::Tab(step)) => vec![Action::ToolInteraction(
                ToolInteractionAction::GotoStep(step),
            )],
            Some(HitId::Submit) => {
                vec![Action::ToolInteraction(ToolInteractionAction::Submit)]
            }
            _ => Vec::new(),
        },
        Region::Notice => vec![Action::Notifications(NotificationAction::Clear)],
        Region::Suggest => match hit.element {
            Some(HitId::Suggest(idx)) => {
                app.editor.auto_complete.select_index(idx);
                vec![Action::Editor(EditorAction::AcceptSuggestion)]
            }
            _ => Vec::new(),
        },
        Region::Composer => {
            // The composer is an input target only when no modal owns focus.
            if app.modal_surface().is_none() {
                let col = x.saturating_sub(hit.rect.x);
                app.editor.move_to_column(hit.rect.width, col);
            }
            Vec::new()
        }
        // Stream click is a no-op; other surfaces have no element actions yet.
        _ => Vec::new(),
    }
}

fn wheel(map: &HitMap<Region, HitId>, x: u16, y: u16, up: bool) -> Vec<Action> {
    let Some(hit) = map.hit_test(x, y) else {
        return Vec::new();
    };
    if hit.region == Region::Stream {
        vec![Action::Timeline(if up {
            TimelineAction::ScrollUp(WHEEL_STEP)
        } else {
            TimelineAction::ScrollDown(WHEEL_STEP)
        })]
    } else {
        Vec::new()
    }
}

/// Approval workflow choices are fixed-order decisions.
fn approval_decision(choice: usize) -> ApprovalDecision {
    match choice {
        0 => ApprovalDecision::Accept,
        1 => ApprovalDecision::AcceptSession,
        2 => ApprovalDecision::AcceptWorkspace,
        3 => ApprovalDecision::AcceptPermanent,
        _ => ApprovalDecision::Decline,
    }
}
