use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use piko_protocol::agents::AgentSpec;

use crate::domain::prompts::find_workspace_root;

pub fn load_agents(cwd: impl AsRef<Path>) -> HashMap<String, AgentSpec> {
    let mut agents = built_in_agents();

    let cwd = cwd
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| cwd.as_ref().to_path_buf());
    let workspace_root = find_workspace_root(&cwd);

    let global_dir = piko_dir().join("agents");
    let project_dir = workspace_root.join(".piko").join("agents");

    // Load from global dir first
    for spec in load_from_dir(&global_dir) {
        agents.insert(spec.id.clone(), spec);
    }

    // Load from project dir, overriding global
    for spec in load_from_dir(&project_dir) {
        agents.insert(spec.id.clone(), spec);
    }

    agents
}

fn load_from_dir(dir: &Path) -> Vec<AgentSpec> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut agents = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        // Parse the TOML file into AgentSpec.
        // We use serde's camelCase conversion defined in the protocol struct.
        // Wait, TOML conventionally uses snake_case, but the struct fields are renamed using `camelCase` in `piko_protocol::agents::AgentSpec`.
        // To be friendly to TOML authors, we might need a wrapper or just expect TOML authors to use camelCase (e.g. `systemPrompt = "..."`).
        // In the design doc, I used snake_case: `system_prompt = "..."`.
        // Let's create an intermediate TOML representation and convert it to AgentSpec to support snake_case.

        match toml::from_str::<TomlAgentSpec>(&content) {
            Ok(toml_spec) => {
                agents.push(toml_spec.into_agent_spec(path.file_stem().unwrap().to_str().unwrap()))
            }
            Err(e) => {
                tracing::warn!("Failed to parse agent config {}: {}", path.display(), e);
            }
        }
    }

    agents
}

#[derive(serde::Deserialize)]
struct TomlAgentSpec {
    id: Option<String>,
    name: String,
    role: String,
    description: Option<String>,
    system_prompt: String,
    model: Option<String>,
    thinking_level: Option<u32>,
    tool_set_ids: Option<Vec<String>>,
    active_tool_names: Option<Vec<String>>,
}

impl TomlAgentSpec {
    fn into_agent_spec(self, fallback_id: &str) -> AgentSpec {
        AgentSpec {
            id: self.id.unwrap_or_else(|| fallback_id.to_string()),
            name: self.name,
            role: self.role,
            description: self.description,
            system_prompt: self.system_prompt,
            model: self.model,
            thinking_level: self.thinking_level.map(|l| match l {
                0 => piko_protocol::model::ThinkingLevel::Off,
                l if l <= 10 => piko_protocol::model::ThinkingLevel::Low,
                l if l <= 50 => piko_protocol::model::ThinkingLevel::Medium,
                _ => piko_protocol::model::ThinkingLevel::High,
            }),
            tool_set_ids: self
                .tool_set_ids
                .unwrap_or_else(|| vec!["builtin".into(), "workspace".into()]),
            active_tool_names: self.active_tool_names,
        }
    }
}

fn built_in_agents() -> HashMap<String, AgentSpec> {
    let mut map = HashMap::new();

    // Default main agent
    map.insert(
        "main".into(),
        AgentSpec {
            id: "main".into(),
            name: "Main".into(),
            role: "assistant".into(),
            description: Some("Default main agent".into()),
            system_prompt: String::new(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["builtin".into(), "workspace".into()],
            active_tool_names: None,
        },
    );

    // Generic subagent
    map.insert(
        "subagent".into(),
        AgentSpec {
            id: "subagent".into(),
            name: "Subagent".into(),
            role: "assistant".into(),
            description: Some("Generic subagent for delegating standard tasks".into()),
            system_prompt: String::new(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["builtin".into(), "workspace".into()],
            active_tool_names: None,
        },
    );

    // Scout agent
    map.insert(
        "scout".into(),
        AgentSpec {
            id: "scout".into(),
            name: "Scout".into(),
            role: "researcher".into(),
            description: Some("Expert at searching the web and summarizing documentation.".into()),
            system_prompt: "You are Scout, a specialized web researcher. Your job is to gather accurate information using web_search and fetch_content tools, and synthesize it clearly.".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["builtin".into(), "workspace".into()], // Real piko might have specific "web" toolset
            active_tool_names: None,
        }
    );

    // Coder agent
    map.insert(
        "coder".into(),
        AgentSpec {
            id: "coder".into(),
            name: "Coder".into(),
            role: "developer".into(),
            description: Some("Expert software engineer for writing and refactoring code.".into()),
            system_prompt: "You are Coder, an expert software engineer. Focus on writing clean, robust code. Use edit and bash to modify and test code.".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["builtin".into(), "workspace".into()],
            active_tool_names: None,
        }
    );

    map
}

fn home_dir() -> Option<PathBuf> {
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
