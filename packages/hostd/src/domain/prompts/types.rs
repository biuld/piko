use std::path::PathBuf;

use super::environment::EnvironmentSnapshot;
use crate::domain::prompts::skills::Skill;

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
pub struct PromptSnapshotOptions {
    pub operator_instructions: Vec<String>,
    pub cwd: PathBuf,
    pub context_files: Vec<ContextFile>,
    pub skills: Vec<Skill>,
    pub prompt_templates: Vec<PromptTemplate>,
    /// Durable run facts for the world-state fragment (`state.run`).
    pub session_id: Option<String>,
    pub agent_instance_id: Option<String>,
    pub operation_id: Option<String>,
    /// Model id executing this run; used by the world-state and model-switch
    /// fragments.
    pub model: Option<String>,
    /// Model id used by the previous turn of this session, when known.
    pub previous_model: Option<String>,
    /// True when this run continues a session with committed prior work.
    pub continuation: bool,
    /// Host facts for the environment-context fragment (`environment.host`).
    pub environment: EnvironmentSnapshot,
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
