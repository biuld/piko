use std::fs;

use piko_hostd::adapters::prompts::{load_context_files, load_prompt_templates, load_skills};
use piko_hostd::api::{Message, MessageContent, MessageEntry, SessionTreeEntry};
use piko_hostd::domain::compaction::{
    CompactionSettings, FileOperations, compute_file_lists, format_file_operations, should_compact,
};
use piko_hostd::domain::prompts::skills::format_skills_for_prompt;
use piko_hostd::domain::prompts::{
    EnvironmentSnapshot, PromptSnapshotOptions, assemble_agent_run_prompt, expand_prompt_template,
    resolved_catalog, snapshot_prompt_resources,
};
use piko_protocol::{AgentSpec, PromptAssemblyRequest, ToolDef, ToolExecutorRef};

fn prompt_tool(name: &str, description: &str) -> ToolDef {
    ToolDef {
        name: name.into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test-tool", name),
        description: description.into(),
        input_schema: serde_json::json!({"type": "object"}),
        executor: ToolExecutorRef {
            kind: "native".into(),
            target: name.into(),
            extra: None,
        },
        execution_mode: None,
        exposure: None,
        capabilities: (name == "read").then(|| vec![piko_protocol::ToolCapability::WorkspaceRead]),
        approval: None,
        metadata: None,
    }
}

fn prompt_request(tool_catalog: Vec<ToolDef>) -> PromptAssemblyRequest {
    PromptAssemblyRequest {
        session_id: "session".into(),
        agent_instance_id: "root".into(),
        agent_spec: AgentSpec {
            id: "main".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "main"),
            name: "Main".into(),
            role: "root".into(),
            description: None,
            base_instructions: "Stable agent identity".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["workspace".into()],
            active_tool_names: None,
        },
        resources: snapshot_prompt_resources(PromptSnapshotOptions {
            cwd: std::path::PathBuf::from("/workspace"),
            context_files: vec![piko_hostd::domain::prompts::ContextFile {
                path: std::path::PathBuf::from("/workspace/AGENTS.md"),
                content: "Project context".into(),
            }],
            skills: vec![piko_hostd::domain::prompts::skills::Skill {
                name: "demo".into(),
                description: "Available skills".into(),
                file_path: "/workspace/.piko/skills/demo.md".into(),
                base_dir: "/workspace/.piko/skills".into(),
                disable_model_invocation: false,
                model_override: None,
                thinking_level: None,
                active_tools: None,
            }],
            ..Default::default()
        }),
        tool_catalog: resolved_catalog(tool_catalog),
    }
}

#[test]
fn loads_context_files_from_ancestors_general_to_specific() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("repo");
    let nested = project.join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    // Mark project/ as workspace root so find_workspace_root stops here.
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::write(project.join("AGENTS.md"), "project").unwrap();
    fs::write(nested.join("AGENTS.md"), "nested").unwrap();

    let files = load_context_files(&nested);
    let contents = files
        .iter()
        .map(|file| file.content.as_str())
        .filter(|content| *content == "project" || *content == "nested")
        .collect::<Vec<_>>();

    assert_eq!(contents, vec!["project", "nested"]);
}

#[test]
fn loads_and_expands_prompt_templates() {
    let temp = tempfile::tempdir().unwrap();
    let prompts = temp.path().join(".piko").join("prompts");
    fs::create_dir_all(&prompts).unwrap();
    fs::write(
        prompts.join("fix.md"),
        "---\ndescription: Fix a bug\nargument-hint: <file>\n---\nFix $1 with $ARGUMENTS",
    )
    .unwrap();

    let templates = load_prompt_templates(temp.path());
    assert_eq!(templates[0].name, "fix");
    assert_eq!(
        expand_prompt_template("/fix src/main.rs now", &templates),
        "Fix src/main.rs with src/main.rs now"
    );
}

#[test]
fn skips_malformed_prompt_templates() {
    let temp = tempfile::tempdir().unwrap();
    let prompts = temp.path().join(".piko").join("prompts");
    fs::create_dir_all(&prompts).unwrap();
    fs::write(prompts.join("bad.md"), "---\n: invalid\n---\nBad").unwrap();
    fs::write(
        prompts.join("good.md"),
        "---\ndescription: Good\n---\nGood $1",
    )
    .unwrap();

    let templates = load_prompt_templates(temp.path());
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].name, "good");
}

