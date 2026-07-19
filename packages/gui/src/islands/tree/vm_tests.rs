//! Unit tests for conversation tree projection.

use std::collections::HashSet;

use piko_client_core::SessionPhase;
use piko_client_core::state::LiveSession;
use piko_protocol::messages::Message;
use piko_protocol::session::{MessageEntry, SessionTreeEntry};

use super::*;

fn msg(id: &str, parent: Option<&str>, agent: &str, text: &str) -> SessionTreeEntry {
    SessionTreeEntry::Message(MessageEntry {
        id: id.into(),
        parent_id: parent.map(str::to_string),
        timestamp: "1".into(),
        agent_id: "spec".into(),
        agent_instance_id: agent.into(),
        source_turn_id: "t".into(),
        transcript_seq: 1,
        message: Message::User {
            content: piko_protocol::MessageContent::String(text.into()),
            timestamp: Some(1),
        },
    })
}

#[test]
fn marks_active_path_and_off_path_preview() {
    let mut state = ClientState::default();
    state.session_phase = SessionPhase::Live;
    let entries = vec![
        msg("a", None, "root", "first"),
        msg("b", Some("a"), "root", "second"),
        msg("c", Some("a"), "root", "branch"),
    ];
    state.live_session = Some(LiveSession {
        session_id: "s1".into(),
        selected_agent: Some("root".into()),
        current_leaf_id: Some("b".into()),
        entries: entries.clone(),
        ..Default::default()
    });

    let expanded = default_tree_expansion(&entries, Some("b"), Some("root"));
    let vm = derive_conversation_tree(&state, Some("c"), &expanded);
    assert!(vm.nodes.iter().any(|n| n.id == "a"));
    assert!(vm.nodes.iter().any(|n| n.id == "b"));
    assert!(vm.nodes.iter().any(|n| n.id == "c"));
    let a = vm.nodes.iter().find(|n| n.id == "a").unwrap();
    let b = vm.nodes.iter().find(|n| n.id == "b").unwrap();
    let c = vm.nodes.iter().find(|n| n.id == "c").unwrap();
    assert!(a.on_path && b.on_path && !c.on_path);
    assert!(a.has_children && a.expanded);
    assert!(!b.has_children && !c.has_children);
    assert!(b.is_leaf);
    assert_eq!(a.depth, 0);
    assert_eq!(b.depth, 1);
    assert_eq!(c.depth, 1);
    assert_eq!(vm.preview_entry_id.as_deref(), Some("c"));
}

#[test]
fn single_child_chain_stays_flat_depth() {
    let mut state = ClientState::default();
    state.session_phase = SessionPhase::Live;
    let entries = vec![
        msg("a", None, "root", "first"),
        msg("b", Some("a"), "root", "second"),
        msg("c", Some("b"), "root", "third"),
        msg("d", Some("c"), "root", "fourth"),
    ];
    state.live_session = Some(LiveSession {
        session_id: "s1".into(),
        selected_agent: Some("root".into()),
        current_leaf_id: Some("d".into()),
        entries: entries.clone(),
        ..Default::default()
    });
    let expanded = default_tree_expansion(&entries, Some("d"), Some("root"));
    assert!(
        expanded.is_empty(),
        "linear chains have no branch disclosure"
    );
    let vm = derive_conversation_tree(&state, None, &expanded);
    assert_eq!(vm.nodes.len(), 4);
    for node in &vm.nodes {
        assert_eq!(node.depth, 0, "id={} should stay flat", node.id);
        assert!(!node.has_children, "id={} is not a branch point", node.id);
    }
}

#[test]
fn filtered_single_child_does_not_indent() {
    let mut state = ClientState::default();
    state.session_phase = SessionPhase::Live;
    // `a` has two raw children, but only `b` is for selected agent `root`.
    let entries = vec![
        msg("a", None, "root", "first"),
        msg("b", Some("a"), "root", "keep"),
        msg("other", Some("a"), "other-agent", "hidden"),
    ];
    state.live_session = Some(LiveSession {
        session_id: "s1".into(),
        selected_agent: Some("root".into()),
        current_leaf_id: Some("b".into()),
        entries: entries.clone(),
        ..Default::default()
    });
    let expanded = default_tree_expansion(&entries, Some("b"), Some("root"));
    let vm = derive_conversation_tree(&state, None, &expanded);
    let a = vm.nodes.iter().find(|n| n.id == "a").unwrap();
    let b = vm.nodes.iter().find(|n| n.id == "b").unwrap();
    assert!(vm.nodes.iter().all(|n| n.id != "other"));
    assert_eq!(a.depth, 0);
    assert_eq!(b.depth, 0);
    assert!(!a.has_children, "one visible child is not a branch point");
    assert!(
        expanded.is_empty(),
        "do not seed disclosure for filtered single child"
    );
}

