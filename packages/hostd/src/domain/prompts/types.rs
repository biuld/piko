use std::path::PathBuf;

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
