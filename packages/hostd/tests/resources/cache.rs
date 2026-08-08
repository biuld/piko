use super::*;

#[test]
fn cache_policy_from_resources_is_written_into_the_plan() {
    let mut request = prompt_request(vec![prompt_tool("read", "Read files")]);
    request.resources.cache_policy = piko_protocol::PromptCachePolicy::Disabled;
    let assembled = assemble_agent_run_prompt(&request);
    assert_eq!(
        assembled.cache_plan.policy,
        piko_protocol::PromptCachePolicy::Disabled
    );
}

#[test]
fn load_skills_prefers_project_over_global_visible_format() {
    let temp = tempfile::tempdir().unwrap();
    let project_skill = temp.path().join(".piko").join("skills").join("demo");
    fs::create_dir_all(&project_skill).unwrap();
    fs::write(
        project_skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Project skill\n---\nBody",
    )
    .unwrap();

    let result = load_skills(temp.path());
    assert_eq!(result.skills.len(), 1);
    let formatted = format_skills_for_prompt(&result.skills);
    assert!(formatted.contains("<name>demo</name>"));
    assert!(formatted.contains("Project skill"));
}

#[test]
fn load_skills_prefers_nearest_definition_with_the_same_name() {
    let temp = tempfile::tempdir().unwrap();
    let parent_skill = temp.path().join(".piko").join("skills").join("demo");
    let nested_cwd = temp.path().join("workspace").join("crate");
    let nested_skill = nested_cwd.join(".piko").join("skills").join("demo");
    fs::create_dir_all(&parent_skill).unwrap();
    fs::create_dir_all(&nested_skill).unwrap();
    fs::write(
        parent_skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Parent skill\n---\nParent body",
    )
    .unwrap();
    fs::write(
        nested_skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Nearest skill\n---\nNearest body",
    )
    .unwrap();

    let result = load_skills(&nested_cwd);
    let skill = result
        .skills
        .iter()
        .find(|skill| skill.name == "demo")
        .expect("demo skill");
    assert_eq!(skill.description, "Nearest skill");
    assert_eq!(skill.file_path, nested_skill.join("SKILL.md"));
}

#[test]
fn load_skills_parses_yaml_arrays_booleans_and_reports_malformed_frontmatter() {
    let temp = tempfile::tempdir().unwrap();
    let skills_dir = temp.path().join(".piko").join("skills");
    fs::create_dir_all(&skills_dir).unwrap();
    fs::write(
        skills_dir.join("tool-skill.md"),
        "---\nname: tool-skill\ndescription: Tool skill\ntools: [read, bash]\ndisable-model-invocation: true\n---\nBody",
    )
    .unwrap();
    fs::write(skills_dir.join("bad.md"), "---\n: invalid\n---\nBody").unwrap();

    let result = load_skills(temp.path());
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].active_tools.as_deref(), Some("read,bash"));
    assert!(result.skills[0].disable_model_invocation);
    assert_eq!(format_skills_for_prompt(&result.skills), "");
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.path.ends_with("bad.md") && !diagnostic.message.is_empty()
    }));
}

#[test]
fn compaction_estimates_threshold_and_formats_file_ops() {
    let entries = vec![SessionTreeEntry::Message(MessageEntry {
        id: "m1".into(),
        parent_id: None,
        timestamp: "1".into(),
        agent_id: "main".into(),
        agent_instance_id: "task-main".into(),
        source_turn_id: "work-main".into(),
        transcript_seq: 1,
        message: Message::User {
            content: MessageContent::String("x".repeat(100)),
            timestamp: None,
        },
    })];
    assert!(should_compact(
        &entries,
        30,
        &CompactionSettings {
            enabled: true,
            reserve_tokens: 10,
            keep_recent_tokens: 10,
            min_growth_tokens: 10,
        }
    ));

    let mut ops = FileOperations::default();
    ops.read.insert("README.md".into());
    ops.read.insert("src/main.rs".into());
    ops.edited.insert("src/main.rs".into());
    let lists = compute_file_lists(&ops);
    assert_eq!(lists.read_files, vec!["README.md"]);
    assert_eq!(lists.modified_files, vec!["src/main.rs"]);
    assert!(
        format_file_operations(&lists.read_files, &lists.modified_files)
            .contains("<modified-files>")
    );
}
