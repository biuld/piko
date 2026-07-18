//! Window-local island open prefs, sizes, and breakpoint rules.
//!
//! Size fields (`session_width`, `right_column_width`, `agents_height`) are
//! persistence for the fixed Workbench tree — not layout units.

use super::island_tree::IslandId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutBreakpoint {
    Wide,
    Medium,
    Narrow,
}

impl LayoutBreakpoint {
    pub fn from_width(width_px: f32) -> Self {
        if width_px >= 1200.0 {
            Self::Wide
        } else if width_px >= 900.0 {
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
pub const CENTER_MIN_WIDTH: f32 = 620.0;
pub const AGENTS_DEFAULT_HEIGHT: f32 = 220.0;

#[derive(Debug, Clone)]
pub struct IslandLayoutState {
    pub breakpoint: LayoutBreakpoint,
    /// User preference: Sessions docked when the breakpoint allows.
    pub sessions_open: bool,
    /// User preference: Agents docked when the breakpoint allows.
    pub agents_open: bool,
    /// User preference: Tree docked when the breakpoint allows.
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
        Self {
            breakpoint: LayoutBreakpoint::Wide,
            sessions_open: true,
            agents_open: true,
            tree_open: true,
            session_width: SESSION_DEFAULT_WIDTH,
            right_column_width: RIGHT_COLUMN_DEFAULT_WIDTH,
            agents_height: AGENTS_DEFAULT_HEIGHT,
            close_session_sheet_on_live: false,
        }
    }
}

impl IslandLayoutState {
    pub fn sync_breakpoint(&mut self, width_px: f32) {
        self.breakpoint = LayoutBreakpoint::from_width(width_px);
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
    }

    pub fn toggle(&mut self, id: IslandId) {
        self.set_open(id, !self.prefer_open(id));
    }

    /// Convenience: toggle Agents and Tree together (`cmd-i`).
    pub fn toggle_right_column(&mut self) {
        let next = !(self.agents_open && self.tree_open);
        self.agents_open = next;
        self.tree_open = next;
    }

    pub fn right_column_pref_open(&self) -> bool {
        self.agents_open && self.tree_open
    }

    /// Whether the island is shown as a docked Workbench panel (not Sheet).
    ///
    /// Dock visibility does not require a live session: Agents/Tree stay in the
    /// Wide layout and show Empty/Loading until a session is ready.
    pub fn is_docked_visible(&self, id: IslandId, _session_live: bool) -> bool {
        if !self.prefer_open(id) {
            return false;
        }
        match id {
            IslandId::Timeline | IslandId::Composer => true,
            IslandId::Sessions => matches!(
                self.breakpoint,
                LayoutBreakpoint::Wide | LayoutBreakpoint::Medium
            ),
            IslandId::Agents | IslandId::Tree => self.breakpoint == LayoutBreakpoint::Wide,
        }
    }

    /// Whether Agents and/or Tree are docked (trailing horizontal sibling present).
    pub fn any_right_column_docked(&self, session_live: bool) -> bool {
        self.is_docked_visible(IslandId::Agents, session_live)
            || self.is_docked_visible(IslandId::Tree, session_live)
    }
}
