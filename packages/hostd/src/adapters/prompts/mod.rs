//! Filesystem-backed prompt material loaders (agents, skills, context
//! files, prompt templates). Pure formatting/parsing lives in
//! `domain::prompts`.

pub mod agent_loader;
pub mod loader;
pub mod skill_loader;

pub use agent_loader::load_agents;
pub use loader::{load_context_files, load_prompt_templates};
pub use skill_loader::load_skills;
