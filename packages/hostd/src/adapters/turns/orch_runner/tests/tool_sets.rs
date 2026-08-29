use super::*;

#[test]
fn ensure_root_tool_sets_adds_user_interaction_and_multi_agent() {
    let mut spec = AgentSpec {
        id: "main".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", "main"),
        name: "Main".into(),
        role: "root".into(),
        kind: piko_protocol::AgentKind::Supervisor,
        description: None,
        base_instructions: "hi".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: vec!["todo".into(), "workspace".into()],
        active_tool_names: None,
    };
    ensure_root_tool_sets(&mut spec);
    assert_eq!(
        spec.tool_set_ids,
        vec![
            "todo".to_string(),
            "workspace".to_string(),
            "user_interaction".to_string(),
            "multi_agent".to_string()
        ]
    );
}

#[test]
fn ensure_root_tool_sets_does_not_grant_delegation_to_worker() {
    let mut spec = AgentSpec {
        id: "scout".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", "scout"),
        name: "Scout".into(),
        role: "researcher".into(),
        kind: piko_protocol::AgentKind::Worker,
        description: None,
        base_instructions: "research".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: vec!["todo".into(), "workspace".into()],
        active_tool_names: None,
    };
    ensure_root_tool_sets(&mut spec);
    assert!(spec.tool_set_ids.iter().any(|id| id == "user_interaction"));
    assert!(!spec.tool_set_ids.iter().any(|id| id == "multi_agent"));
}

#[test]
fn resolve_recovered_agent_spec_prefers_durable_snapshot_then_registry_fallback() {
    let root_agent_spec = AgentSpec {
        id: "main".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", "main"),
        name: "Main".into(),
        role: "root".into(),
        kind: piko_protocol::AgentKind::Supervisor,
        description: None,
        base_instructions: "stable root prompt".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: vec![
            "todo".into(),
            "workspace".into(),
            "user_interaction".into(),
            "multi_agent".into(),
        ],
        active_tool_names: None,
    };
    let mut resolved_specs = std::collections::HashMap::new();
    resolved_specs.insert(
        "main".into(),
        AgentSpec {
            id: "main".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "main"),
            name: "Main".into(),
            role: "root".into(),
            kind: piko_protocol::AgentKind::Supervisor,
            description: None,
            base_instructions: "raw toml".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["todo".into(), "workspace".into()],
            active_tool_names: None,
        },
    );
    resolved_specs.insert(
        "coder".into(),
        AgentSpec {
            id: "coder".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "coder"),
            name: "Coder".into(),
            role: "worker".into(),
            kind: piko_protocol::AgentKind::Supervisor,
            description: None,
            base_instructions: "code".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["todo".into(), "workspace".into(), "multi_agent".into()],
            active_tool_names: None,
        },
    );

    let root = resolve_recovered_agent_spec(
        "agent_session_root",
        "agent_session_root",
        None,
        "main",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(root.base_instructions, "stable root prompt");
    assert!(root.tool_set_ids.iter().any(|id| id == "multi_agent"));
    assert!(root.tool_set_ids.iter().any(|id| id == "user_interaction"));

    let durable_root = resolve_recovered_agent_spec(
        "agent_session_root",
        "agent_session_root",
        Some(resolved_specs["main"].clone()),
        "main",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(durable_root.base_instructions, "raw toml");
    assert!(
        !durable_root
            .tool_set_ids
            .iter()
            .any(|id| id == "multi_agent")
    );

    let child = resolve_recovered_agent_spec(
        "agent_coder_1",
        "agent_session_root",
        None,
        "coder",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(child.base_instructions, "code");
    assert_eq!(
        child.tool_set_ids,
        vec![
            "todo".to_string(),
            "workspace".to_string(),
            "multi_agent".to_string()
        ]
    );
    assert!(!child.tool_set_ids.iter().any(|id| id == "user_interaction"));
}
