//! Outbound port for loading prompt-assembly material (context files,
//! prompt templates, skills) from the workspace. Pure prompt assembly stays
//! in `domain::prompts`; only the filesystem loading is behind this port.

use crate::domain::prompts::skills::LoadSkillsResult;
use crate::domain::prompts::{ContextFile, PromptTemplate};

pub trait PromptMaterialLoader: Send + Sync {
    fn load_prompt_templates(&self, cwd: &str) -> Vec<PromptTemplate>;
    fn load_context_files(&self, cwd: &str) -> Vec<ContextFile>;
    fn load_skills(&self, cwd: &str) -> LoadSkillsResult;
}
