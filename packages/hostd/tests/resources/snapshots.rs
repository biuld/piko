use super::*;

fn run_facts_options() -> PromptSnapshotOptions {
    PromptSnapshotOptions {
        model: Some("model-b".into()),
        previous_model: Some("model-a".into()),
        environment: EnvironmentSnapshot {
            os: Some("macos".into()),
            arch: Some("aarch64".into()),
            shell: Some("/bin/zsh".into()),
            hostname: Some("host-1".into()),
            timezone: Some("+08:00".into()),
            locale: Some("en_US.UTF-8".into()),
        },
        ..Default::default()
    }
}

#[test]
fn snapshots_emit_environment_host_and_model_switch_blocks_without_world_state() {
    let snapshot = snapshot_prompt_resources(run_facts_options());
    let by_id = snapshot
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect::<std::collections::HashMap<_, _>>();

    // World-state moved to a retained transcript Context message (F-04
    // slice 2): the frozen prompt must not carry it as a block, and the
    // snapshot's message slot is populated by the turn submit path instead.
    assert!(snapshot.world_state.is_none());
    assert!(!snapshot.blocks.iter().any(|block| block.id == "state.run"));

    let host = by_id.get("environment.host").expect("environment block");
    assert_eq!(host.kind, piko_protocol::PromptBlockKind::Environment);
    assert_eq!(host.cache_scope, piko_protocol::CacheScope::RunDynamic);
    assert_eq!(
        host.content,
        concat!(
            "os: macos\n",
            "arch: aarch64\n",
            "shell: /bin/zsh\n",
            "hostname: host-1\n",
            "timezone: +08:00\n",
            "locale: en_US.UTF-8",
        )
    );

    let switch = by_id
        .get("context.model-switch")
        .expect("model-switch block");
    assert_eq!(switch.kind, piko_protocol::PromptBlockKind::Context);
    assert_eq!(switch.cache_scope, piko_protocol::CacheScope::RunDynamic);
    assert!(switch.content.starts_with("<model_switch>"));
    assert!(switch.content.contains("\"model-a\""));
    assert!(switch.content.contains("\"model-b\""));
    assert!(switch.content.ends_with("</model_switch>"));
}

#[test]
fn snapshots_omit_new_fragments_when_facts_are_absent() {
    let snapshot = snapshot_prompt_resources(PromptSnapshotOptions::default());
    assert!(!snapshot.blocks.iter().any(|block| matches!(
        block.id.as_str(),
        "environment.host" | "context.model-switch"
    )));
}

#[test]
fn model_switch_block_is_absent_on_first_run_and_unchanged_model() {
    let first_run = snapshot_prompt_resources(PromptSnapshotOptions {
        model: Some("model-a".into()),
        previous_model: None,
        ..Default::default()
    });
    assert!(
        !first_run
            .blocks
            .iter()
            .any(|block| block.id == "context.model-switch")
    );

    let unchanged = snapshot_prompt_resources(PromptSnapshotOptions {
        model: Some("model-a".into()),
        previous_model: Some("model-a".into()),
        ..Default::default()
    });
    assert!(
        !unchanged
            .blocks
            .iter()
            .any(|block| block.id == "context.model-switch")
    );
}

#[test]
fn run_dynamic_fragments_are_deterministic_and_cache_safe() {
    let with_project = |options: PromptSnapshotOptions| {
        snapshot_prompt_resources(PromptSnapshotOptions {
            context_files: vec![piko_hostd::domain::prompts::ContextFile {
                path: std::path::PathBuf::from("/workspace/AGENTS.md"),
                content: "Project context".into(),
            }],
            ..options
        })
    };
    let mut first = prompt_request(vec![prompt_tool("read", "Read files")]);
    first.resources = with_project(run_facts_options());
    let mut second = first.clone();
    second.resources = with_project(PromptSnapshotOptions {
        model: Some("model-c".into()),
        previous_model: Some("model-a".into()),
        environment: EnvironmentSnapshot {
            os: Some("linux".into()),
            ..Default::default()
        },
        ..run_facts_options()
    });

    let assembled_first = assemble_agent_run_prompt(&first);
    let assembled_second = assemble_agent_run_prompt(&second);
    assert_ne!(
        assembled_first.source_digest,
        assembled_second.source_digest
    );
    assert_eq!(
        assembled_first.cache_plan.semantic_prefix_digest,
        assembled_second.cache_plan.semantic_prefix_digest,
        "run-dynamic fragments must not invalidate the stable cache prefix"
    );

    let mut project_changed = first.clone();
    let project = project_changed
        .resources
        .blocks
        .iter_mut()
        .find(|block| block.id.starts_with("project.context"))
        .unwrap();
    project.content = "changed project policy".into();
    project.content_digest = "changed-project".into();
    let assembled_project = assemble_agent_run_prompt(&project_changed);
    assert_ne!(
        assembled_first.cache_plan.semantic_prefix_digest,
        assembled_project.cache_plan.semantic_prefix_digest,
        "project context changes must still invalidate the prefix"
    );
}
