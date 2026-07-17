//! Flat layout system — pure function from LayoutMode to Vec<Constraint>.
//!
//! Design:
//! - All panels are independent rows; no nesting. No floaters — every visible
//!   element participates in the layout.
//! - Layout is a pure function of `LayoutMode` + dynamic measurements.
//! - Visibility = removal from constraint array (no 0-height tricks).
//! - Replacement = same position swap.
//! - BottomBar is always the last constraint.
//! - Slot layout is edge-flush; Timeline applies its own horizontal inset.

use ratatui::layout::{Constraint, Rect};

use crate::app::{AppMode, AppState, Placement};

/// Default left/right gutter for edge-flush chat surfaces without full borders
/// (Timeline, AgentPanel).
pub const DEFAULT_HORIZONTAL_INSET: u16 = 1;

/// Shrink `area` by `inset` cells on the left and right only.
///
/// On tiny terminals the inset is clamped so at least one cell of width remains
/// whenever the original area is non-empty.
pub fn inset_horizontal(area: Rect, inset: u16) -> Rect {
    if area.width == 0 {
        return area;
    }
    let horizontal = inset.min(area.width.saturating_sub(1) / 2);
    Rect {
        x: area.x.saturating_add(horizontal),
        y: area.y,
        width: area.width.saturating_sub(horizontal.saturating_mul(2)),
        height: area.height,
    }
}

/// The four layout configurations.  Mapped from AppMode + Approval state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    /// Default: Timeline → AgentPanel → NotificationRow? → Editor → BottomBar
    Chat,
    /// An overlay replaces the Editor slot (Slot D).
    PartialOverlay { mode: AppMode },
    /// A full-screen overlay replaces all slots except BottomBar.
    FullOverlay { mode: AppMode },
}

impl LayoutMode {
    /// Compute the LayoutMode from current AppState.
    pub fn from_app(app: &AppState) -> Self {
        // Approval always wins — it blocks all other modes.
        if !app.approvals.is_empty() || app.focus_manager.active_mode() == AppMode::Approval {
            return LayoutMode::PartialOverlay {
                mode: AppMode::Approval,
            };
        }
        if !app.interactions.is_empty()
            || app.focus_manager.active_mode() == AppMode::ToolInteraction
        {
            return LayoutMode::PartialOverlay {
                mode: AppMode::ToolInteraction,
            };
        }

        let active = app.focus_manager.active_mode();
        match active.placement() {
            Some(Placement::Full) => LayoutMode::FullOverlay { mode: active },
            Some(Placement::Partial) => LayoutMode::PartialOverlay { mode: active },
            None => LayoutMode::Chat,
        }
    }
}

/// Slot indices returned by `build_constraints`.  Not all slots are present in
/// every mode; `Option<usize>` slots signal conditional presence.
#[derive(Clone, Copy, Debug)]
pub struct LayoutSlots {
    /// Slot A: Timeline (Chat/Partial/Approval) or FullPanel (FullOverlay).
    pub timeline_or_full: usize,
    /// Slot B: AgentPanel (present in Chat/Partial/Approval).
    pub agent_panel: Option<usize>,
    /// Slot C: NotificationRow (conditional, only in Chat/Partial when idle + notifs).
    pub notification_row: Option<usize>,
    /// Slot D-prime: PartialPanel (PartialOverlay) or ApprovalPanel (Approval).
    pub partial_or_approval: Option<usize>,
    /// Slot D: Editor (Chat/Approval only).
    pub editor: Option<usize>,
    /// Slot between NotificationRow and Editor: completion suggestions (Chat only,
    /// conditional on has_suggestions).
    pub suggestions: Option<usize>,
    /// Slot E: BottomBar (always present).
    pub bottom_bar: usize,
}

/// Core pure function: `LayoutMode → (Vec<Constraint>, LayoutSlots)`.
///
/// Slot layout per mode:
///
/// | Mode            | Slot A            | Slot B        | Slot C               | Slot D'           | Slot D                 | Slot E    |
/// |-----------------|-------------------|---------------|----------------------|-------------------|------------------------|-----------|
/// | Chat            | Fill(1) Timeline  | Length(h)     | Length(1) if notif   | Length(s) if sugg | Length(5) Editor       | Length(1) |
/// | PartialOverlay  | Fill(1) Timeline  | Length(h)     | Length(1) if notif   | —                 | Fill(1) Partial         | Length(1) |
/// | FullOverlay     | Fill(1) FullPanel | —             | —                    | —                 | —                      | Length(1) |
pub fn build_constraints(
    mode: LayoutMode,
    agent_height: u16,
    has_notification: bool,
    has_suggestions: bool,
    suggestion_count: usize,
    editor_height: u16,
) -> (Vec<Constraint>, LayoutSlots) {
    match mode {
        LayoutMode::Chat => build_chat_or_partial(
            false,
            agent_height,
            has_notification,
            has_suggestions,
            suggestion_count,
            editor_height,
        ),
        LayoutMode::PartialOverlay { mode: _ } => {
            build_chat_or_partial(true, agent_height, has_notification, false, 0, 0)
        }
        LayoutMode::FullOverlay { mode: _ } => build_full(),
    }
}

