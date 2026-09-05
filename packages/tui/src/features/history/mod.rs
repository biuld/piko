//! Read-only Session History browser (F-52 / D-69).

mod detail;
mod layout;
mod pointer;
mod present;
mod render;
mod rows;
#[cfg(test)]
mod tests;

use crate::ui::components::split_pane::{PaneSide, SplitPanePlan};
use piko_protocol::{
    HistoryAgentSummary, HistoryItemDetail, HistoryItemSummary, HistoryJournalPage,
    HistoryProvenanceFilter, HistoryTranscriptItem, HistoryTranscriptPage, HistoryWorkPage,
    HistoryWorkSummary, SessionHistoryOverview,
};
use piko_tui_layout::ViewportState;
use std::cell::{Cell, RefCell};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HistoryLens {
    #[default]
    Work,
    Agents,
    Transcript,
    Journal,
}

impl HistoryLens {
    pub const ALL: [Self; 4] = [Self::Work, Self::Agents, Self::Transcript, Self::Journal];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|lens| *lens == self).unwrap_or(0)
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL[index.min(Self::ALL.len() - 1)]
    }
}

#[derive(Clone)]
pub enum HistoryRow {
    Session(piko_protocol::SessionSummary),
    Work(HistoryWorkSummary),
    Agent {
        agent: HistoryAgentSummary,
        depth: u32,
    },
    Item {
        item: HistoryItemSummary,
        depth: u32,
    },
    Transcript(HistoryTranscriptItem),
    CommitHeader {
        revision: u64,
        producer: String,
        events: usize,
        committed_at: i64,
    },
}

