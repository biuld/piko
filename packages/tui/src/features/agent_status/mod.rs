//! Session agent instances — Select surface (`SurfaceId::Agents`, ComposerBand).
//!
//! Lists agents in the current session and switches the viewed agent (timeline
//! target). Compact status also projects into BottomBar chrome.

mod pointer;

use ratatui::{Frame, layout::Rect};

use piko_tui_layout::{Component, InteractionState, SurfacePanel};

use crate::{
    app::{HitId, QueueStatus},
    navigation::{SelectBandBudget, SurfaceId},
    theme::Theme,
    ui::components::{
        ACTIVE_MARKER, FAIL_GLYPH, IDLE_MARKER, SUCCESS_GLYPH,
        selectable_list::{
            ColumnCell, SelectableItem, SelectableList, minimal_row_regions, paint_row_hover,
            render_selectable_list_minimal,
        },
        spinner_glyph,
    },
};

/// Agent entry displayed in the panel.
#[derive(Clone)]
pub struct AgentEntry {
    pub agent_id: String,
    pub agent_instance_id: String,
    pub name: String,
    pub parent_agent_instance_id: Option<String>,
    pub lifecycle: piko_protocol::AgentInstanceLifecycle,
    pub activity: piko_protocol::AgentActivity,
    pub unread_report_count: u32,
    pub status: piko_protocol::AgentStatus,
}

/// AgentPanel state (maintained in AppState).
#[derive(Default)]
pub struct AgentPanelState {
    pub list: SelectableList<AgentEntry>,
    pub active_agent_instance_id: Option<String>,
    pub focus: bool,
    pub filter: String,
    /// Set only after an authoritative agent projection (reconcile / AgentList).
    pub agents_hydrated: bool,
}

#[derive(Clone, Copy)]
pub struct AgentPanelView<'a> {
    pub state: &'a AgentPanelState,
    /// Foreground projection for each agent_instance_id (parallel to list items).
    pub foreground: &'a [piko_protocol::AgentForeground],
    pub queue: &'a QueueStatus,
    pub spinner_frame: usize,
    pub theme: &'a Theme,
}

impl Component<HitId, AgentPanelView<'_>> for AgentPanelState {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &AgentPanelView<'_>) {
        AgentPanelState::render(frame, area, *ctx);
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &AgentPanelView<'_>,
        interaction: InteractionState<HitId>,
    ) {
        AgentPanelState::render(frame, area, *ctx);
        if !self.is_loading() {
            let items = self.hit_items();
            let regions =
                minimal_row_regions(area, "agents", &items, self.list.selected, &self.filter);
            paint_row_hover(frame, &regions, interaction, self.list.selected, ctx.theme);
        }
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        if self.is_loading() {
            return Vec::new();
        }
        let items = self.hit_items();
        minimal_row_regions(area, "agents", &items, self.list.selected, &self.filter)
            .into_iter()
            .map(|(rect, i)| (rect, HitId::Row(i)))
            .collect()
    }
}

impl SurfacePanel<SurfaceId, HitId, AgentPanelView<'_>> for AgentPanelState {
    fn region(&self) -> SurfaceId {
        SurfaceId::Agents
    }
}

impl AgentPanelState {
    fn hit_items(&self) -> Vec<SelectableItem> {
        self.list
            .items
            .iter()
            .map(|agent| {
                SelectableItem::columns([
                    ColumnCell::primary(format!(
                        "{} {} {}",
                        agent.name, agent.agent_id, agent.agent_instance_id
                    )),
                    ColumnCell::secondary(""),
                ])
            })
            .collect()
    }
    pub fn agents(&self) -> &[AgentEntry] {
        &self.list.items
    }

    pub fn is_loading(&self) -> bool {
        !self.agents_hydrated
    }

    pub fn mark_hydrated(&mut self) {
        self.agents_hydrated = true;
    }

    pub fn begin_loading(&mut self) {
        self.list.clear();
        self.active_agent_instance_id = None;
        self.filter.clear();
        self.agents_hydrated = false;
    }

