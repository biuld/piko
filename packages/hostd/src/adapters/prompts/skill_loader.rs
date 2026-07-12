//! Filesystem loader for `.piko/skills/` and `.agents/skills/` skill
//! definitions. Pure skill types/formatting live in `domain::prompts::skills`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::adapters::prompts::loader::home_dir;
use crate::domain::prompts::parse_frontmatter_result;
use crate::domain::prompts::skills::{
    LoadSkillsResult, Skill, SkillDiagnostic, SkillDiagnosticKind, validate_name,
};

pub fn load_skills(cwd: impl AsRef<Path>) -> LoadSkillsResult {
    let cwd = cwd.as_ref();
    let home = home_dir();
    let mut by_name = HashMap::new();
    let mut diagnostics = Vec::new();

    // Traverse from cwd upward to (and including) the home directory.
    // Skills found closer to cwd win over same-named skills higher up.
    // Both .piko/skills/ and .agents/skills/ are checked at every level.
    let mut current = Some(cwd);
    while let Some(dir) = current {
        for config_dir in [".piko", ".agents"] {
            let skills_dir = dir.join(config_dir).join("skills");
            let result = scan_directory(&skills_dir, true);
            diagnostics.extend(result.diagnostics);
            for skill in result.skills {
                by_name.entry(skill.name.clone()).or_insert(skill);
            }
        }
        if home.as_deref() == Some(dir) {
            break;
        }
        current = dir.parent();
    }

    LoadSkillsResult {
        skills: by_name.into_values().collect(),
        diagnostics,
    }
}

fn scan_directory(dir: &Path, include_root_files: bool) -> LoadSkillsResult {
    let Ok(entries) = fs::read_dir(dir) else {
        return LoadSkillsResult::default();
    };
    let entries = entries.flatten().collect::<Vec<_>>();
    let mut result = LoadSkillsResult::default();
    let mut seen = HashSet::new();

    for entry in &entries {
        if entry.file_name() == "SKILL.md" && entry.path().is_file() {
            let loaded = load_skill_from_file(&entry.path());
            result.diagnostics.extend(loaded.diagnostics);
            if let Some(skill) = loaded.skills.into_iter().next() {
                result.skills.push(skill);
            }
            return result;
        }
    }

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            let loaded = scan_directory(&path, false);
            for skill in loaded.skills {
                if seen.insert(skill.name.clone()) {
                    result.skills.push(skill);
                }
            }
            result.diagnostics.extend(loaded.diagnostics);
        } else if include_root_files && path.extension().and_then(|ext| ext.to_str()) == Some("md")
        {
            let loaded = load_skill_from_file(&path);
            for skill in loaded.skills {
                if seen.insert(skill.name.clone()) {
                    result.skills.push(skill);
                }
            }
            result.diagnostics.extend(loaded.diagnostics);
        }
    }
    result
}

fn load_skill_from_file(path: &Path) -> LoadSkillsResult {
    let mut result = LoadSkillsResult::default();
    let Ok(raw) = fs::read_to_string(path) else {
        result.diagnostics.push(SkillDiagnostic {
            kind: SkillDiagnosticKind::Warning,
            message: "failed to read skill file".to_string(),
            path: path.to_path_buf(),
        });
        return result;
    };
    let (frontmatter, _) = match parse_frontmatter_result(&raw) {
        Ok(parsed) => parsed,
        Err(message) => {
            result.diagnostics.push(SkillDiagnostic {
                kind: SkillDiagnosticKind::Warning,
                message,
                path: path.to_path_buf(),
            });
            return result;
        }
    };
    let base_dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let fallback_name = base_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unnamed")
        .to_string();
    let name = frontmatter.get("name").cloned().unwrap_or(fallback_name);
    let description = frontmatter.get("description").cloned().unwrap_or_default();

    for error in validate_name(&name) {
        result.diagnostics.push(SkillDiagnostic {
            kind: SkillDiagnosticKind::Warning,
            message: error,
            path: path.to_path_buf(),
        });
    }
    if description.trim().is_empty() {
        result.diagnostics.push(SkillDiagnostic {
            kind: SkillDiagnosticKind::Warning,
            message: "description is required".to_string(),
            path: path.to_path_buf(),
        });
        return result;
    }
    if description.len() > 1024 {
        result.diagnostics.push(SkillDiagnostic {
            kind: SkillDiagnosticKind::Warning,
            message: format!(
                "description exceeds 1024 characters ({})",
                description.len()
            ),
            path: path.to_path_buf(),
        });
    }

    result.skills.push(Skill {
        name,
        description,
        file_path: path.to_path_buf(),
        base_dir,
        disable_model_invocation: frontmatter
            .get("disable-model-invocation")
            .is_some_and(|value| value == "true"),
        model_override: frontmatter.get("model").cloned(),
        thinking_level: frontmatter.get("thinking").cloned(),
        active_tools: frontmatter.get("tools").cloned(),
    });
    result
}
