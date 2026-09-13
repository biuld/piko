//! Filesystem-backed prompt material loaders (agents, skills, context
//! files, prompt templates). Pure formatting/parsing lives in
//! `domain::prompts`.

pub mod agent_loader;
pub mod loader;
pub mod skill_loader;

pub use agent_loader::load_agents;
pub use loader::{load_context_files, load_prompt_templates};
pub use skill_loader::load_skills;

use std::path::{Path, PathBuf};

use crate::domain::prompts::skills::LoadSkillsResult;
use crate::domain::prompts::{ContextFile, PromptTemplate};
use crate::ports::prompt_materials::{MAX_MENTION_FILE_BYTES, MentionFile, PromptMaterialLoader};

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

    fn load_mention_file(&self, raw_path: &str, cwd: &str) -> Option<MentionFile> {
        read_workspace_mention_file(raw_path, Path::new(cwd))
    }

    fn load_skill_body(&self, file_path: &Path) -> Option<String> {
        let bytes = std::fs::read(file_path).ok()?;
        if bytes.len() as u64 > MAX_MENTION_FILE_BYTES || bytes.contains(&0) {
            return None;
        }
        String::from_utf8(bytes).ok()
    }
}

/// Resolve and bound-read one `@path` mention target. Symlinks are resolved
/// before the workspace containment check so an in-workspace link cannot
/// escape it. Reads are capped at [`MAX_MENTION_FILE_BYTES`].
fn read_workspace_mention_file(raw_path: &str, cwd: &Path) -> Option<MentionFile> {
    let candidate = if Path::new(raw_path).is_absolute() {
        PathBuf::from(raw_path)
    } else {
        cwd.join(raw_path)
    };
    let cwd_canon = cwd.canonicalize().ok()?;
    let path_canon = candidate.canonicalize().ok()?;
    if !path_canon.starts_with(&cwd_canon) || !path_canon.is_file() {
        return None;
    }
    let metadata = std::fs::metadata(&path_canon).ok()?;
    if metadata.len() > MAX_MENTION_FILE_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path_canon).ok()?;
    // Re-check the actual read: the file may have grown after metadata was
    // sampled.
    if bytes.len() as u64 > MAX_MENTION_FILE_BYTES || bytes.contains(&0) {
        return None;
    }
    let body = String::from_utf8(bytes).ok()?;
    let display_path = path_canon
        .strip_prefix(&cwd_canon)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path_canon.to_string_lossy().replace('\\', "/"));
    Some(MentionFile { display_path, body })
}
