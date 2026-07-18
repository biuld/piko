//! Window-local Workbench layout: breakpoints, pane visibility, sizes.

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
pub const INSPECTOR_DEFAULT_WIDTH: f32 = 340.0;
pub const INSPECTOR_MIN_WIDTH: f32 = 260.0;
pub const CENTER_MIN_WIDTH: f32 = 620.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Agents,
    Map,
}

#[derive(Debug, Clone)]
pub struct LayoutState {
    pub breakpoint: LayoutBreakpoint,
    /// User preference: keep session pane when wide enough.
    pub prefer_session_open: bool,
    /// User preference: keep inspector when wide enough.
    pub prefer_inspector_open: bool,
    pub session_width: f32,
    pub inspector_width: f32,
    /// Narrow sheet / collapsed inspector: which panel is shown.
    pub inspector_tab: InspectorTab,
    /// Close session sheet after next Live transition when opened from sheet.
    pub close_session_sheet_on_live: bool,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            breakpoint: LayoutBreakpoint::Wide,
            prefer_session_open: true,
            prefer_inspector_open: true,
            session_width: SESSION_DEFAULT_WIDTH,
            inspector_width: INSPECTOR_DEFAULT_WIDTH,
            inspector_tab: InspectorTab::Agents,
            close_session_sheet_on_live: false,
        }
    }
}

impl LayoutState {
    /// Update breakpoint from window width without clobbering user prefs.
    pub fn sync_breakpoint(&mut self, width_px: f32) {
        self.breakpoint = LayoutBreakpoint::from_width(width_px);
    }

    pub fn show_session_pane(&self) -> bool {
        match self.breakpoint {
            LayoutBreakpoint::Wide | LayoutBreakpoint::Medium => self.prefer_session_open,
            LayoutBreakpoint::Narrow => false,
        }
    }

    pub fn show_inspector_pane(&self, session_live: bool) -> bool {
        if !session_live {
            return false;
        }
        match self.breakpoint {
            LayoutBreakpoint::Wide => self.prefer_inspector_open,
            LayoutBreakpoint::Medium | LayoutBreakpoint::Narrow => false,
        }
    }

    pub fn toggle_session_pref(&mut self) {
        self.prefer_session_open = !self.prefer_session_open;
    }

    pub fn toggle_inspector_pref(&mut self) {
        self.prefer_inspector_open = !self.prefer_inspector_open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints() {
        assert_eq!(LayoutBreakpoint::from_width(1300.), LayoutBreakpoint::Wide);
        assert_eq!(
            LayoutBreakpoint::from_width(1000.),
            LayoutBreakpoint::Medium
        );
        assert_eq!(LayoutBreakpoint::from_width(800.), LayoutBreakpoint::Narrow);
    }

    #[test]
    fn medium_hides_inspector_keeps_session_pref() {
        let mut s = LayoutState::default();
        s.sync_breakpoint(1000.);
        assert!(s.show_session_pane());
        assert!(!s.show_inspector_pane(true));
        s.prefer_session_open = false;
        assert!(!s.show_session_pane());
    }

    #[test]
    fn wide_respects_user_inspector_pref() {
        let mut s = LayoutState::default();
        s.sync_breakpoint(1400.);
        s.prefer_inspector_open = false;
        assert!(!s.show_inspector_pane(true));
        s.prefer_inspector_open = true;
        assert!(s.show_inspector_pane(true));
    }
}
