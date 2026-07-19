//! Window-local island open prefs, sizes, and dock-fit rules.
//!
//! Size fields (`session_width`, `right_column_width`, `agents_height`) are
//! persistence for the fixed Workbench tree — not layout units.
//!
//! Dock visibility is resolved from user prefs + available width (center
//! minimum protected). Auto-collapse never overwrites prefs. Collapse order
//! when both columns are preferred: right first, then left.

use super::island_tree::IslandId;

/// Horizontal gutter between islands / canvas padding (matches `UiMetrics::island_gutter`).
pub const WORKBENCH_GUTTER: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutBreakpoint {
    Wide,
    Medium,
    Narrow,
}

impl LayoutBreakpoint {
    /// Derive a Sheet/UX hint from effective docked columns (not an authority for dock).
    pub fn from_docked(sessions: bool, right_column: bool) -> Self {
        if sessions && right_column {
            Self::Wide
        } else if sessions {
            Self::Medium
        } else {
            Self::Narrow
        }
    }
}

pub const SESSION_DEFAULT_WIDTH: f32 = 300.0;
pub const SESSION_MIN_WIDTH: f32 = 220.0;
pub const RIGHT_COLUMN_DEFAULT_WIDTH: f32 = 340.0;
pub const RIGHT_COLUMN_MIN_WIDTH: f32 = 260.0;
/// Readable center column floor; dock budget and single-column layout share this.
pub const CENTER_MIN_WIDTH: f32 = 620.0;
/// Window min width = center floor + left/right canvas gutters (keeps them consistent).
pub const WINDOW_MIN_WIDTH: f32 = CENTER_MIN_WIDTH + 2.0 * WORKBENCH_GUTTER;
/// Practical window height floor (TitleBar + StatusBar + usable center).
pub const WINDOW_MIN_HEIGHT: f32 = 600.0;
pub const AGENTS_DEFAULT_HEIGHT: f32 = 220.0;

/// Effective docked columns after fit resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DockedColumns {
    pub sessions: bool,
    pub right_column: bool,
}

/// Resolve which outer columns can dock given prefs and window width.
///
/// `window_width` is the full viewport width. Canvas outer padding (2× gutter)
/// and inter-column gutters are subtracted inside.
///
/// When both prefs are open but space is tight: drop the right column first,
/// then the left. Auto-collapse does not mutate prefs.
pub fn resolve_docked(sessions_pref: bool, right_pref: bool, window_width: f32) -> DockedColumns {
    let gutter = WORKBENCH_GUTTER;
    let inner = (window_width - 2.0 * gutter).max(0.0);
    let both_need = CENTER_MIN_WIDTH + SESSION_MIN_WIDTH + RIGHT_COLUMN_MIN_WIDTH + 2.0 * gutter;
    let left_need = CENTER_MIN_WIDTH + SESSION_MIN_WIDTH + gutter;
    let right_need = CENTER_MIN_WIDTH + RIGHT_COLUMN_MIN_WIDTH + gutter;

    if sessions_pref && right_pref {
        if inner >= both_need {
            DockedColumns {
                sessions: true,
                right_column: true,
            }
        } else if inner >= left_need {
            DockedColumns {
                sessions: true,
                right_column: false,
            }
        } else {
            DockedColumns::default()
        }
    } else if sessions_pref {
        DockedColumns {
            sessions: inner >= left_need,
            right_column: false,
        }
    } else if right_pref {
        DockedColumns {
            sessions: false,
            right_column: inner >= right_need,
        }
    } else {
        DockedColumns::default()
    }
}

#[derive(Debug, Clone)]
pub struct IslandLayoutState {
    /// Last synced viewport width (points); drives fit resolution.
    pub window_width: f32,
    /// Derived Sheet/UX hint from current docked columns.
    pub breakpoint: LayoutBreakpoint,
    /// User preference: Sessions docked when width allows.
    pub sessions_open: bool,
    /// User preference: Agents docked when width allows.
    pub agents_open: bool,
    /// User preference: Tree docked when width allows.
    pub tree_open: bool,
    pub session_width: f32,
    /// Shared width when Agents and/or Tree appear as the trailing horizontal split.
    pub right_column_width: f32,
    /// Preferred height for the Agents island inside the Agents ↕ Tree split.
    pub agents_height: f32,
    pub close_session_sheet_on_live: bool,
}

impl Default for IslandLayoutState {
    fn default() -> Self {
        let window_width = 1360.0;
        let mut state = Self {
            window_width,
            breakpoint: LayoutBreakpoint::Wide,
            sessions_open: true,
            agents_open: true,
            tree_open: true,
            session_width: SESSION_DEFAULT_WIDTH,
            right_column_width: RIGHT_COLUMN_DEFAULT_WIDTH,
            agents_height: AGENTS_DEFAULT_HEIGHT,
            close_session_sheet_on_live: false,
        };
        state.sync_window_width(window_width);
        state
    }
}

impl IslandLayoutState {
    /// Sync viewport width and refresh the derived breakpoint hint.
    pub fn sync_window_width(&mut self, width_px: f32) {
        self.window_width = width_px;
        let docked = self.docked_columns();
        self.breakpoint = LayoutBreakpoint::from_docked(docked.sessions, docked.right_column);
    }

    pub fn docked_columns(&self) -> DockedColumns {
        resolve_docked(
            self.sessions_open,
            self.right_column_pref_open(),
            self.window_width,
        )
    }

    pub fn prefer_open(&self, id: IslandId) -> bool {
        match id {
            IslandId::Sessions => self.sessions_open,
            IslandId::Timeline | IslandId::Composer => true,
            IslandId::Agents => self.agents_open,
            IslandId::Tree => self.tree_open,
        }
    }

    pub fn set_open(&mut self, id: IslandId, open: bool) {
        match id {
            IslandId::Sessions => self.sessions_open = open,
            IslandId::Timeline | IslandId::Composer => {}
            IslandId::Agents => self.agents_open = open,
            IslandId::Tree => self.tree_open = open,
        }
        let docked = self.docked_columns();
        self.breakpoint = LayoutBreakpoint::from_docked(docked.sessions, docked.right_column);
    }

    pub fn set_right_column_open(&mut self, open: bool) {
        self.agents_open = open;
        self.tree_open = open;
        let docked = self.docked_columns();
        self.breakpoint = LayoutBreakpoint::from_docked(docked.sessions, docked.right_column);
    }

    pub fn right_column_pref_open(&self) -> bool {
        self.agents_open && self.tree_open
    }

    /// Whether the island is shown as a docked Workbench panel (not Sheet).
    ///
    /// Dock visibility does not require a live session: Agents/Tree stay docked
    /// when width allows and show Empty/Loading until a session is ready.
    pub fn is_docked_visible(&self, id: IslandId, _session_live: bool) -> bool {
        let docked = self.docked_columns();
        match id {
            IslandId::Timeline | IslandId::Composer => true,
            IslandId::Sessions => self.sessions_open && docked.sessions,
            IslandId::Agents | IslandId::Tree => self.prefer_open(id) && docked.right_column,
        }
    }

    /// Whether Agents and/or Tree are docked (trailing horizontal sibling present).
    pub fn any_right_column_docked(&self, session_live: bool) -> bool {
        self.is_docked_visible(IslandId::Agents, session_live)
            || self.is_docked_visible(IslandId::Tree, session_live)
    }
}
