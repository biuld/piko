use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use piko_protocol::agents::AgentSpec;

use crate::adapters::prompts::loader::find_workspace_root;

pub fn load_agents(cwd: impl AsRef<Path>) -> HashMap<String, AgentSpec> {
    let mut agents = HashMap::new();

    let cwd = cwd
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| cwd.as_ref().to_path_buf());
    let workspace_root = find_workspace_root(&cwd);

    let (global_dir, global_provenance) = global_agent_source();
    let project_dir = workspace_root.join(".piko").join("agents");

    // The active base catalog comes first; workspace definitions override it.
    for spec in load_from_dir(&global_dir, global_provenance) {
        agents.insert(spec.id.clone(), spec);
    }

    // Load from project dir, overriding global
    for spec in load_from_dir(&project_dir, "workspace-agent") {
        agents.insert(spec.id.clone(), spec);
    }

    agents
}

fn load_from_dir(dir: &Path, provenance_kind: &str) -> Vec<AgentSpec> {
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
            Ok(mut spec) => {
                spec.version =
                    piko_orchd_api::stable_internal_id("agent-spec", &[agent_id, &content]);
                spec.provenance = piko_protocol::PromptSource::new(
                    provenance_kind,
                    path.to_string_lossy().replace('\\', "/"),
                )
                .with_version(spec.version.clone());
                agents.push(spec);
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
    kind: Option<piko_protocol::AgentKind>,
    description: Option<String>,
    instructions: String,
    model: Option<String>,
    thinking_level: Option<piko_protocol::model::ThinkingLevel>,
    tool_set_ids: Option<Vec<String>>,
    active_tool_names: Option<Vec<String>>,
}

impl TomlAgentSpec {
    fn into_agent_spec(self, fallback_id: &str) -> AgentSpec {
        let kind = self.kind.unwrap_or_default();
        let tool_set_ids = self
            .tool_set_ids
            .unwrap_or_else(|| vec!["todo".into(), "workspace".into()]);
        if matches!(kind, piko_protocol::AgentKind::Worker)
            && tool_set_ids.iter().any(|id| id == "multi_agent")
        {
            tracing::warn!(
                agent_id = fallback_id,
                "worker agent declares multi_agent; delegation tools will be hidden"
            );
        }
        AgentSpec {
            id: self.id.unwrap_or_else(|| fallback_id.to_string()),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("unclassified-agent", fallback_id),
            name: self.name,
            role: self.role,
            kind,
            description: self.description,
            base_instructions: self.instructions,
            model: self.model,
            thinking_level: self.thinking_level,
            tool_set_ids,
            active_tool_names: self.active_tool_names,
        }
    }
}

fn parse_agent_toml(fallback_id: &str, content: &str) -> Result<AgentSpec, toml::de::Error> {
    toml::from_str::<TomlAgentSpec>(content).map(|spec| spec.into_agent_spec(fallback_id))
}

#[cfg(test)]
fn fixture_agents() -> HashMap<String, AgentSpec> {
    let mut map = HashMap::new();
    let resource_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/agents");
    for spec in load_from_dir(&resource_dir, "test-agent") {
        let agent_id = spec.id.clone();
        let content = fs::read_to_string(resource_dir.join(format!("{agent_id}.toml"))).unwrap();
        let mut spec = spec;
        spec.version = piko_orchd_api::stable_internal_id("agent-spec", &[&agent_id, &content]);
        spec.provenance =
            piko_protocol::PromptSource::new("test-agent", format!("agents/{agent_id}"))
                .with_version(spec.version.clone());
        map.insert(spec.id.clone(), spec);
    }
    map
}

fn global_agent_source() -> (PathBuf, &'static str) {
    global_agent_source_from(
        std::env::var_os("PIKO_DEV_SOURCE_ROOT").map(PathBuf::from),
        piko_dir(),
    )
}

fn global_agent_source_from(
    dev_source_root: Option<PathBuf>,
    piko_root: PathBuf,
) -> (PathBuf, &'static str) {
    match dev_source_root {
        Some(root) => (
            root.join("packages/hostd/resources/agents"),
            "development-agent",
        ),
        None => (piko_root.join("agents").join("spec"), "installed-agent"),
    }
}

fn piko_dir() -> PathBuf {
    if let Some(root) = std::env::var_os("PIKO_HOME") {
        return PathBuf::from(root);
    }
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".piko")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_agents_are_loaded_from_toml_resources() {
        let agents = fixture_agents();

        assert!(agents.contains_key("main"));
        assert!(agents.contains_key("general"));
        assert!(agents.contains_key("scout"));
        assert!(agents.contains_key("coder"));
        assert!(!agents.contains_key("subagent"));
        assert_eq!(agents["main"].name, "Main");
        assert_eq!(agents["general"].name, "General");
        assert_eq!(agents["main"].kind, piko_protocol::AgentKind::Supervisor);
        assert_eq!(agents["general"].kind, piko_protocol::AgentKind::Supervisor);
        assert_eq!(agents["coder"].kind, piko_protocol::AgentKind::Supervisor);
        assert_eq!(agents["scout"].kind, piko_protocol::AgentKind::Worker);
    }

    #[test]
    fn fixture_agents_match_tool_set_matrix() {
        let agents = fixture_agents();
        assert_eq!(
            agents["main"].tool_set_ids,
            vec![
                "todo".to_string(),
                "workspace".to_string(),
                "user_interaction".to_string(),
                "multi_agent".to_string(),
            ]
        );
        for worker in ["general", "coder"] {
            assert_eq!(
                agents[worker].tool_set_ids,
                vec![
                    "todo".to_string(),
                    "workspace".to_string(),
                    "multi_agent".to_string(),
                ],
                "{worker} tool_set_ids"
            );
        }
        assert_eq!(
            agents["scout"].tool_set_ids,
            vec!["todo".to_string(), "workspace".to_string()]
        );
    }

    #[test]
    fn development_agent_source_is_independent_from_piko_home() {
        let (dir, provenance) =
            global_agent_source_from(Some(PathBuf::from("checkout")), PathBuf::from("user-state"));

        assert_eq!(
            dir,
            PathBuf::from("checkout/packages/hostd/resources/agents")
        );
        assert_eq!(provenance, "development-agent");
    }

    #[test]
    fn installed_agent_source_uses_piko_home() {
        let (dir, provenance) = global_agent_source_from(None, PathBuf::from("installation"));

        assert_eq!(dir, PathBuf::from("installation/agents/spec"));
        assert_eq!(provenance, "installed-agent");
    }

    #[test]
    fn parses_workspace_agent_toml_with_filename_id_and_thinking_level() {
        let spec = parse_agent_toml(
            "reviewer",
            r#"
name = "Reviewer"
role = "reviewer"
description = "Reviews code."
instructions = "Review carefully."
thinking_level = "medium"
tool_set_ids = ["todo"]
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
        assert_eq!(spec.kind, piko_protocol::AgentKind::Worker);
    }

    #[test]
    fn parses_explicit_supervisor_kind() {
        let spec = parse_agent_toml(
            "planner",
            r#"
name = "Planner"
role = "planner"
kind = "supervisor"
instructions = "Plan carefully."
"#,
        )
        .unwrap();

        assert_eq!(spec.kind, piko_protocol::AgentKind::Supervisor);
    }
}