#[test]
fn on_path_preview_stays_display_only() {
    let mut state = ClientState::default();
    state.session_phase = SessionPhase::Live;
    let entries = vec![
        msg("a", None, "root", "first"),
        msg("b", Some("a"), "root", "second"),
    ];
    state.live_session = Some(LiveSession {
        session_id: "s1".into(),
        selected_agent: Some("root".into()),
        current_leaf_id: Some("b".into()),
        entries: entries.clone(),
        ..Default::default()
    });
    let expanded = default_tree_expansion(&entries, Some("b"), Some("root"));
    let vm = derive_conversation_tree(&state, Some("a"), &expanded);
    assert_eq!(vm.preview_entry_id.as_deref(), Some("a"));
}

#[test]
fn collapsed_parent_hides_children() {
    let mut state = ClientState::default();
    state.session_phase = SessionPhase::Live;
    let entries = vec![
        msg("a", None, "root", "first"),
        msg("b", Some("a"), "root", "second"),
        msg("c", Some("a"), "root", "branch"),
    ];
    state.live_session = Some(LiveSession {
        session_id: "s1".into(),
        selected_agent: Some("root".into()),
        current_leaf_id: Some("b".into()),
        entries,
        ..Default::default()
    });
    let expanded = HashSet::new();
    let vm = derive_conversation_tree(&state, None, &expanded);
    assert_eq!(vm.nodes.len(), 1);
    assert_eq!(vm.nodes[0].id, "a");
    assert!(vm.nodes[0].has_children);
    assert!(!vm.nodes[0].expanded);
}

#[test]
fn bookkeeping_model_and_thinking_entries_are_hidden() {
    use piko_protocol::session::{ModelChangeEntry, ThinkingLevelChangeEntry};

    let mut state = ClientState::default();
    state.session_phase = SessionPhase::Live;
    let entries = vec![
        msg("a", None, "root", "first"),
        SessionTreeEntry::ModelChange(ModelChangeEntry {
            id: "m1".into(),
            parent_id: Some("a".into()),
            timestamp: "1".into(),
            provider: "deepseek".into(),
            model_id: "flash".into(),
        }),
        SessionTreeEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
            id: "t1".into(),
            parent_id: Some("m1".into()),
            timestamp: "1".into(),
            thinking_level: "low".into(),
        }),
        msg("b", Some("t1"), "root", "second"),
    ];
    state.live_session = Some(LiveSession {
        session_id: "s1".into(),
        selected_agent: Some("root".into()),
        current_leaf_id: Some("b".into()),
        entries: entries.clone(),
        ..Default::default()
    });
    let expanded = default_tree_expansion(&entries, Some("b"), Some("root"));
    let vm = derive_conversation_tree(&state, None, &expanded);
    assert_eq!(
        vm.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert!(vm.nodes.iter().all(|n| n.kind != TreeEntryKind::Model));
}

#[test]
fn prune_keeps_surviving_expansion_ids() {
    let mut expanded: HashSet<String> = ["a", "gone"].into_iter().map(str::to_string).collect();
    let surviving: HashSet<String> = ["a", "b"].into_iter().map(str::to_string).collect();
    prune_tree_expansion(&mut expanded, &surviving);
    assert_eq!(expanded, ["a".into()].into_iter().collect());
}

#[test]
fn message_tool_call_uses_tool_kind_for_wrench_icon() {
    crate::i18n::init();
    let entry = SessionTreeEntry::Message(MessageEntry {
        id: "tc".into(),
        parent_id: None,
        timestamp: "1".into(),
        agent_id: "spec".into(),
        agent_instance_id: "root".into(),
        source_turn_id: "t".into(),
        transcript_seq: 1,
        message: Message::ToolCall {
            id: "call-1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({}),
            model: None,
            provider: None,
            timestamp: Some(1),
        },
    });
    let (kind, label) = entry_label(&entry);
    assert_eq!(kind, TreeEntryKind::Tool);
    assert!(label.contains("bash"));
}

#[test]
fn thinking_only_assistant_uses_thought_preview_not_empty() {
    crate::i18n::init();
    let entry = SessionTreeEntry::Message(MessageEntry {
        id: "m1".into(),
        parent_id: None,
        timestamp: "1".into(),
        agent_id: "spec".into(),
        agent_instance_id: "root".into(),
        source_turn_id: "t".into(),
        transcript_seq: 1,
        message: Message::Assistant {
            content: vec![piko_protocol::ContentBlock::Thinking {
                thinking: "consider spawning an agent".into(),
                thinking_signature: None,
            }],
            api: "chat".into(),
            provider: "test".into(),
            model: "m".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: Some(1),
        },
    });
    let (kind, label) = entry_label(&entry);
    assert_eq!(kind, TreeEntryKind::Thinking);
    assert!(label.contains("consider spawning"));
    assert!(!label.contains("empty"));
}
