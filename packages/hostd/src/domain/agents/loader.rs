use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use piko_protocol::agents::AgentSpec;

use crate::domain::prompts::find_workspace_root;

const BUILT_IN_AGENT_RESOURCES: &[(&str, &str)] = &[
    ("main", include_str!("../../../resources/agents/main.toml")),
    (
        "general",
        include_str!("../../../resources/agents/general.toml"),
    ),
    (
        "scout",
        include_str!("../../../resources/agents/scout.toml"),
    ),
    (
        "coder",
        include_str!("../../../resources/agents/coder.toml"),
    ),
];

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

        let Some(agent_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };

        match parse_agent_toml(agent_id, &content) {
            Ok(spec) => agents.push(spec),
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
    thinking_level: Option<piko_protocol::model::ThinkingLevel>,
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
            thinking_level: self.thinking_level,
            tool_set_ids: self
                .tool_set_ids
                .unwrap_or_else(|| vec!["builtin".into(), "workspace".into()]),
            active_tool_names: self.active_tool_names,
        }
    }
}

fn parse_agent_toml(fallback_id: &str, content: &str) -> Result<AgentSpec, toml::de::Error> {
    toml::from_str::<TomlAgentSpec>(content).map(|spec| spec.into_agent_spec(fallback_id))
}

fn built_in_agents() -> HashMap<String, AgentSpec> {
    let mut map = HashMap::new();
    for (agent_id, content) in BUILT_IN_AGENT_RESOURCES {
        match parse_agent_toml(agent_id, content) {
            Ok(spec) => {
                map.insert(spec.id.clone(), spec);
            }
            Err(error) => {
                tracing::error!("Failed to parse built-in agent {agent_id}: {error}");
            }
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_agents_are_loaded_from_toml_resources() {
        let agents = built_in_agents();

        assert!(agents.contains_key("main"));
        assert!(agents.contains_key("general"));
        assert!(agents.contains_key("scout"));
        assert!(agents.contains_key("coder"));
        assert!(!agents.contains_key("subagent"));
        assert_eq!(agents["main"].name, "Main");
        assert_eq!(agents["general"].name, "General");
    }

    #[test]
    fn parses_workspace_agent_toml_with_filename_id_and_thinking_level() {
        let spec = parse_agent_toml(
            "reviewer",
            r#"
name = "Reviewer"
role = "reviewer"
description = "Reviews code."
system_prompt = "Review carefully."
thinking_level = "medium"
tool_set_ids = ["builtin"]
active_tool_names = ["read"]
"#,
        )
        .unwrap();

        assert_eq!(spec.id, "reviewer");
        assert_eq!(
            spec.thinking_level,
            Some(piko_protocol::model::ThinkingLevel::Medium)
        );
        assert_eq!(spec.active_tool_names, Some(vec!["read".to_string()]));
    }
}
