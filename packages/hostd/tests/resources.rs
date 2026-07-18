use std::fs;

use piko_hostd::adapters::prompts::{load_context_files, load_prompt_templates, load_skills};
use piko_hostd::api::{Message, MessageContent, MessageEntry, SessionTreeEntry};
use piko_hostd::domain::compaction::{
    CompactionSettings, FileOperations, compute_file_lists, format_file_operations, should_compact,
};
use piko_hostd::domain::prompts::skills::format_skills_for_prompt;
use piko_hostd::domain::prompts::{
    PromptSnapshotOptions, assemble_agent_run_prompt, expand_prompt_template, resolved_catalog,
    snapshot_prompt_resources,
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