#[test]
fn expands_prompt_template_argument_slices_and_quotes() {
    let templates = vec![piko_hostd::domain::prompts::PromptTemplate {
        name: "slice".into(),
        description: "Slice args".into(),
        argument_hint: None,
        content: "one=$1 two=$2 ten=$10 all=$@ rest=${@:2} pair=${@:2:2} missing=$11 quoted=$3"
            .into(),
        file_path: std::path::PathBuf::from("slice.md"),
    }];

    assert_eq!(
        expand_prompt_template("/slice a1 a2 a3 a4 a5 a6 a7 a8 a9 a10", &templates),
        "one=a1 two=a2 ten=a10 all=a1 a2 a3 a4 a5 a6 a7 a8 a9 a10 rest=a2 a3 a4 a5 a6 a7 a8 a9 a10 pair=a2 a3 missing= quoted=a3"
    );

    assert_eq!(
        expand_prompt_template("/slice alpha beta 'gamma delta' epsilon", &templates),
        "one=alpha two=beta ten= all=alpha beta gamma delta epsilon rest=beta gamma delta epsilon pair=beta gamma delta missing= quoted=gamma delta"
    );
}

#[test]
fn snapshots_semantic_context_skills_and_templates() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join(".piko").join("skills").join("demo");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: demo\ndescription: Demo skill\n---\nBody",
    )
    .unwrap();
    let skills = load_skills(temp.path()).skills;
    let snapshot = snapshot_prompt_resources(PromptSnapshotOptions {
        cwd: temp.path().to_path_buf(),
        context_files: vec![piko_hostd::domain::prompts::ContextFile {
            path: temp.path().join("AGENTS.md"),
            content: "project rules".into(),
        }],
        skills,
        prompt_templates: vec![piko_hostd::domain::prompts::PromptTemplate {
            name: "fix".into(),
            description: "Fix".into(),
            argument_hint: None,
            content: "Fix".into(),
            file_path: temp.path().join(".piko/prompts/fix.md"),
        }],
        ..PromptSnapshotOptions::default()
    });
    let prompt = snapshot
        .blocks
        .iter()
        .map(|block| block.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(prompt.contains("project rules"));
    assert!(prompt.contains("Available skill metadata"));
    assert!(prompt.contains("Prompt templates expanded by hostd"));
    assert!(prompt.contains("Current date: 20"));
    assert!(!prompt.contains("unix-day-"));
}

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

#[test]
fn agent_run_prompt_describes_only_the_resolved_tool_catalog() {
    let prompt =
        assemble_agent_run_prompt(&prompt_request(vec![prompt_tool("bash", "Run commands")]));

    assert!(
        !prompt
            .blocks
            .iter()
            .any(|block| block.id == "catalog.skills")
    );
}

#[test]
fn agent_run_prompt_is_deterministic_for_equivalent_tool_catalogs() {
    let first = assemble_agent_run_prompt(&prompt_request(vec![
        prompt_tool("read", "Read files"),
        prompt_tool("bash", "Run commands"),
    ]));
    let second = assemble_agent_run_prompt(&prompt_request(vec![
        prompt_tool("bash", "Run commands"),
        prompt_tool("read", "Read files"),
    ]));

    assert_eq!(first, second);
    assert!(
        first
            .blocks
            .iter()
            .any(|block| block.id == "catalog.skills")
    );
}

#[test]
fn run_dynamic_environment_does_not_invalidate_stable_cache_prefix() {
    let mut first_request = prompt_request(vec![prompt_tool("read", "Read files")]);
    let mut second_request = first_request.clone();
    let environment = second_request
        .resources
        .blocks
        .iter_mut()
        .find(|block| block.id == "environment.run")
        .unwrap();
    environment.content = "Current date: 2099-01-01\nCurrent working directory: /other".into();
    environment.content_digest = "changed-environment".into();

    let first = assemble_agent_run_prompt(&first_request);
    let second = assemble_agent_run_prompt(&second_request);
    assert_ne!(first.source_digest, second.source_digest);
    assert_eq!(
        first.cache_plan.semantic_prefix_digest,
        second.cache_plan.semantic_prefix_digest
    );

    let project = first_request
        .resources
        .blocks
        .iter_mut()
        .find(|block| block.id.starts_with("project.context"))
        .unwrap();
    project.content = "changed project policy".into();
    project.content_digest = "changed-project".into();
    let changed_project = assemble_agent_run_prompt(&first_request);
    assert_ne!(
        first.cache_plan.semantic_prefix_digest,
        changed_project.cache_plan.semantic_prefix_digest
    );
}

