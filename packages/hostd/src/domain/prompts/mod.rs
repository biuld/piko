use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub mod skills;

use crate::domain::prompts::skills::{Skill, format_skills_for_prompt};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PromptResources {
    pub system_prompt: String,
    pub context_files: Vec<ContextFile>,
    pub prompt_templates: Vec<PromptTemplate>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextFile {
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptTemplate {
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
    pub content: String,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct BuildSystemPromptOptions {
    pub custom_prompt: Option<String>,
    pub selected_tools: Option<Vec<String>>,
    pub tool_snippets: Option<std::collections::HashMap<String, String>>,
    pub prompt_guidelines: Vec<String>,
    pub append_system_prompt: Option<String>,
    pub cwd: PathBuf,
    pub context_files: Vec<ContextFile>,
    pub skills: Vec<Skill>,
    pub prompt_templates: Vec<PromptTemplate>,
}

#[derive(Debug, thiserror::Error)]
pub enum PromptResourceError {
    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn load_context_files(cwd: impl AsRef<Path>) -> Vec<ContextFile> {
    let cwd = cwd
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| cwd.as_ref().to_path_buf());

    let workspace_root = find_workspace_root(&cwd);

    // Collect directories from workspace_root down to cwd, then load
    // AGENTS.md from each in order (general → specific).
    let mut dirs = Vec::new();
    let mut current = cwd.as_path();
    loop {
        dirs.push(current.to_path_buf());
        if current == workspace_root.as_path() {
            break;
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    dirs.reverse();

    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for dir in dirs {
        if let Some(file) = load_from_dir(&dir) {
            if seen.insert(file.path.clone()) {
                files.push(file);
            }
        }
    }
    files
}

pub fn load_prompt_templates(cwd: impl AsRef<Path>) -> Vec<PromptTemplate> {
    let cwd = cwd.as_ref();
    let project_dir = cwd.join(".piko").join("prompts");
    let global_dir = piko_dir().join("prompts");
    let mut seen = HashSet::new();
    let mut templates = Vec::new();

    for template in load_templates_from_dir(&project_dir) {
        if seen.insert(template.name.clone()) {
            templates.push(template);
        }
    }
    for template in load_templates_from_dir(&global_dir) {
        if seen.insert(template.name.clone()) {
            templates.push(template);
        }
    }
    templates
}

pub fn expand_prompt_template(text: &str, templates: &[PromptTemplate]) -> String {
    if !text.starts_with('/') {
        return text.to_string();
    }
    let mut parts = text[1..].splitn(2, char::is_whitespace);
    let Some(name) = parts.next() else {
        return text.to_string();
    };
    let args_string = parts.next().unwrap_or("").trim();
    let Some(template) = templates.iter().find(|template| template.name == name) else {
        return text.to_string();
    };
    substitute_args(&template.content, &parse_command_args(args_string))
}

pub fn build_system_prompt(options: BuildSystemPromptOptions) -> String {
    let cwd = options.cwd.to_string_lossy().replace('\\', "/");
    let date = current_date_string();
    let selected_tools = options
        .selected_tools
        .unwrap_or_else(|| vec!["read".into(), "bash".into(), "edit".into(), "write".into()]);

    if let Some(custom_prompt) = options.custom_prompt {
        let mut prompt = custom_prompt;
        append_context(&mut prompt, &options.context_files);
        if selected_tools.iter().any(|tool| tool == "read") {
            prompt.push_str(&format_skills_for_prompt(&options.skills));
        }
        prompt.push_str(&format_prompt_templates(&options.prompt_templates));
        prompt.push_str(&format!("\nCurrent date: {date}"));
        prompt.push_str(&format!("\nCurrent working directory: {cwd}"));
        return prompt;
    }

    let snippets = default_tool_snippets();
    let tools_list = selected_tools
        .iter()
        .filter_map(|tool| {
            snippets
                .get(tool)
                .map(|snippet| format!("- {tool}: {snippet}"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tools_list = if tools_list.is_empty() {
        "(none)".to_string()
    } else {
        tools_list
    };
    let guidelines = build_guidelines(&options.prompt_guidelines);

    let mut prompt = format!(
        "You are an expert coding assistant operating inside piko, a coding agent harness. You help users by reading files, executing commands, editing code, and writing new files.\n\nAvailable tools:\n{tools_list}\n\nGuidelines:\n{guidelines}\n\nPi documentation (read only when the user asks about pi itself, its SDK, extensions, themes, skills, or TUI):\n- Main documentation: /opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/README.md\n- Additional docs: /opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/docs\n- Examples: /opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/examples (extensions, custom tools, SDK)\n- When reading pi docs or examples, resolve docs/... under Additional docs and examples/... under Examples, not the current working directory\n- When asked about: extensions (docs/extensions.md, examples/extensions/), themes (docs/themes.md), skills (docs/skills.md), prompt templates (docs/prompt-templates.md), TUI components (docs/tui.md), keybindings (docs/keybindings.md), SDK integrations (docs/sdk.md), custom providers (docs/custom-provider.md), adding models (docs/models.md), pi packages (docs/packages.md)\n- When working on pi topics, read the docs and examples, and follow .md cross-references before implementing\n- Always read pi .md files completely and follow links to related docs (e.g., tui.md for TUI API details)"
    );

    if let Some(append) = options.append_system_prompt {
        prompt.push_str("\n\n");
        prompt.push_str(&append);
    }
    append_context(&mut prompt, &options.context_files);
    if selected_tools.iter().any(|tool| tool == "read") {
        prompt.push_str(&format_skills_for_prompt(&options.skills));
    }
    prompt.push_str(&format_prompt_templates(&options.prompt_templates));
    prompt.push_str(&format!("\nCurrent date: {date}"));
    prompt.push_str(&format!("\nCurrent working directory: {cwd}"));
    prompt
}

pub(crate) fn parse_frontmatter_result(
    content: &str,
) -> Result<(std::collections::HashMap<String, String>, String), String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.starts_with("---") {
        return Ok((std::collections::HashMap::new(), normalized));
    }
    let Some(end) = normalized[3..].find("\n---") else {
        return Ok((std::collections::HashMap::new(), normalized));
    };
    let yaml_start = 4.min(normalized.len());
    let yaml_end = 3 + end;
    let yaml = &normalized[yaml_start..yaml_end];
    let body = normalized[yaml_end + 4..].trim().to_string();
    let value =
        serde_yaml::from_str::<serde_yaml::Value>(yaml).map_err(|error| error.to_string())?;
    let Some(mapping) = value.as_mapping() else {
        return Ok((std::collections::HashMap::new(), body));
    };
    let mut map = std::collections::HashMap::new();
    for (key, value) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };
        if let Some(value) = yaml_value_to_string(value) {
            map.insert(key.to_string(), value);
        }
    }
    Ok((map, body))
}

fn yaml_value_to_string(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::Null => Some(String::new()),
        serde_yaml::Value::Bool(value) => Some(value.to_string()),
        serde_yaml::Value::Number(value) => Some(value.to_string()),
        serde_yaml::Value::String(value) => Some(value.clone()),
        serde_yaml::Value::Sequence(values) => Some(
            values
                .iter()
                .filter_map(yaml_value_to_string)
                .collect::<Vec<_>>()
                .join(","),
        ),
        serde_yaml::Value::Mapping(_) | serde_yaml::Value::Tagged(_) => None,
    }
}

fn load_from_dir(dir: &Path) -> Option<ContextFile> {
    for name in ["AGENTS.md", "AGENTS.MD", "CLAUDE.md", "CLAUDE.MD"] {
        let path = dir.join(name);
        if path.is_file()
            && let Ok(content) = fs::read_to_string(&path)
        {
            return Some(ContextFile { path, content });
        }
    }
    None
}

fn load_templates_from_dir(dir: &Path) -> Vec<PromptTemplate> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut templates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok((frontmatter, body)) = parse_frontmatter_result(&raw) else {
            continue;
        };
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unnamed")
            .to_string();
        let description = frontmatter.get("description").cloned().unwrap_or_else(|| {
            let first = body
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("");
            if first.chars().count() > 60 {
                format!("{}...", first.chars().take(60).collect::<String>())
            } else {
                first.to_string()
            }
        });
        templates.push(PromptTemplate {
            name,
            description,
            argument_hint: frontmatter.get("argument-hint").cloned(),
            content: body,
            file_path: path,
        });
    }
    templates.sort_by(|a, b| a.name.cmp(&b.name));
    templates
}

fn parse_command_args(args: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in args.chars() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

fn substitute_args(content: &str, args: &[String]) -> String {
    let mut result = substitute_numeric_args(content, args);
    result = substitute_arg_slices(&result, args);
    let all = args.join(" ");
    result.replace("$ARGUMENTS", &all).replace("$@", &all)
}

fn substitute_numeric_args(content: &str, args: &[String]) -> String {
    let mut result = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '$' || !chars.peek().is_some_and(|next| next.is_ascii_digit()) {
            result.push(ch);
            continue;
        }

        let mut digits = String::new();
        while chars.peek().is_some_and(|next| next.is_ascii_digit()) {
            if let Some(digit) = chars.next() {
                digits.push(digit);
            }
        }

        let index = digits.parse::<usize>().unwrap_or(0).saturating_sub(1);
        if let Some(arg) = args.get(index) {
            result.push_str(arg);
        }
    }

    result
}

fn substitute_arg_slices(content: &str, args: &[String]) -> String {
    let mut result = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find("${@:") {
        result.push_str(&rest[..start]);
        let after_marker = &rest[start + 4..];
        let Some(end) = after_marker.find('}') else {
            result.push_str(&rest[start..]);
            return result;
        };

        let expression = &after_marker[..end];
        let replacement = parse_arg_slice_expression(expression)
            .map(|(start, length)| {
                let zero_based = start.saturating_sub(1);
                match length {
                    Some(length) => args
                        .iter()
                        .skip(zero_based)
                        .take(length)
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(" "),
                    None => args
                        .iter()
                        .skip(zero_based)
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(" "),
                }
            })
            .unwrap_or_else(|| rest[start..start + 4 + end + 1].to_string());

        result.push_str(&replacement);
        rest = &after_marker[end + 1..];
    }

    result.push_str(rest);
    result
}

fn parse_arg_slice_expression(expression: &str) -> Option<(usize, Option<usize>)> {
    let (start, length) = expression
        .split_once(':')
        .map_or((expression, None), |(start, length)| (start, Some(length)));
    if start.is_empty() || !start.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let start = start.parse::<usize>().ok()?;
    let length = match length {
        Some(length) if length.chars().all(|ch| ch.is_ascii_digit()) => {
            Some(length.parse::<usize>().ok()?)
        }
        Some(_) => return None,
        None => None,
    };
    Some((start, length))
}

fn append_context(prompt: &mut String, context_files: &[ContextFile]) {
    if context_files.is_empty() {
        return;
    }
    prompt.push_str("\n\n<project_context>\n\nProject-specific instructions and guidelines:\n\n");
    for file in context_files {
        prompt.push_str(&format!(
            "<project_instructions path=\"{}\">\n{}\n</project_instructions>\n\n",
            escape_xml(&file.path.to_string_lossy()),
            file.content
        ));
    }
    prompt.push_str("</project_context>\n");
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

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

fn piko_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".piko")
}

/// Walk upward from `cwd` looking for a workspace-root marker (`.git`,
/// `.piko`, or `.agents` directory). Stops at the home directory so the
/// search never escapes the user's workspace tree. Returns `cwd` itself
/// when no marker is found.
pub(crate) fn find_workspace_root(cwd: &Path) -> PathBuf {
    let home = home_dir();
    let mut current = Some(cwd);
    while let Some(dir) = current {
        if dir.join(".git").exists()
            || dir.join(".piko").is_dir()
            || dir.join(".agents").is_dir()
        {
            return dir.to_path_buf();
        }
        if home.as_deref() == Some(dir) {
            break;
        }
        current = dir.parent();
    }
    cwd.to_path_buf()
}
