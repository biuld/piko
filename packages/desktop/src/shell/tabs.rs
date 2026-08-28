//! Bind host agents onto island `TabGroup` (F-43 / D-60).

use island::components::tabs::{TabBadge, TabItem};
use piko_client_core::{ClientState, agent_foreground};
use piko_protocol::{AgentForeground, AgentInstanceLifecycle};

use crate::connection::DesktopConnection;

/// In-flight select-agent id, else the host-selected agent.
pub fn view_key<'a>(
    pending_agent: Option<&'a str>,
    selected_agent: Option<&'a str>,
) -> Option<&'a str> {
    pending_agent.or(selected_agent)
}

pub fn tab_items(core: &ClientState, pending_agent: Option<&str>) -> Vec<TabItem<String>> {
    let Some(session) = core.live_session.as_ref() else {
        return Vec::new();
    };
    if session.agents.is_empty() {
        let Some(id) = view_key(pending_agent, session.selected_agent.as_deref()) else {
            return Vec::new();
        };
        return vec![TabItem::new(id.to_string(), "Main")];
    }
    let view = view_key(pending_agent, session.selected_agent.as_deref());
    let labels = display_labels(session);
    session
        .agents
        .iter()
        .enumerate()
        .map(|(index, agent)| {
            let label = labels[index].clone();
            let muted = matches!(
                agent.lifecycle,
                AgentInstanceLifecycle::Closed
                    | AgentInstanceLifecycle::Terminated
                    | AgentInstanceLifecycle::Unavailable
            );
            let selected = view == Some(agent.agent_instance_id.as_str());
            TabItem::new(agent.agent_instance_id.clone(), label)
                .badge(tab_badge(core, &agent.agent_instance_id, selected))
                .muted(muted)
        })
        .collect()
}

pub fn tabs_disabled(connection: DesktopConnection, overlay_open: bool) -> bool {
    connection != DesktopConnection::Live || overlay_open
}

pub fn agent_label(core: &ClientState, agent_instance_id: &str) -> String {
    let Some(session) = core.live_session.as_ref() else {
        return agent_instance_id.to_string();
    };
    session
        .agents
        .iter()
        .find(|agent| agent.agent_instance_id == agent_instance_id)
        .map(|agent| {
            if agent.name.is_empty() {
                agent.agent_id.clone()
            } else {
                agent.name.clone()
            }
        })
        .unwrap_or_else(|| agent_instance_id.to_string())
}

pub fn view_target_requires_action(core: &ClientState, view_key: Option<&str>) -> bool {
    let (Some(session), Some(id)) = (core.live_session.as_ref(), view_key) else {
        return false;
    };
    agent_foreground(
        id,
        &session.agents,
        &session.active_turns,
        &session.pending_approvals,
        &session.pending_interactions,
    ) == AgentForeground::RequiresAction
}

pub fn truncate_chrome_label(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        label.to_string()
    } else {
        let mut cut: String = label.chars().take(max_chars).collect();
        cut.push('…');
        cut
    }
}

/// Last path segment of a model id, truncated for a chrome picker capsule.
pub fn model_chrome_label(model_id: &str) -> String {
    let base = model_id
        .rsplit(['/', ':'])
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(model_id);
    truncate_chrome_label(base, MODEL_LABEL_MAX)
}

/// Keep model capsules compact enough to leave room for neighboring chrome.
const MODEL_LABEL_MAX: usize = 16;

/// Title-case thinking level for a chrome picker capsule.
pub fn thinking_chrome_label(level: &str) -> String {
    match level {
        "off" => "Off".into(),
        "minimal" => "Minimal".into(),
        "low" => "Low".into(),
        "medium" => "Medium".into(),
        "high" => "High".into(),
        "xhigh" => "Extra high".into(),
        "max" => "Max".into(),
        other => truncate_chrome_label(other, 12),
    }
}

fn display_labels(session: &piko_client_core::LiveSession) -> Vec<String> {
    let bases: Vec<String> = session
        .agents
        .iter()
        .map(|agent| {
            if agent.name.is_empty() {
                agent.agent_id.clone()
            } else {
                agent.name.clone()
            }
        })
        .collect();
    bases
        .iter()
        .enumerate()
        .map(|(index, base)| {
            let duplicate = bases
                .iter()
                .enumerate()
                .any(|(other, label)| other != index && label == base);
            if duplicate {
                let suffix = suffix8(&session.agents[index].agent_instance_id);
                format!("{base} · {suffix}")
            } else {
                base.clone()
            }
        })
        .collect()
}

