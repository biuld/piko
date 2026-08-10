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
    /// Model id executing this run; used by the model-switch fragment and
    /// the hostd world-state facts (F-04 slice 2).
    pub model: Option<String>,
    /// Model id used by the previous turn of this session, when known.
    pub previous_model: Option<String>,
    /// Host facts for the environment-context fragment (`environment.host`).
    pub environment: EnvironmentSnapshot,
    /// Provider prompt-cache policy for this run (F-03 / D-28).
    pub cache_policy: piko_protocol::PromptCachePolicy,
    /// Current durable todo list for the running agent (F-27), when feature on
    /// and items non-empty. Injected as a separate `todo.list` fragment.
    pub todo_list: Option<piko_protocol::TodoList>,
    /// When true, include the standing todo drive instruction in the policy block.
    pub todo_feature_on: bool,
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