// ── mode-specific builders ───────────────────────────────────────────────────

fn build_chat_or_partial(
    is_partial: bool,
    agent_height: u16,
    has_notification: bool,
    has_suggestions: bool,
    suggestion_count: usize,
    editor_height: u16,
) -> (Vec<Constraint>, LayoutSlots) {
    let mut constraints: Vec<Constraint> = Vec::new();
    let mut idx: usize = 0;

    // Slot A: Timeline (elastic)
    constraints.push(Constraint::Fill(1));
    let timeline_or_full = idx;
    idx += 1;

    // Slot B: AgentPanel (fixed)
    constraints.push(Constraint::Length(agent_height));
    let agent_panel = idx;
    idx += 1;

    // Slot C: NotificationRow (conditional, only if idle + has notifications)
    let notification_row = if has_notification {
        constraints.push(Constraint::Length(1));
        let n = idx;
        idx += 1;
        Some(n)
    } else {
        None
    };

    // Slot D': Completion suggestions (Chat only, when visible)
    let suggestions = if !is_partial && has_suggestions {
        let h = (suggestion_count as u16 + 2).min(8);
        constraints.push(Constraint::Length(h));
        let s = idx;
        idx += 1;
        Some(s)
    } else {
        None
    };

    // Slot D: Editor (Chat) or PartialPanel (PartialOverlay)
    let partial_or_approval;
    let editor;
    if is_partial {
        constraints.push(Constraint::Fill(1));
        partial_or_approval = Some(idx);
        editor = None;
        idx += 1;
    } else {
        constraints.push(Constraint::Length(editor_height));
        partial_or_approval = None;
        editor = Some(idx);
        idx += 1;
    }

    // Slot E: BottomBar (always)
    constraints.push(Constraint::Length(1));
    let bottom_bar = idx;

    (
        constraints,
        LayoutSlots {
            timeline_or_full,
            agent_panel: Some(agent_panel),
            notification_row,
            partial_or_approval,
            editor,
            suggestions,
            bottom_bar,
        },
    )
}

fn build_full() -> (Vec<Constraint>, LayoutSlots) {
    let constraints = vec![
        Constraint::Fill(1),   // Slot A: FullPanel (replaces A+B+C+D)
        Constraint::Length(1), // Slot E: BottomBar
    ];
    let slots = LayoutSlots {
        timeline_or_full: 0,
        agent_panel: None,
        notification_row: None,
        partial_or_approval: None,
        editor: None,
        suggestions: None,
        bottom_bar: 1,
    };
    (constraints, slots)
}

// ── dynamic measurements ─────────────────────────────────────────────────────

/// Compute the dynamic agent panel height:
/// - Collapsed: 1 (when no active turn, no queue)
/// - Expanded: 2 (agent line + queue line)
pub fn agent_panel_height(app: &AppState) -> u16 {
    app.agent_panel.height()
}

/// Whether the notification row should be visible.
pub fn has_visible_notification(app: &AppState) -> bool {
    app.notifications.has_visible()
}

/// Whether completion suggestions are visible (Chat mode only).
pub fn has_visible_suggestions(app: &AppState) -> bool {
    app.mode == AppMode::Chat && app.editor.auto_complete.is_active()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inset_horizontal_applies_left_right_gutter() {
        let area = Rect::new(0, 0, 80, 24);
        let inset = inset_horizontal(area, 1);
        assert_eq!(inset, Rect::new(1, 0, 78, 24));
    }

    #[test]
    fn inset_horizontal_clamps_on_tiny_terminal() {
        let area = Rect::new(0, 0, 3, 2);
        let inset = inset_horizontal(area, 1);
        assert_eq!(inset, Rect::new(1, 0, 1, 2));
    }

    #[test]
    fn inset_horizontal_preserves_empty_width() {
        let area = Rect::new(5, 5, 0, 10);
        assert_eq!(inset_horizontal(area, 1), area);
    }

    #[test]
    fn inset_horizontal_zero_is_identity() {
        let area = Rect::new(2, 3, 40, 20);
        assert_eq!(inset_horizontal(area, 0), area);
    }
}