#[derive(Default)]
pub struct HistoryPanel {
    pub session_id: Option<String>,
    pub overview: Option<SessionHistoryOverview>,
    pub work: Option<HistoryWorkPage>,
    pub journal: Option<HistoryJournalPage>,
    pub transcript: Option<HistoryTranscriptPage>,
    pub detail: Option<HistoryItemDetail>,
    pub lens: HistoryLens,
    pub selected: usize,
    pub agent_id: Option<String>,
    pub filter: String,
    pub filter_editing: bool,
    pub provenance: HistoryProvenanceFilter,
    pub loading: bool,
    pub error: Option<String>,
    pub pending_command_id: Option<String>,
    pub choosing_session: bool,
    pub sessions: Vec<piko_protocol::SessionSummary>,
    pub(super) viewport: Cell<ViewportState>,
    pub last_width: Cell<u16>,
    pub active_pane: PaneSide,
    pub(super) detail_viewport: Cell<ViewportState>,
    pub(super) painted_regions: RefCell<Vec<(ratatui::layout::Rect, crate::app::HitId)>>,
    pub(super) wide: Cell<bool>,
    pub(super) painted_split: Cell<Option<SplitPanePlan>>,
    pub(super) list_stack: Vec<(usize, ViewportState)>,
    pub detail_loading: bool,
    pub detail_error: Option<String>,
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
        self.wide.get() && !self.choosing_session
    }

    pub fn set_overview(&mut self, overview: SessionHistoryOverview) {
        if let Some(current) = &mut self.overview
            && current.session_id == overview.session_id
            && current.revision == overview.revision
        {
            current.works.extend(overview.works);
            current.next_cursor = overview.next_cursor;
            self.loading = false;
            return;
        }
        self.session_id = Some(overview.session_id.clone());
        self.overview = Some(overview);
        self.loading = false;
        self.error = None;
        self.clamp_selection();
    }

    pub fn set_work(&mut self, page: HistoryWorkPage) {
        if let Some(current) = &mut self.work
            && current.root_input_id == page.root_input_id
            && current.revision == page.revision
        {
            current.items.extend(page.items);
            current.next_cursor = page.next_cursor;
            self.loading = false;
            return;
        }
        self.list_stack.push((self.selected, self.viewport.get()));
        self.viewport.set(ViewportState::default());
        self.work = Some(page);
        self.clear_detail();
        self.selected = 0;
        self.loading = false;
        self.error = None;
    }

    pub fn set_journal(&mut self, page: HistoryJournalPage) {
        if let Some(current) = &mut self.journal
            && current.revision == page.revision
        {
            current.commits.extend(page.commits);
            current.next_cursor = page.next_cursor;
            self.loading = false;
            return;
        }
        self.journal = Some(page);
        self.selected = 0;
        self.loading = false;
        self.error = None;
    }

    pub fn set_transcript(&mut self, page: HistoryTranscriptPage) {
        if let Some(current) = &mut self.transcript
            && current.revision == page.revision
        {
            current.items.extend(page.items);
            current.next_cursor = page.next_cursor;
            self.loading = false;
            return;
        }
        self.transcript = Some(page);
        self.selected = 0;
        self.loading = false;
        self.error = None;
    }

    pub fn set_detail(&mut self, detail: HistoryItemDetail) {
        self.detail_viewport.get_mut().scroll_to(0);
        self.detail = Some(detail);
        self.detail_loading = false;
        self.detail_error = None;
        self.active_pane = PaneSide::Second;
        self.loading = false;
        self.error = None;
    }

    pub fn selected_work_id(&self) -> Option<String> {
        match self.visible_rows().get(self.selected)? {
            HistoryRow::Work(work) => Some(work.root_input_id.clone()),
            _ => None,
        }
    }

    pub fn selected_agent_id(&self) -> Option<String> {
        match self.visible_rows().get(self.selected)? {
            HistoryRow::Agent { agent, .. } => Some(agent.agent_instance_id.clone()),
            _ => None,
        }
    }

    pub fn selected_item_ref(&self) -> Option<piko_protocol::HistoryItemRef> {
        match self.visible_rows().get(self.selected)? {
            HistoryRow::Item { item, .. } if item.has_detail => Some(item.item_ref.clone()),
            HistoryRow::Transcript(item) if item.has_detail => Some(item.item_ref.clone()),
            _ => None,
        }
    }

    pub fn select_next(&mut self) {
        if self.active_pane == PaneSide::Second {
            self.detail_viewport.get_mut().scroll_by(1);
            return;
        }
        self.selected = (self.selected + 1).min(self.row_count().saturating_sub(1));
    }

    pub fn select_prev(&mut self) {
        if self.active_pane == PaneSide::Second {
            self.detail_viewport.get_mut().scroll_by(-1);
            return;
        }
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn cycle_lens(&mut self, backwards: bool) -> HistoryLens {
        self.pending_command_id = None;
        self.loading = false;
        self.clear_detail();
        let current = self.lens.index();
        let len = HistoryLens::ALL.len();
        self.lens = HistoryLens::ALL[if backwards {
            (current + len - 1) % len
        } else {
            (current + 1) % len
        }];
        self.selected = 0;
        self.list_stack.clear();
        self.viewport.set(ViewportState::default());
        self.agent_id = None;
        self.work = None;
        self.lens
    }

    pub fn select_lens(&mut self, index: usize) -> HistoryLens {
        self.lens = HistoryLens::from_index(index);
        self.pending_command_id = None;
        self.selected = 0;
        self.list_stack.clear();
        self.viewport.set(ViewportState::default());
        self.agent_id = None;
        self.work = None;
        self.clear_detail();
        self.lens
    }

    pub fn drill_into_agent(&mut self, agent_id: String) {
        self.list_stack.push((self.selected, self.viewport.get()));
        self.viewport.set(ViewportState::default());
        self.agent_id = Some(agent_id);
        self.selected = 0;
        self.work = None;
        self.clear_detail();
    }

    /// Returns true when the caller should close the surface.
    pub fn back(&mut self) -> bool {
        self.pending_command_id = None;
        self.loading = false;
        if self.filter_editing || !self.filter.is_empty() {
            self.filter_editing = false;
            self.filter.clear();
            return false;
        }
        if self.active_pane == PaneSide::Second || self.detail.is_some() || self.detail_loading {
            self.clear_detail();
            return false;
        }
        if self.work.take().is_some() {
            self.restore_list();
            return false;
        }
        if self.agent_id.take().is_some() {
            self.restore_list();
            return false;
        }
        true
    }

    pub fn row_count(&self) -> usize {
        self.visible_rows().len()
    }

    pub fn shows_detail_only(&self) -> bool {
        self.active_pane == PaneSide::Second && !self.is_wide()
    }

    fn restore_list(&mut self) {
        let (selected, viewport) = self.list_stack.pop().unwrap_or_default();
        self.selected = selected;
        self.viewport.set(viewport);
        self.clamp_selection();
    }

    pub fn has_more(&self) -> bool {
        match self.lens {
            HistoryLens::Work | HistoryLens::Agents => self
                .work
                .as_ref()
                .map(|page| page.next_cursor.is_some())
                .unwrap_or_else(|| {
                    self.overview
                        .as_ref()
                        .is_some_and(|page| page.next_cursor.is_some())
                }),
            HistoryLens::Transcript => self
                .transcript
                .as_ref()
                .is_some_and(|page| page.next_cursor.is_some()),
            HistoryLens::Journal => self
                .journal
                .as_ref()
                .is_some_and(|page| page.next_cursor.is_some()),
        }
    }

    pub fn inspect_summary(&mut self) {
        if self.detail_loading {
            self.pending_command_id = None;
        }
        self.clear_detail();
        self.opened_row = self.visible_rows().get(self.selected).cloned();
        if self.opened_row.is_some() {
            self.active_pane = PaneSide::Second;
            self.detail_viewport.set(ViewportState::default());
        }
    }

    pub fn clear_detail(&mut self) {
        self.detail = None;
        self.opened_row = None;
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
        let name = self
            .overview
            .as_ref()
            .and_then(|overview| overview.name.clone())
            .filter(|name| !name.is_empty())
            .or_else(|| self.session_id.clone())
            .unwrap_or_else(|| "Session".into());
        let lens = match self.lens {
            HistoryLens::Work => "Work",
            HistoryLens::Agents => "Agents",
            HistoryLens::Transcript => "Transcript",
            HistoryLens::Journal => "Journal",
        };
        let mut parts = vec![name, lens.to_string()];
        if let Some(page) = &self.work {
            parts.push(crate::ui::line_layout::truncate_cols(
                &page.root_input_id,
                16,
            ));
        } else if let Some(agent_id) = &self.agent_id {
            let spec = self.overview.as_ref().and_then(|overview| {
                overview
                    .agents
                    .iter()
                    .find(|agent| agent.agent_instance_id == *agent_id)
                    .map(|agent| agent.agent_spec_id.clone())
            });
            parts.push(spec.unwrap_or_else(|| crate::features::short_id(agent_id)));
        }
        match self.provenance {
            HistoryProvenanceFilter::All => parts.push("facts + diagnostics".into()),
            HistoryProvenanceFilter::Facts => parts.push("facts".into()),
            HistoryProvenanceFilter::Diagnostics => parts.push("diagnostics".into()),
        }
        if let Some(revision) = self.overview.as_ref().map(|overview| overview.revision) {
            parts.push(format!("rev {revision}"));
        }
        parts.join(" · ")
    }
}