fn suffix8(id: &str) -> String {
    let mut chars = id.chars();
    let skip = id.chars().count().saturating_sub(8);
    chars.nth(skip).into_iter().chain(chars).collect()
}

fn tab_badge(core: &ClientState, agent_instance_id: &str, selected: bool) -> TabBadge {
    let Some(session) = core.live_session.as_ref() else {
        return TabBadge::None;
    };
    let foreground = agent_foreground(
        agent_instance_id,
        &session.agents,
        &session.active_turns,
        &session.pending_approvals,
        &session.pending_interactions,
    );
    match foreground {
        AgentForeground::RequiresAction => TabBadge::Attention,
        AgentForeground::Running | AgentForeground::Queued | AgentForeground::Cancelling => {
            TabBadge::Dot
        }
        AgentForeground::Idle => {
            let unread = session
                .agents
                .iter()
                .find(|agent| agent.agent_instance_id == agent_instance_id)
                .map(|agent| agent.unread_report_count)
                .unwrap_or(0);
            if unread > 0 && !selected {
                TabBadge::Count(unread)
            } else {
                TabBadge::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_client_core::state::SessionPhase;
    use piko_protocol::{AgentActivity, AgentInfo, AgentStatus};

    fn agent(id: &str, name: &str) -> AgentInfo {
        AgentInfo {
            session_id: "s1".into(),
            agent_instance_id: id.into(),
            agent_id: name.into(),
            parent_agent_instance_id: None,
            lifecycle: AgentInstanceLifecycle::Open,
            activity: AgentActivity::Idle,
            unread_report_count: 0,
            name: name.into(),
            role: "agent".into(),
            status: AgentStatus::Idle,
        }
    }

    #[test]
    fn empty_agent_list_still_shows_selected_tab() {
        let mut core = ClientState::default();
        core.session_phase = SessionPhase::Live;
        core.live_session = Some(piko_client_core::LiveSession {
            session_id: "s1".into(),
            selected_agent: Some("root".into()),
            ..piko_client_core::LiveSession::default()
        });
        let items = tab_items(&core, None);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "root");
        assert_eq!(items[0].label.as_ref(), "Main");
        assert!(items[0].tooltip.is_none());
    }

    #[test]
    fn unique_agent_names_do_not_put_instance_ids_in_tooltips() {
        let mut core = ClientState::default();
        core.session_phase = SessionPhase::Live;
        let mut session = piko_client_core::LiveSession {
            session_id: "s1".into(),
            selected_agent: Some("agent_aaaaaaaa_root".into()),
            ..piko_client_core::LiveSession::default()
        };
        session.agents = vec![agent("agent_aaaaaaaa_root", "Main")];
        core.live_session = Some(session);
        let items = tab_items(&core, None);
        assert_eq!(items[0].label.as_ref(), "Main");
        assert!(items[0].tooltip.is_none());
    }

    #[test]
    fn view_key_prefers_pending() {
        assert_eq!(view_key(Some("b"), Some("a")), Some("b"));
        assert_eq!(view_key(None, Some("a")), Some("a"));
        assert_eq!(view_key(None, None), None);
    }

    #[test]
    fn duplicate_names_take_instance_suffix() {
        let mut core = ClientState::default();
        core.session_phase = SessionPhase::Live;
        let mut session = piko_client_core::LiveSession {
            session_id: "s1".into(),
            selected_agent: Some("aaaaaaaa".into()),
            ..piko_client_core::LiveSession::default()
        };
        session.agents = vec![agent("aaaaaaaa", "Worker"), agent("bbbbbbbb", "Worker")];
        core.live_session = Some(session);
        let items = tab_items(&core, None);
        assert!(items[0].label.contains("aaaaaaaa") || items[0].label.contains("Worker ·"));
        assert_ne!(items[0].label, items[1].label);
    }

    #[test]
    fn chrome_labels_truncate_with_ellipsis() {
        assert_eq!(truncate_chrome_label("short", 22), "short");
        let long = truncate_chrome_label("deepseek-v4-flash-vision-exp", 22);
        assert_eq!(long.chars().count(), 23);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn model_chrome_label_uses_last_segment() {
        assert_eq!(
            model_chrome_label("deepseek/deepseek-v4-flash-vision-exp"),
            "deepseek-v4-flas…"
        );
        assert_eq!(model_chrome_label("gpt-4.1"), "gpt-4.1");
    }

    #[test]
    fn thinking_chrome_label_is_title_case() {
        assert_eq!(thinking_chrome_label("high"), "High");
        assert_eq!(thinking_chrome_label("xhigh"), "Extra high");
        assert_eq!(thinking_chrome_label("off"), "Off");
    }
}
