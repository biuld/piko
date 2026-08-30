use std::fs;

use piko_hostd::adapters::bookkeeping::estimate_context_tokens;
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
        root_input_id: "input-1".into(),
        agent_spec: AgentSpec {
            id: "main".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "main"),
            name: "Main".into(),
            role: "root".into(),
            kind: piko_protocol::AgentKind::Supervisor,
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

mod cache;
mod catalog;
mod loads;
mod snapshots;
