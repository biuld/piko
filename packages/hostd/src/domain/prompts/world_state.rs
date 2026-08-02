//! World-state facts and full/diff rendering (F-04 slice 2).
//!
//! hostd owns the durable per-session fact baseline and decides whether a
//! run injects the full snapshot (no baseline) or only the facts that
//! changed since the previous run (baseline present). The rendered message
//! is a data-only `Message::Context` retained in the transcript, never a
//! frozen prompt block.

use serde::{Deserialize, Serialize};

use piko_protocol::messages::{Message, MessageContent};
use piko_protocol::{ContentTrust, PromptSource};

use crate::util::now_ms;

/// First line of a diff-mode world-state message, distinguishing an update
/// from a full snapshot.
pub const WORLD_STATE_DIFF_HEADER: &str = "world-state changed since the previous run:";

/// Marker for a fact that was present in the baseline but is unavailable now.
const UNSET_MARKER: &str = "<unset>";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunKind {
    Initial,
    Continuation,
}

impl RunKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Continuation => "continuation",
        }
    }
}

/// Durable run facts captured once per accepted root turn. Field order is
/// the fixed emission order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldStateFacts {
    pub session_id: Option<String>,
    pub agent_instance_id: Option<String>,
    pub operation_id: Option<String>,
    pub run_kind: RunKind,
    pub model: Option<String>,
}

/// Full-snapshot content: one line per available fact in fixed order,
/// byte-identical to the F-03 `state.run` block content. `run_kind: initial`
/// is only emitted when at least one other fact is present, preserving the
/// original block rule.
pub fn world_state_full_content(facts: &WorldStateFacts) -> Option<String> {
    let mut lines = Vec::new();
    push_line(&mut lines, "session_id", facts.session_id.as_deref());
    push_line(
        &mut lines,
        "agent_instance_id",
        facts.agent_instance_id.as_deref(),
    );
    push_line(&mut lines, "operation_id", facts.operation_id.as_deref());
    match facts.run_kind {
        RunKind::Continuation => {
            lines.push(format!("run_kind: {}", RunKind::Continuation.as_str()))
        }
        RunKind::Initial if !lines.is_empty() => {
            lines.push(format!("run_kind: {}", RunKind::Initial.as_str()));
        }
        RunKind::Initial => {}
    }
    push_line(&mut lines, "model", facts.model.as_deref());
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Diff content against the previous run's baseline: a header line plus one
/// `fact: value` line per changed fact in fixed order. A fact that became
/// unavailable renders `fact: <unset>`. `None` when nothing changed.
pub fn world_state_diff_content(
    previous: &WorldStateFacts,
    current: &WorldStateFacts,
) -> Option<String> {
    let mut lines = Vec::new();
    push_diff(
        &mut lines,
        "session_id",
        previous.session_id.as_deref(),
        current.session_id.as_deref(),
    );
    push_diff(
        &mut lines,
        "agent_instance_id",
        previous.agent_instance_id.as_deref(),
        current.agent_instance_id.as_deref(),
    );
    push_diff(
        &mut lines,
        "operation_id",
        previous.operation_id.as_deref(),
        current.operation_id.as_deref(),
    );
    if previous.run_kind != current.run_kind {
        lines.push(format!("run_kind: {}", current.run_kind.as_str()));
    }
    push_diff(
        &mut lines,
        "model",
        previous.model.as_deref(),
        current.model.as_deref(),
    );
    if lines.is_empty() {
        return None;
    }
    let mut content = vec![WORLD_STATE_DIFF_HEADER.to_string()];
    content.extend(lines);
    Some(content.join("\n"))
}

/// Data-only world-state Context message with the trust/source identity of
/// the F-03 `state.run` block.
pub fn world_state_context_message(content: String) -> Message {
    Message::Context {
        content: MessageContent::String(content),
        trust: ContentTrust::Trusted,
        source: PromptSource::new("run-state", "hostd/session"),
        timestamp: Some(now_ms()),
    }
}

fn push_line(lines: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        lines.push(format!("{key}: {value}"));
    }
}

fn push_diff(lines: &mut Vec<String>, key: &str, previous: Option<&str>, current: Option<&str>) {
    if previous == current {
        return;
    }
    match current {
        Some(value) => lines.push(format!("{key}: {value}")),
        None => lines.push(format!("{key}: {UNSET_MARKER}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(operation_id: &str, model: Option<&str>) -> WorldStateFacts {
        WorldStateFacts {
            session_id: Some("session-1".into()),
            agent_instance_id: Some("agent_1".into()),
            operation_id: Some(operation_id.into()),
            run_kind: RunKind::Continuation,
            model: model.map(str::to_string),
        }
    }

    #[test]
    fn full_content_matches_f03_block_shape() {
        let content = world_state_full_content(&facts("turn_1", Some("model-a"))).unwrap();
        assert_eq!(
            content,
            "session_id: session-1\n\
agent_instance_id: agent_1\n\
operation_id: turn_1\n\
run_kind: continuation\n\
model: model-a"
        );
    }

    #[test]
    fn initial_run_kind_requires_another_fact() {
        let mut bare = WorldStateFacts {
            session_id: None,
            agent_instance_id: None,
            operation_id: None,
            run_kind: RunKind::Initial,
            model: None,
        };
        assert_eq!(world_state_full_content(&bare), None);
        bare.operation_id = Some("turn_1".into());
        assert_eq!(
            world_state_full_content(&bare).unwrap(),
            "operation_id: turn_1\nrun_kind: initial"
        );
    }

    #[test]
    fn diff_emits_only_changed_facts_in_order() {
        let previous = facts("turn_1", Some("model-a"));
        let current = facts("turn_2", Some("model-a"));
        let content = world_state_diff_content(&previous, &current).unwrap();
        assert_eq!(
            content,
            "world-state changed since the previous run:\noperation_id: turn_2"
        );
    }

    #[test]
    fn diff_marks_removed_facts_as_unset() {
        let previous = facts("turn_1", Some("model-a"));
        let current = facts("turn_2", None);
        let content = world_state_diff_content(&previous, &current).unwrap();
        assert!(content.contains("model: <unset>"));
    }

    #[test]
    fn identical_facts_produce_no_diff() {
        let same = facts("turn_1", Some("model-a"));
        assert_eq!(world_state_diff_content(&same, &same), None);
    }

    #[test]
    fn context_message_carries_trusted_run_state_source() {
        let message = world_state_context_message("operation_id: turn_2".into());
        let Message::Context { trust, source, .. } = message else {
            panic!("expected a Context message");
        };
        assert_eq!(trust, ContentTrust::Trusted);
        assert_eq!(source.kind, "run-state");
        assert_eq!(source.locator, "hostd/session");
    }
}
