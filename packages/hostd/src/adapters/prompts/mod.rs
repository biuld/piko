//! Filesystem-backed prompt material loaders (agents, skills, context
//! files, prompt templates). Pure formatting/parsing lives in
//! `domain::prompts`.

pub mod agent_loader;
pub mod loader;
pub mod skill_loader;

pub use agent_loader::load_agents;
pub use loader::{load_context_files, load_prompt_templates};
pub use skill_loader::load_skills;

use crate::domain::prompts::skills::LoadSkillsResult;
use crate::domain::prompts::{ContextFile, PromptTemplate};
use crate::ports::prompt_materials::PromptMaterialLoader;

/// Default [`PromptMaterialLoader`] backed by the real filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsPromptMaterialLoader;

impl PromptMaterialLoader for FsPromptMaterialLoader {
    fn load_prompt_templates(&self, cwd: &str) -> Vec<PromptTemplate> {
        load_prompt_templates(cwd)
    }

    fn load_context_files(&self, cwd: &str) -> Vec<ContextFile> {
        load_context_files(cwd)
    }

    fn load_skills(&self, cwd: &str) -> LoadSkillsResult {
        load_skills(cwd)
    }
}
