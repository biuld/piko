//! Read-only Session Trajectory browser (F-52 / D-69).

mod detail;
mod layout;
mod pointer;
mod present;
mod render;
pub(crate) mod rows;
#[cfg(test)]
mod tests;

use crate::ui::components::split_pane::{PaneSide, SplitPanePlan};
use piko_protocol::{
    HistoryAgentSummary, HistoryItemDetail, HistoryLaneSummary, HistoryStreamItem,
    HistoryStreamPage, SessionHistoryOverview,
};
use piko_tui_layout::ViewportState;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

const DETAIL_CACHE_LIMIT: usize = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DetailTab {
    #[default]
    Summary,
    Payload,
    Result,
    Timing,
}

impl DetailTab {
    pub const ALL: [Self; 4] = [Self::Summary, Self::Payload, Self::Result, Self::Timing];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL[index.min(Self::ALL.len() - 1)]
    }

    pub fn cycle(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }
}

#[derive(Clone)]
pub enum HistoryRow {
    Session(piko_protocol::SessionSummary),
    Agent {
        agent: HistoryAgentSummary,
        depth: u32,
    },
    Stream(HistoryStreamItem),
}

#[derive(Default)]
pub struct HistoryPanel {
    pub session_id: Option<String>,
    pub overview: Option<SessionHistoryOverview>,
    pub agent_id: Option<String>,
    pub stream: Option<HistoryStreamPage>,
    pub lanes: Option<HistoryLaneSummary>,
    pub detail: Option<HistoryItemDetail>,
    /// Details opened at the current inspected revision, keyed by item token.
    pub detail_cache: HashMap<String, HistoryItemDetail>,
    pub detail_tab: DetailTab,
    pub filter: String,
    pub filter_editing: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// Correlation ids of in-flight history requests (stream + lane + detail).
    pub pending_commands: Vec<String>,
    pub choosing_session: bool,
    pub agent_choosing: bool,
    pub sessions: Vec<piko_protocol::SessionSummary>,
    pub selected: usize,
    pub(super) viewport: Cell<ViewportState>,
    pub last_width: Cell<u16>,
    pub active_pane: PaneSide,
    pub(super) detail_viewport: Cell<ViewportState>,
    /// Horizontal viewport over the lane strip when blocks overflow its width;
    /// `lane_unit` is the native columns per sequence unit (0 = fit mode).
    pub(super) lane_viewport: Cell<ViewportState>,
    pub(super) lane_unit: Cell<u16>,
    pub(super) lane_strip_rect: Cell<Option<ratatui::layout::Rect>>,
    detail_render_cache: RefCell<Option<detail::CachedDetail>>,
    pub(super) painted_regions: RefCell<Vec<(ratatui::layout::Rect, crate::app::HitId)>>,
    pub(super) painted_lane_blocks: RefCell<Vec<(ratatui::layout::Rect, usize)>>,
    pub(super) wide: Cell<bool>,
    pub(super) painted_split: Cell<Option<SplitPanePlan>>,
    pub detail_loading: bool,
    pub detail_error: Option<String>,
    /// Newest wanted row while a detail fetch is already in flight; served
    /// immediately when the in-flight response lands.
    pub detail_queued: Option<piko_protocol::HistoryItemRef>,
    pub opened_row: Option<HistoryRow>,
}

pub struct HistoryCtx<'a> {
    pub theme: &'a crate::theme::Theme,
    pub hints: Option<&'a str>,
}

impl HistoryPanel {
    pub fn begin(&mut self, session_id: String) {
        *self = Self {
            session_id: Some(session_id),
            loading: true,
            ..Self::default()
        };
    }

    pub fn is_wide(&self) -> bool {
        self.wide.get() && !self.choosing_session && !self.agent_choosing
    }

    pub fn set_overview(&mut self, overview: SessionHistoryOverview) {
        self.session_id = Some(overview.session_id.clone());
        self.overview = Some(overview);
        self.loading = false;
        self.error = None;
        self.clamp_selection();
    }

    pub fn set_stream(&mut self, page: HistoryStreamPage) {
        if let Some(current) = &mut self.stream
            && current.agent_instance_id == page.agent_instance_id
            && current.revision == page.revision
        {
            current.items.extend(page.items);
            current.next_cursor = page.next_cursor;
            self.loading = false;
            return;
        }
        self.stream = Some(page);
        self.selected = 0;
        self.loading = false;
        self.error = None;
        self.clamp_selection();
    }

    pub fn set_lanes(&mut self, lanes: HistoryLaneSummary) {
        self.lanes = Some(lanes);
    }

