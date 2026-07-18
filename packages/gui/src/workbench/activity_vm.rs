//! Activity Center view-model: bounded operational surface between Timeline and Composer.

use piko_client_core::{
    AttentionItem, AttentionKind, ClientState, ConnectionState, LiveSession, TimelineItem,
    ToolStatus, prompt_queue_from_state,
};
use piko_protocol::TurnStatus;
use piko_protocol::messages::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityItemKind {
    Approval,
    Interaction,
    TurnRunning,
    TurnQueued,
    ToolRunning,
    ToolFailed,
    Warning,
    UnreadReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityItem {
    pub id: String,
    pub kind: ActivityItemKind,
    pub label: String,
    pub agent_instance_id: Option<String>,
    /// When set, activating focuses this prompt in the coordinator.
    pub prompt_id: Option<String>,
    pub prompt_kind: Option<AttentionKind>,
    pub actionable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActivityViewModel {
    pub summary: String,
    pub show_stop: bool,
    pub items: Vec<ActivityItem>,
    /// Expand unless the user explicitly collapsed while this fingerprint held.
    pub prefer_expanded: bool,
    pub has_actionable: bool,
}

pub fn derive_activity(state: &ClientState) -> ActivityViewModel {
    let mut items = Vec::new();
    let mut show_stop = false;

    if state.shell.connection == ConnectionState::Disconnected {
        items.push(ActivityItem {
            id: "disconnect".into(),
            kind: ActivityItemKind::Warning,
            label: "Host disconnected".into(),
            agent_instance_id: None,
            prompt_id: None,
            prompt_kind: None,
            actionable: true,
        });
    }

    if let Some(err) = state.last_error.as_ref() {
        items.push(ActivityItem {
            id: "last-error".into(),
            kind: ActivityItemKind::Warning,
            label: format!("Error: {err}"),
            agent_instance_id: None,
            prompt_id: None,
            prompt_kind: None,
            actionable: true,
        });
    }

    for prompt in prompt_queue_from_state(state) {
        items.push(activity_from_prompt(&prompt));
    }

    if let Some(session) = state.live_session.as_ref() {
        let selected = session.selected_agent.clone();

        for turn in &session.active_turns {
            match turn.status {
                TurnStatus::Queued => {
                    items.push(ActivityItem {
                        id: format!("turn-{}", turn.turn_id),
                        kind: ActivityItemKind::TurnQueued,
                        label: format!(
                            "Turn queued ({})",
                            short_agent(session, &turn.agent_instance_id)
                        ),
                        agent_instance_id: Some(turn.agent_instance_id.clone()),
                        prompt_id: None,
                        prompt_kind: None,
                        actionable: false,
                    });
                }
                TurnStatus::Running | TurnStatus::WaitingForApproval | TurnStatus::Cancelling => {
                    let status_label = match turn.status {
                        TurnStatus::WaitingForApproval => "waiting for approval",
                        TurnStatus::Cancelling => "cancelling",
                        _ => "running",
                    };
                    items.push(ActivityItem {
                        id: format!("turn-{}", turn.turn_id),
                        kind: ActivityItemKind::TurnRunning,
                        label: format!(
                            "{}: {status_label}",
                            short_agent(session, &turn.agent_instance_id)
                        ),
                        agent_instance_id: Some(turn.agent_instance_id.clone()),
                        prompt_id: None,
                        prompt_kind: None,
                        actionable: false,
                    });
                }
                TurnStatus::Failed => {
                    items.push(ActivityItem {
                        id: format!("turn-fail-{}", turn.turn_id),
                        kind: ActivityItemKind::Warning,
                        label: format!(
                            "Turn failed ({})",
                            short_agent(session, &turn.agent_instance_id)
                        ),
                        agent_instance_id: Some(turn.agent_instance_id.clone()),
                        prompt_id: None,
                        prompt_kind: None,
                        actionable: true,
                    });
                }
                _ => {}
            }
        }

        for failure in &session.turn_failures {
            items.push(ActivityItem {
                id: format!("turn-fail-{}", failure.turn_id),
                kind: ActivityItemKind::Warning,
                label: format!(
                    "{} failed: {}",
                    short_agent(session, &failure.agent_instance_id),
                    failure.error
                ),
                agent_instance_id: Some(failure.agent_instance_id.clone()),
                prompt_id: None,
                prompt_kind: None,
                actionable: true,
            });
        }

        let queued = session.queue.steer_count
            + session.queue.follow_up_count
            + session.queue.next_turn_count;
        if queued > 0 {
            items.push(ActivityItem {
                id: "host-queue".into(),
                kind: ActivityItemKind::TurnQueued,
                label: format!("{queued} queued item(s)"),
                agent_instance_id: None,
                prompt_id: None,
                prompt_kind: None,
                actionable: false,
            });
        }

        show_stop = session.active_turns.iter().any(|t| {
            selected.as_ref() == Some(&t.agent_instance_id)
                && matches!(
                    t.status,
                    TurnStatus::Queued
                        | TurnStatus::Running
                        | TurnStatus::WaitingForApproval
                        | TurnStatus::Cancelling
                )
        });

        for agent in &session.agents {
            if agent.unread_report_count > 0 {
                items.push(ActivityItem {
                    id: format!("unread-{}", agent.agent_instance_id),
                    kind: ActivityItemKind::UnreadReport,
                    label: format!(
                        "{}: {} unread report(s)",
                        agent.name, agent.unread_report_count
                    ),
                    agent_instance_id: Some(agent.agent_instance_id.clone()),
                    prompt_id: None,
                    prompt_kind: None,
                    actionable: true,
                });
            }
        }

        for (agent_id, timeline) in &session.timelines {
            for item in timeline.items() {
                if let TimelineItem::Tool(tool) = item {
                    let (kind, label, actionable) = match tool.status {
                        ToolStatus::Running => (
                            ActivityItemKind::ToolRunning,
                            format!("Tool running: {}", tool.tool_name),
                            false,
                        ),
                        ToolStatus::Failed => (
                            ActivityItemKind::ToolFailed,
                            format!("Tool failed: {}", tool.tool_name),
                            true,
                        ),
                        ToolStatus::Completed => continue,
                    };
                    items.push(ActivityItem {
                        id: format!("tool-{}", tool.tool_call_id),
                        kind,
                        label,
                        agent_instance_id: Some(agent_id.clone()),
                        prompt_id: None,
                        prompt_kind: None,
                        actionable,
                    });
                }
            }
            let open_calls = open_tool_calls(timeline.items());
            for (call_id, name) in open_calls {
                items.push(ActivityItem {
                    id: format!("tool-run-{call_id}"),
                    kind: ActivityItemKind::ToolRunning,
                    label: format!("Tool running: {name}"),
                    agent_instance_id: Some(agent_id.clone()),
                    prompt_id: None,
                    prompt_kind: None,
                    actionable: false,
                });
            }
            for (call_id, name) in failed_tool_results(timeline.items()) {
                items.push(ActivityItem {
                    id: format!("tool-fail-{call_id}"),
                    kind: ActivityItemKind::ToolFailed,
                    label: format!("Tool failed: {name}"),
                    agent_instance_id: Some(agent_id.clone()),
                    prompt_id: None,
                    prompt_kind: None,
                    actionable: true,
                });
            }
        }
    }

    let has_actionable = items.iter().any(|i| i.actionable);
    let prefer_expanded = has_actionable;
    let summary = summarize(&items, state);

    ActivityViewModel {
        summary,
        show_stop,
        items,
        prefer_expanded,
        has_actionable,
    }
}

fn activity_from_prompt(prompt: &AttentionItem) -> ActivityItem {
    let kind = match prompt.kind {
        AttentionKind::Approval => ActivityItemKind::Approval,
        AttentionKind::Interaction => ActivityItemKind::Interaction,
    };
    ActivityItem {
        id: format!("prompt-{}", prompt.id),
        kind,
        label: prompt.summary.clone(),
        agent_instance_id: Some(prompt.agent_instance_id.clone()),
        prompt_id: Some(prompt.id.clone()),
        prompt_kind: Some(prompt.kind),
        actionable: true,
    }
}

fn short_agent(session: &LiveSession, agent_id: &str) -> String {
    session
        .agents
        .iter()
        .find(|a| a.agent_instance_id == agent_id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| agent_id.to_string())
}

fn summarize(items: &[ActivityItem], state: &ClientState) -> String {
    if items.is_empty() {
        if let Some(session) = state.live_session.as_ref()
            && let Some(agent_id) = session.selected_agent.as_ref()
        {
            let name = short_agent(session, agent_id);
            return format!("{name}: idle");
        }
        return "Idle".into();
    }

    let approvals = items
        .iter()
        .filter(|i| i.kind == ActivityItemKind::Approval)
        .count();
    let interactions = items
        .iter()
        .filter(|i| i.kind == ActivityItemKind::Interaction)
        .count();
    let running = items
        .iter()
        .filter(|i| {
            matches!(
                i.kind,
                ActivityItemKind::TurnRunning | ActivityItemKind::ToolRunning
            )
        })
        .count();
    let queued = items
        .iter()
        .filter(|i| i.kind == ActivityItemKind::TurnQueued)
        .count();
    let warnings = items
        .iter()
        .filter(|i| {
            matches!(
                i.kind,
                ActivityItemKind::Warning | ActivityItemKind::ToolFailed
            )
        })
        .count();

    let mut parts = Vec::new();
    if approvals > 0 {
        parts.push(format!(
            "{approvals} approval{}",
            if approvals == 1 { "" } else { "s" }
        ));
    }
    if interactions > 0 {
        parts.push(format!(
            "{interactions} interaction{}",
            if interactions == 1 { "" } else { "s" }
        ));
    }
    if running > 0 {
        parts.push(format!("{running} running"));
    }
    if queued > 0 {
        parts.push(format!("{queued} queued"));
    }
    if warnings > 0 {
        parts.push(format!(
            "{warnings} warning{}",
            if warnings == 1 { "" } else { "s" }
        ));
    }
    if parts.is_empty() {
        format!("{} item(s)", items.len())
    } else {
        parts.join(" · ")
    }
}

fn open_tool_calls(items: &[TimelineItem]) -> Vec<(String, String)> {
    let mut open: Vec<(String, String)> = Vec::new();
    let mut done = std::collections::HashSet::new();
    for item in items {
        match item {
            TimelineItem::Committed(c) => match &c.message {
                Message::ToolCall { id, name, .. } => {
                    open.push((id.clone(), name.clone()));
                }
                Message::ToolResult { tool_call_id, .. } => {
                    done.insert(tool_call_id.clone());
                }
                _ => {}
            },
            TimelineItem::RealtimeDraft(_) => {}
            TimelineItem::Tool(_) => {}
        }
    }
    open.into_iter()
        .filter(|(id, _)| !done.contains(id))
        .collect()
}

fn failed_tool_results(items: &[TimelineItem]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for item in items {
        if let TimelineItem::Committed(c) = item
            && let Message::ToolResult {
                tool_call_id,
                tool_name,
                is_error: Some(true),
                ..
            } = &c.message
        {
            out.push((
                tool_call_id.clone(),
                tool_name.clone().unwrap_or_else(|| "tool".into()),
            ));
        }
    }
    out
}
