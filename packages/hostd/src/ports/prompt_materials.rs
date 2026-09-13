//! Outbound port for loading prompt-assembly material (context files,
//! prompt templates, skills, mention targets) from the workspace. Pure
//! prompt assembly stays in `domain::prompts`; only the filesystem loading
//! is behind this port.

use crate::domain::prompts::skills::LoadSkillsResult;
use crate::domain::prompts::{ContextFile, PromptTemplate};

/// A workspace file resolved for a `@path` user mention.
#[derive(Debug, Clone, PartialEq)]
pub struct MentionFile {
    /// Display path relative to the workspace root (forward slashes).
    pub display_path: String,
    pub body: String,
}

pub trait PromptMaterialLoader: Send + Sync {
    fn load_prompt_templates(&self, cwd: &str) -> Vec<PromptTemplate>;
    fn load_context_files(&self, cwd: &str) -> Vec<ContextFile>;
    fn load_skills(&self, cwd: &str) -> LoadSkillsResult;

    /// Resolve one `@path` mention against the workspace. Returns `None`
    /// when the path escapes the workspace, is missing, or is not a file.
    /// Implementations must bound the read size (see
    /// `MAX_MENTION_FILE_BYTES`) so a huge mention target cannot balloon
    /// the prompt.
    fn load_mention_file(&self, raw_path: &str, cwd: &str) -> Option<MentionFile>;

    /// Read one loaded skill's body for a `$skill` mention. Returns `None`
    /// when the file disappeared or became unreadable since load time.
    fn load_skill_body(&self, file_path: &std::path::Path) -> Option<String>;
}

/// Upper bound for a single mention file body injected into the prompt.
pub const MAX_MENTION_FILE_BYTES: u64 = 256 * 1024;