    /// ComposerBand content-row budget (dense single-line rows).
    pub fn select_band_budget(&self) -> SelectBandBudget {
        if self.is_loading() {
            return SelectBandBudget::minimal_dense_list(1);
        }
        SelectBandBudget::minimal_dense_list(
            self.list
                .filtered_indices(&self.filter, |a| agent_matches(a, &self.filter))
                .len(),
        )
    }

    /// Align keyboard selection with the currently viewed agent before open.
    pub fn prepare_for_switch(&mut self) {
        self.filter.clear();
        if let Some(active) = self.active_agent_instance_id.as_deref()
            && let Some(idx) = self
                .list
                .items
                .iter()
                .position(|a| a.agent_instance_id == active)
        {
            self.list.selected = idx;
        } else if self.list.selected >= self.list.len() {
            self.list.selected = 0;
        }
    }

    pub fn render(frame: &mut Frame<'_>, area: Rect, view: AgentPanelView<'_>) {
        if view.state.is_loading() {
            let line =
                crate::ui::components::feedback::loading_line(view.spinner_frame, view.theme);
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let items = vec![SelectableItem::columns([
                ColumnCell::primary(text),
                ColumnCell::secondary("waiting for session agent list"),
            ])];
            render_selectable_list_minimal(
                frame,
                area,
                "agents",
                &items,
                0,
                &view.state.filter,
                view.state.focus,
                view.theme,
            );
            return;
        }

        let items = build_selectable_items(&view);
        render_selectable_list_minimal(
            frame,
            area,
            "agents",
            &items,
            view.state.list.selected,
            &view.state.filter,
            view.state.focus,
            view.theme,
        );
    }

    pub fn select_next(&mut self) {
        let filter = self.filter.clone();
        self.list
            .select_next(&filter, |a| agent_matches(a, &filter));
    }

    pub fn select_prev(&mut self) {
        let filter = self.filter.clone();
        self.list
            .select_prev(&filter, |a| agent_matches(a, &filter));
    }

    pub fn reset_selection(&mut self) {
        let filter = self.filter.clone();
        self.list
            .reset_selection(&filter, |a| agent_matches(a, &filter));
    }

    pub fn selected_agent(&self) -> Option<&AgentEntry> {
        self.list.selected_item()
    }

    pub fn upsert_agent(&mut self, agent: AgentEntry) {
        if let Some(existing) = self
            .list
            .items
            .iter_mut()
            .find(|a| a.agent_instance_id == agent.agent_instance_id)
        {
            existing.agent_id = agent.agent_id;
            existing.name = agent.name;
            if agent.parent_agent_instance_id.is_some() {
                existing.parent_agent_instance_id = agent.parent_agent_instance_id;
            }
            existing.status = agent.status;
            existing.lifecycle = agent.lifecycle;
            existing.activity = agent.activity;
            existing.unread_report_count = agent.unread_report_count;
        } else {
            self.list.items.push(agent);
        }
    }
}

fn agent_matches(agent: &AgentEntry, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let q = filter.to_lowercase();
    agent.name.to_lowercase().contains(&q)
        || agent.agent_id.to_lowercase().contains(&q)
        || agent.agent_instance_id.to_lowercase().contains(&q)
}

fn build_selectable_items(view: &AgentPanelView<'_>) -> Vec<SelectableItem> {
    if view.state.list.is_empty() {
        return Vec::new();
    }

    let prefixes = build_tree_prefixes(view.state.agents());
    let queue_note = queue_detail(view.queue, view.foreground);

    view.state
        .list
        .items
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let is_active =
                view.state.active_agent_instance_id.as_deref() == Some(&agent.agent_instance_id);
            let foreground = view
                .foreground
                .get(i)
                .copied()
                .unwrap_or(piko_protocol::AgentForeground::Idle);
            let (glyph, status_label) =
                status_label(agent, is_active, foreground, view.spinner_frame);
            let primary = format!("{}{glyph} {}", prefixes[i], agent.name);
            let mut detail = status_label;
            if !queue_note.is_empty() && is_active {
                detail = if detail.is_empty() {
                    queue_note.clone()
                } else {
                    format!("{detail} · {queue_note}")
                };
            }
            SelectableItem::columns([ColumnCell::primary(primary), ColumnCell::secondary(detail)])
                .active(is_active)
        })
        .collect()
}