    pub fn set_detail(&mut self, detail: HistoryItemDetail) {
        self.cache_detail(detail.clone());
        self.detail_viewport.get_mut().scroll_to(0);
        self.detail = Some(detail);
        self.detail_loading = false;
        self.detail_error = None;
        if !self.is_wide() {
            self.active_pane = PaneSide::Second;
        }
        self.loading = false;
        self.error = None;
    }

    pub(crate) fn cache_detail(&mut self, detail: HistoryItemDetail) {
        if self.detail_cache.len() >= DETAIL_CACHE_LIMIT
            && !self.detail_cache.contains_key(&detail.item_ref.token)
            && let Some(oldest) = self.detail_cache.keys().next().cloned()
        {
            self.detail_cache.remove(&oldest);
        }
        self.detail_cache
            .insert(detail.item_ref.token.clone(), detail);
    }

    pub fn selected_item_ref(&self) -> Option<piko_protocol::HistoryItemRef> {
        self.stream_item_at(self.selected)
            .filter(|item| item.has_detail)
            .map(|item| item.item_ref.clone())
    }

    pub fn select_next(&mut self) {
        if self.active_pane == PaneSide::Second {
            self.detail_viewport.get_mut().scroll_by(1);
            return;
        }
        self.selected = (self.selected + 1).min(self.row_count().saturating_sub(1));
        self.reveal_lane_block();
    }

    pub fn select_prev(&mut self) {
        if self.active_pane == PaneSide::Second {
            self.detail_viewport.get_mut().scroll_by(-1);
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.reveal_lane_block();
    }

    pub fn cycle_detail_tab(&mut self) -> DetailTab {
        self.detail_tab = self.detail_tab.cycle();
        self.detail_tab
    }

    pub fn select_detail_tab(&mut self, index: usize) {
        self.detail_tab = DetailTab::from_index(index);
    }

    /// Enter the agent picker mode; rows switch to the nested agent list.
    pub fn start_agent_choosing(&mut self) {
        self.agent_choosing = true;
        self.selected = 0;
    }

    pub fn select_agent(&mut self, agent_id: String) {
        self.agent_choosing = false;
        if self.agent_id.as_deref() == Some(agent_id.as_str()) {
            return;
        }
        self.agent_id = Some(agent_id);
        self.stream = None;
        self.lanes = None;
        self.selected = 0;
        self.clear_detail();
    }

    /// Returns true when the caller should close the surface.
    pub fn back(&mut self) -> bool {
        self.pending_commands.clear();
        self.loading = false;
        if self.filter_editing || !self.filter.is_empty() {
            self.filter_editing = false;
            self.filter.clear();
            return false;
        }
        if self.agent_choosing {
            self.agent_choosing = false;
            self.selected = 0;
            return false;
        }
        if self.active_pane == PaneSide::Second || self.detail_loading {
            self.clear_detail();
            return false;
        }
        true
    }

    pub fn row_count(&self) -> usize {
        self.visible_row_count()
    }

    pub fn shows_detail_only(&self) -> bool {
        self.active_pane == PaneSide::Second && !self.is_wide()
    }

    #[cfg(test)]
    pub fn has_more(&self) -> bool {
        self.stream
            .as_ref()
            .is_some_and(|page| page.next_cursor.is_some())
    }

    pub fn clear_detail(&mut self) {
        self.detail = None;
        self.detail_queued = None;
        self.detail_loading = false;
        self.detail_error = None;
        self.active_pane = PaneSide::First;
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.row_count().saturating_sub(1));
    }

    pub(super) fn breadcrumb(&self) -> String {
        if self.choosing_session {
            return "Select a session to inspect".into();
        }
        if self.agent_choosing {
            return "Select an agent stream".into();
        }
        let name = self
            .overview
            .as_ref()
            .and_then(|overview| overview.name.clone())
            .filter(|name| !name.is_empty())
            .or_else(|| self.session_id.clone())
            .unwrap_or_else(|| "Session".into());
        let agent = self
            .overview
            .as_ref()
            .and_then(|overview| {
                let id = self.agent_id.as_deref()?;
                overview
                    .agents
                    .iter()
                    .find(|agent| agent.agent_instance_id == id)
                    .map(|agent| agent.agent_spec_id.clone())
            })
            .unwrap_or_else(|| {
                self.agent_id
                    .as_deref()
                    .map(crate::features::short_id)
                    .unwrap_or_else(|| "no agent".into())
            });
        let mut parts = vec![name, agent];
        if let Some(revision) = self.overview.as_ref().map(|overview| overview.revision) {
            parts.push(format!("rev {revision}"));
        }
        parts.join(" · ")
    }
}
