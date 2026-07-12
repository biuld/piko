//! Filesystem loaders for prompt material (`AGENTS.md` context files and
//! `.piko/prompts/*.md` templates). Pure parsing/formatting for the loaded
//! material lives in `domain::prompts`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::prompts::{ContextFile, PromptTemplate, parse_frontmatter_result};

pub fn load_context_files(cwd: impl AsRef<Path>) -> Vec<ContextFile> {
    let cwd = cwd
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| cwd.as_ref().to_path_buf());

    let workspace_root = find_workspace_root(&cwd);

    // Collect directories from workspace_root down to cwd, then load
    // AGENTS.md from each in order (general → specific).
    let mut dirs = Vec::new();
    let mut current = cwd.as_path();
    loop {
        dirs.push(current.to_path_buf());
        if current == workspace_root.as_path() {
            break;
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    dirs.reverse();

    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for dir in dirs {
        if let Some(file) = load_from_dir(&dir)
            && seen.insert(file.path.clone())
        {
            files.push(file);
        }
    }
    files
}

fn load_from_dir(dir: &Path) -> Option<ContextFile> {
    for name in ["AGENTS.md", "AGENTS.MD", "CLAUDE.md", "CLAUDE.MD"] {
        let path = dir.join(name);
        if path.is_file()
            && let Ok(content) = fs::read_to_string(&path)
        {
            return Some(ContextFile { path, content });
        }
    }
    None
}

pub fn load_prompt_templates(cwd: impl AsRef<Path>) -> Vec<PromptTemplate> {
    let cwd = cwd.as_ref();
    let project_dir = cwd.join(".piko").join("prompts");
    let global_dir = piko_dir().join("prompts");
    let mut seen = HashSet::new();
    let mut templates = Vec::new();

    for template in load_templates_from_dir(&project_dir) {
        if seen.insert(template.name.clone()) {
            templates.push(template);
        }
    }
    for template in load_templates_from_dir(&global_dir) {
        if seen.insert(template.name.clone()) {
            templates.push(template);
        }
    }
    templates
}

fn load_templates_from_dir(dir: &Path) -> Vec<PromptTemplate> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut templates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok((frontmatter, body)) = parse_frontmatter_result(&raw) else {
            continue;
        };
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unnamed")
            .to_string();
        let description = frontmatter.get("description").cloned().unwrap_or_else(|| {
            let first = body
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("");
            if first.chars().count() > 60 {
                format!("{}...", first.chars().take(60).collect::<String>())
            } else {
                first.to_string()
            }
        });
        templates.push(PromptTemplate {
            name,
            description,
            argument_hint: frontmatter.get("argument-hint").cloned(),
            content: body,
            file_path: path,
        });
    }
    templates.sort_by(|a, b| a.name.cmp(&b.name));
    templates
}

pub(crate) fn home_dir() -> Option<PathBuf> {
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

/// Walk upward from `cwd` looking for a workspace-root marker (`.git`,
/// `.piko`, or `.agents` directory). Stops at the home directory so the
/// search never escapes the user's workspace tree. Returns `cwd` itself
/// when no marker is found.
pub(crate) fn find_workspace_root(cwd: &Path) -> PathBuf {
    let home = home_dir();
    let mut current = Some(cwd);
    while let Some(dir) = current {
        if dir.join(".git").exists() || dir.join(".piko").is_dir() || dir.join(".agents").is_dir() {
            return dir.to_path_buf();
        }
        if home.as_deref() == Some(dir) {
            break;
        }
        current = dir.parent();
    }
    cwd.to_path_buf()
}