fn status_label(
    agent: &AgentEntry,
    is_active: bool,
    foreground: piko_protocol::AgentForeground,
    frame_idx: usize,
) -> (&'static str, String) {
    let (glyph, mut parts) = match foreground {
        piko_protocol::AgentForeground::Running | piko_protocol::AgentForeground::Cancelling => {
            (spinner_glyph(frame_idx), vec!["running".into()])
        }
        piko_protocol::AgentForeground::RequiresAction => {
            (ACTIVE_MARKER, vec!["needs action".into()])
        }
        piko_protocol::AgentForeground::Queued => (IDLE_MARKER, vec!["queued".into()]),
        piko_protocol::AgentForeground::Idle => match agent.status {
            piko_protocol::AgentStatus::Running => (ACTIVE_MARKER, vec!["busy".into()]),
            piko_protocol::AgentStatus::Completed => (SUCCESS_GLYPH, vec!["done".into()]),
            piko_protocol::AgentStatus::Failed => (FAIL_GLYPH, vec!["failed".into()]),
            piko_protocol::AgentStatus::Cancelled => (FAIL_GLYPH, vec!["cancelled".into()]),
            piko_protocol::AgentStatus::Closed => (FAIL_GLYPH, vec!["closed".into()]),
            _ if is_active => (ACTIVE_MARKER, Vec::new()),
            _ => (IDLE_MARKER, Vec::new()),
        },
    };

    match agent.lifecycle {
        piko_protocol::AgentInstanceLifecycle::Open => {}
        piko_protocol::AgentInstanceLifecycle::Closed => parts.push("lifecycle closed".into()),
        piko_protocol::AgentInstanceLifecycle::Terminated => {
            parts.push("lifecycle terminated".into())
        }
        piko_protocol::AgentInstanceLifecycle::Unavailable => {
            parts.push("lifecycle unavailable".into())
        }
    }
    if agent.unread_report_count > 0 {
        parts.push(format!("+{} report", agent.unread_report_count));
    }
    if !agent.agent_id.is_empty() && agent.agent_id != agent.name {
        parts.push(agent.agent_id.clone());
    }

    (glyph, parts.join(" · "))
}

fn queue_detail(queue: &QueueStatus, _foreground: &[piko_protocol::AgentForeground]) -> String {
    let mut parts = Vec::new();
    if queue.steer_count > 0 {
        parts.push(format!("{} steer", queue.steer_count));
    }
    let queued = queue.follow_up_count.saturating_add(queue.next_turn_count);
    if queued > 0 {
        parts.push(format!("{queued} queued"));
    }
    parts.join(" · ")
}

// ── tree prefix builder ──────────────────────────────────────────────────────

fn build_tree_prefixes(agents: &[AgentEntry]) -> Vec<String> {
    let n = agents.len();
    let mut prefixes = Vec::with_capacity(n);

    for i in 0..n {
        let agent = &agents[i];
        let Some(parent_id) = agent.parent_agent_instance_id.as_deref() else {
            prefixes.push(String::new());
            continue;
        };

        let mut ancestor_ids: Vec<String> = Vec::new();
        let mut current = Some(parent_id.to_string());
        while let Some(id) = current.take() {
            ancestor_ids.push(id.clone());
            current = agents[..i]
                .iter()
                .find(|a| a.agent_instance_id == id)
                .and_then(|a| a.parent_agent_instance_id.clone());
        }

        let mut s = String::new();
        // The nearest parent owns this row's connector. Only ancestors above
        // it contribute gutter columns, and every tree column is three cells
        // wide so gutters stay aligned with the connector below them.
        for anc_id in ancestor_ids.iter().skip(1).rev() {
            let continues = agents[i + 1..]
                .iter()
                .any(|a| a.parent_agent_instance_id.as_deref() == Some(anc_id));
            if continues {
                s.push_str("│  ");
            } else {
                s.push_str("   ");
            }
        }

        let is_last = !agents[i + 1..]
            .iter()
            .any(|a| a.parent_agent_instance_id.as_deref() == Some(parent_id));
        if is_last {
            s.push_str("└─ ");
        } else {
            s.push_str("├─ ");
        }

        prefixes.push(s);
    }

    prefixes
}

#[cfg(test)]
mod tests;
