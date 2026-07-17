use crate::domain::prompts::skills::format_skills_for_prompt;

use super::types::{BuildSystemPromptOptions, ContextFile, PromptTemplate};

pub fn snapshot_prompt_resources(
    options: BuildSystemPromptOptions,
) -> piko_protocol::PromptResourceSnapshot {
    let cwd = options.cwd.to_string_lossy().replace('\\', "/");
    let date = current_date_string();
    let product_instructions = if let Some(custom_prompt) = options.custom_prompt {
        custom_prompt
    } else {
        let guidelines = build_guidelines(&options.prompt_guidelines);
        let mut prompt = format!(
            "You are an expert coding assistant operating inside piko, a coding agent harness. You help users by reading files, executing commands, editing code, and writing new files.\n\nGuidelines:\n{guidelines}"
        );
        if let Some(append) = options.append_system_prompt {
            prompt.push_str("\n\n");
            prompt.push_str(&append);
        }
        prompt
    };
    piko_protocol::PromptResourceSnapshot {
        product_instructions,
        context_section: format_context(&options.context_files),
        skills_section: format_skills_for_prompt(&options.skills),
        prompt_templates_section: format_prompt_templates(&options.prompt_templates),
        environment_section: format!("\nCurrent date: {date}\nCurrent working directory: {cwd}"),
    }
}

pub fn assemble_agent_run_prompt(
    request: &piko_protocol::PromptAssemblyRequest,
) -> piko_protocol::AgentRunPrompt {
    let mut sections = Vec::new();
    if !request.resources.product_instructions.trim().is_empty() {
        sections.push(request.resources.product_instructions.trim().to_string());
    }
    if !request.agent_spec.base_system_prompt.trim().is_empty() {
        sections.push(request.agent_spec.base_system_prompt.trim().to_string());
    }
    let mut tools = request.tool_catalog.clone();
    tools.sort_by(|left, right| left.name.cmp(&right.name));

    let has_read = request.tool_catalog.iter().any(|tool| tool.name == "read");
    for section in [
        Some(&request.resources.context_section),
        has_read.then_some(&request.resources.skills_section),
        Some(&request.resources.prompt_templates_section),
        Some(&request.resources.environment_section),
    ]
    .into_iter()
    .flatten()
    {
        if !section.trim().is_empty() {
            sections.push(section.trim().to_string());
        }
    }
    let system_prompt = sections.join("\n\n");
    let catalog_digest_input = serde_json::to_string(&tools)
        .expect("resolved tool catalog must serialize for prompt diagnostics");
    let assembly_version = piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION.to_string();
    piko_protocol::AgentRunPrompt {
        source_digest: piko_orchd_api::stable_internal_id(
            "prompt",
            &[&assembly_version, &system_prompt, &catalog_digest_input],
        ),
        assembly_version: piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
        system_prompt,
    }
}

pub fn build_system_prompt(options: BuildSystemPromptOptions) -> String {
    let selected_tools = options
        .selected_tools
        .clone()
        .unwrap_or_else(|| vec!["read".into(), "bash".into(), "edit".into(), "write".into()]);
    let snippets = default_tool_snippets();
    let tool_catalog = selected_tools
        .iter()
        .filter_map(|name| {
            snippets
                .get(name)
                .map(|description| test_tool(name, description))
        })
        .collect();
    let request = piko_protocol::PromptAssemblyRequest {
        session_id: String::new(),
        agent_instance_id: String::new(),
        agent_spec: piko_protocol::AgentSpec {
            id: "main".into(),
            name: "Main".into(),
            role: "root".into(),
            description: None,
            base_system_prompt: String::new(),
            model: None,
            thinking_level: None,
            tool_set_ids: Vec::new(),
            active_tool_names: None,
        },
        resources: snapshot_prompt_resources(options),
        tool_catalog,
    };
    assemble_agent_run_prompt(&request).system_prompt
}

fn format_context(context_files: &[ContextFile]) -> String {
    if context_files.is_empty() {
        return String::new();
    }
    let mut prompt = String::new();
    prompt.push_str("\n\n<project_context>\n\nProject-specific instructions and guidelines:\n\n");
    for file in context_files {
        prompt.push_str(&format!(
            "<project_instructions path=\"{}\">\n{}\n</project_instructions>\n\n",
            escape_xml(&file.path.to_string_lossy()),
            file.content
        ));
    }
    prompt.push_str("</project_context>\n");
    prompt
}

fn test_tool(name: &str, description: &str) -> piko_protocol::ToolDef {
    piko_protocol::ToolDef {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: serde_json::json!({"type": "object"}),
        executor: piko_protocol::ToolExecutorRef {
            kind: "native".into(),
            target: name.to_string(),
            extra: None,
        },
        execution_mode: None,
        exposure: None,
        capabilities: None,
        approval: None,
        metadata: None,
    }
}

fn format_prompt_templates(templates: &[PromptTemplate]) -> String {
    if templates.is_empty() {
        return String::new();
    }
    let mut section = "\n\n## Prompt Templates\n\nThe following prompt templates are available as slash commands:\n".to_string();
    for template in templates {
        let hint = template
            .argument_hint
            .as_ref()
            .map(|hint| format!(" {hint}"))
            .unwrap_or_default();
        section.push_str(&format!(
            "- /{}{} — {}\n",
            template.name, hint, template.description
        ));
    }
    section.push_str("\nWhen the user types a /command matching one of these templates, expand it using the template content.");
    section
}

fn default_tool_snippets() -> std::collections::HashMap<String, String> {
    [
        ("read", "Read file contents"),
        ("bash", "Execute bash commands (ls, grep, find, etc.)"),
        (
            "edit",
            "Make precise file edits with exact text replacement",
        ),
        ("write", "Create or overwrite files"),
        ("ls", "List directory contents"),
        ("grep", "Search file contents for a pattern"),
        ("find", "Find files by name pattern"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect()
}

fn build_guidelines(extra: &[String]) -> String {
    let mut items = vec![
        "Use bash for file operations like ls, rg, find".to_string(),
        "Use read to examine files instead of cat or sed.".to_string(),
        "Use edit for precise changes (edits[].oldText must match exactly)".to_string(),
        "When changing multiple separate locations in one file, use one edit call with multiple entries in edits[] instead of multiple edit calls".to_string(),
        "Each edits[].oldText is matched against the original file, not after earlier edits are applied. Do not emit overlapping or nested edits. Merge nearby changes into one edit.".to_string(),
        "Keep edits[].oldText as small as possible while still being unique in the file. Do not pad with large unchanged regions.".to_string(),
        "Use write only for new files or complete rewrites.".to_string(),
        "Be concise in your responses".to_string(),
        "Show file paths clearly when working with files".to_string(),
    ];
    for item in extra {
        if !item.trim().is_empty() && !items.contains(item) {
            items.push(item.trim().to_string());
        }
    }
    items
        .into_iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn current_date_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