#[test]
fn catalog_stable_scopes_are_labeled_independently_of_project_context() {
    let request = prompt_request(vec![prompt_tool("read", "Read files")]);
    let skills = request
        .resources
        .blocks
        .iter()
        .find(|block| block.id == "catalog.skills")
        .expect("skills catalog");
    let project = request
        .resources
        .blocks
        .iter()
        .find(|block| block.id.starts_with("project.context"))
        .expect("project context");
    assert_eq!(skills.cache_scope, piko_protocol::CacheScope::CatalogStable);
    assert_eq!(
        project.cache_scope,
        piko_protocol::CacheScope::ResourceSnapshot
    );
    assert_eq!(
        piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
        5,
        "D-28 assembly version"
    );
}

#[test]
fn project_and_catalog_invalidations_are_independent() {
    let baseline =
        assemble_agent_run_prompt(&prompt_request(vec![prompt_tool("read", "Read files")]));
    let resource_digest = |plan: &piko_protocol::PromptCachePlan| {
        plan.prefix_segments
            .iter()
            .find(|segment| segment.scope == piko_protocol::CacheScope::ResourceSnapshot)
            .map(|segment| segment.segment_digest.clone())
    };
    let catalog_digest = |plan: &piko_protocol::PromptCachePlan| {
        plan.prefix_segments
            .iter()
            .find(|segment| segment.scope == piko_protocol::CacheScope::CatalogStable)
            .map(|segment| segment.segment_digest.clone())
    };
    let base_resource = resource_digest(&baseline.cache_plan).expect("resource segment");
    let base_catalog = catalog_digest(&baseline.cache_plan).expect("catalog segment");

    let mut project_changed = prompt_request(vec![prompt_tool("read", "Read files")]);
    let project = project_changed
        .resources
        .blocks
        .iter_mut()
        .find(|block| block.id.starts_with("project.context"))
        .unwrap();
    project.content = "rewritten project".into();
    // Re-digest matches assemble's content_digest convention only for segment
    // comparison of stable digests field — assembly recomputes from content.
    project.content_digest = "manual".into();
    let after_project = assemble_agent_run_prompt(&project_changed);
    assert_ne!(
        resource_digest(&after_project.cache_plan).as_ref(),
        Some(&base_resource)
    );
    assert_eq!(
        catalog_digest(&after_project.cache_plan).as_ref(),
        Some(&base_catalog),
        "project edits must not invalidate catalog segment"
    );

    let mut skill_changed = prompt_request(vec![prompt_tool("read", "Read files")]);
    let skills = skill_changed
        .resources
        .blocks
        .iter_mut()
        .find(|block| block.id == "catalog.skills")
        .unwrap();
    skills.content = "Available skill metadata:\n- name: other".into();
    skills.content_digest = "skill-changed".into();
    let after_skills = assemble_agent_run_prompt(&skill_changed);
    assert_ne!(
        catalog_digest(&after_skills.cache_plan).as_ref(),
        Some(&base_catalog)
    );
    assert_eq!(
        resource_digest(&after_skills.cache_plan).as_ref(),
        Some(&base_resource),
        "skill catalog edits must not invalidate resource segment"
    );

    let tools_changed = assemble_agent_run_prompt(&prompt_request(vec![
        prompt_tool("read", "Read files"),
        prompt_tool("bash", "Run commands"),
    ]));
    assert_ne!(
        catalog_digest(&tools_changed.cache_plan).as_ref(),
        Some(&base_catalog)
    );
    assert_eq!(
        resource_digest(&tools_changed.cache_plan).as_ref(),
        Some(&base_resource)
    );
}

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
