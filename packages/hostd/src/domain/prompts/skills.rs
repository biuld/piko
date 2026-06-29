use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::prompts::{home_dir, parse_frontmatter_result};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillResource {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub file_path: PathBuf,
    pub base_dir: PathBuf,
    pub disable_model_invocation: bool,
    pub model_override: Option<String>,
    pub thinking_level: Option<String>,
    pub active_tools: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillDiagnostic {
    pub kind: SkillDiagnosticKind,
    pub message: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkillDiagnosticKind {
    Warning,
    Error,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadSkillsResult {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

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

pub fn format_skills_for_prompt(skills: &[Skill]) -> String {
    let visible = skills
        .iter()
        .filter(|skill| !skill.disable_model_invocation)
        .collect::<Vec<_>>();
    if visible.is_empty() {
        return String::new();
    }
    let mut lines = vec![
        "\n\nThe following skills provide specialized instructions for specific tasks.".to_string(),
        "Use the read tool to load a skill's file when the task matches its description.".to_string(),
        "When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.".to_string(),
        String::new(),
        "<available_skills>".to_string(),
    ];
    for skill in visible {
        lines.push("  <skill>".to_string());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
        lines.push(format!(
            "    <description>{}</description>",
            escape_xml(&skill.description)
        ));
        lines.push(format!(
            "    <location>{}</location>",
            escape_xml(&skill.file_path.to_string_lossy())
        ));
        lines.push("  </skill>".to_string());
    }
    lines.push("</available_skills>".to_string());
    lines.join("\n")
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

fn validate_name(name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if name.len() > 64 {
        errors.push(format!("name exceeds 64 characters ({})", name.len()));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        errors.push(
            "name contains invalid characters (must be lowercase a-z, 0-9, hyphens only)"
                .to_string(),
        );
    }
    if name.starts_with('-') || name.ends_with('-') {
        errors.push("name must not start or end with a hyphen".to_string());
    }
    if name.contains("--") {
        errors.push("name must not contain consecutive hyphens".to_string());
    }
    errors
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
