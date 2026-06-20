/**
 * System prompt construction for piko.
 *
 * Aggregates: skills, context files, prompt templates, tool descriptions,
 * and guidelines into a consistent system prompt structure.
 */

import type { Skill } from "../skills/index.js";
import { formatSkillsForPrompt } from "../skills/index.js";
import type { PromptTemplate } from "./prompt-templates.js";

// ============================================================================
// Types
// ============================================================================

export interface BuildSystemPromptOptions {
  /** Custom system prompt (replaces the default). */
  customPrompt?: string;
  /** Tools to include in the prompt. Default: [read, bash, edit, write]. */
  selectedTools?: string[];
  /** One-line tool snippets keyed by tool name. */
  toolSnippets?: Record<string, string>;
  /** Additional guidelines appended to the defaults. */
  promptGuidelines?: string[];
  /** Text appended to the end of the system prompt. */
  appendSystemPrompt?: string;
  /** Working directory. */
  cwd: string;
  /** Pre-loaded context files (AGENTS.md, etc.). */
  contextFiles?: Array<{ path: string; content: string }>;
  /** Pre-loaded skills. */
  skills?: Skill[];
  /** Prompt templates (from .piko/prompts/). Included as available slash commands. */
  promptTemplates?: PromptTemplate[];
}

// ============================================================================
// Default tool snippets
// ============================================================================

const DEFAULT_TOOL_SNIPPETS: Record<string, string> = {
  read: "Read file contents",
  bash: "Execute bash commands (ls, grep, find, etc.)",
  edit: "Make precise file edits with exact text replacement",
  write: "Create or overwrite files",
  ls: "List directory contents",
  grep: "Search file contents for a pattern",
  find: "Find files by name pattern",
};

// ============================================================================
// Config paths (for pi documentation references)
// ============================================================================

const README_PATH = "/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/README.md";
const DOCS_PATH = "/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/docs";
const EXAMPLES_PATH = "/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/examples";

// ============================================================================
// Builder
// ============================================================================

/** Build the system prompt with tools, guidelines, skills, and context. */
export function buildSystemPrompt(options: BuildSystemPromptOptions): string {
  const {
    customPrompt,
    selectedTools,
    toolSnippets,
    promptGuidelines,
    appendSystemPrompt,
    cwd,
    contextFiles,
    skills,
    promptTemplates,
  } = options;

  const promptCwd = cwd.replace(/\\/g, "/");

  // Current date
  const now = new Date();
  const date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;

  if (customPrompt) {
    let prompt = customPrompt;

    // Append project context files
    if (contextFiles && contextFiles.length > 0) {
      prompt += "\n\n<project_context>\n\n";
      prompt += "Project-specific instructions and guidelines:\n\n";
      for (const { path, content } of contextFiles) {
        prompt += `<project_instructions path="${escapeXml(path)}">\n${content}\n</project_instructions>\n\n`;
      }
      prompt += "</project_context>\n";
    }

    // Append skills section
    const hasRead = !selectedTools || selectedTools.includes("read");
    if (hasRead && skills && skills.length > 0) {
      prompt += formatSkillsForPrompt(skills);
    }

    // Append prompt templates section
    if (promptTemplates && promptTemplates.length > 0) {
      prompt += formatPromptTemplates(promptTemplates);
    }

    prompt += `\nCurrent date: ${date}`;
    prompt += `\nCurrent working directory: ${promptCwd}`;

    return prompt;
  }

  // ---- Default prompt ----
  const tools = selectedTools || ["read", "bash", "edit", "write"];
  const snippets = { ...DEFAULT_TOOL_SNIPPETS, ...toolSnippets };
  const visibleTools = tools.filter((name) => snippets[name]);
  const toolsList =
    visibleTools.length > 0
      ? visibleTools.map((name) => `- ${name}: ${snippets[name]}`).join("\n")
      : "(none)";

  // Build guidelines
  const guidelines = buildGuidelines(tools, promptGuidelines);

  const hasRead = tools.includes("read");

  // Main prompt body
  let prompt = `You are an expert coding assistant operating inside piko, a coding agent harness. You help users by reading files, executing commands, editing code, and writing new files.

Available tools:
${toolsList}

Guidelines:
${guidelines}

Pi documentation (read only when the user asks about pi itself, its SDK, extensions, themes, skills, or TUI):
- Main documentation: ${README_PATH}
- Additional docs: ${DOCS_PATH}
- Examples: ${EXAMPLES_PATH} (extensions, custom tools, SDK)
- When reading pi docs or examples, resolve docs/... under Additional docs and examples/... under Examples, not the current working directory
- When asked about: extensions (docs/extensions.md, examples/extensions/), themes (docs/themes.md), skills (docs/skills.md), prompt templates (docs/prompt-templates.md), TUI components (docs/tui.md), keybindings (docs/keybindings.md), SDK integrations (docs/sdk.md), custom providers (docs/custom-provider.md), adding models (docs/models.md), pi packages (docs/packages.md)
- When working on pi topics, read the docs and examples, and follow .md cross-references before implementing
- Always read pi .md files completely and follow links to related docs (e.g., tui.md for TUI API details)`;

  // Append system prompt supplement
  if (appendSystemPrompt) {
    prompt += `\n\n${appendSystemPrompt}`;
  }

  // Append project context files
  if (contextFiles && contextFiles.length > 0) {
    prompt += "\n\n<project_context>\n\n";
    prompt += "Project-specific instructions and guidelines:\n\n";
    for (const { path, content } of contextFiles) {
      prompt += `<project_instructions path="${escapeXml(path)}">\n${content}\n</project_instructions>\n\n`;
    }
    prompt += "</project_context>\n";
  }

  // Append skills section
  if (hasRead && skills && skills.length > 0) {
    prompt += formatSkillsForPrompt(skills);
  }

  // Append prompt templates section
  if (promptTemplates && promptTemplates.length > 0) {
    prompt += formatPromptTemplates(promptTemplates);
  }

  // Date and working directory
  prompt += `\nCurrent date: ${date}`;
  prompt += `\nCurrent working directory: ${promptCwd}`;

  return prompt;
}

// ============================================================================
// Prompt templates formatter
// ============================================================================

/** Format prompt templates as a prompt section listing available slash commands. */
function formatPromptTemplates(templates: PromptTemplate[]): string {
  let section = "\n\n## Prompt Templates\n\n";
  section += "The following prompt templates are available as slash commands:\n";
  for (const t of templates) {
    const hint = t.argumentHint ? ` ${t.argumentHint}` : "";
    section += `- /${t.name}${hint} — ${t.description}\n`;
  }
  section +=
    "\nWhen the user types a /command matching one of these templates, expand it using the template content.";
  return section;
}

// ============================================================================
// Guidelines builder
// ============================================================================

function buildGuidelines(_tools: string[], extra: string[] = []): string {
  const set = new Set<string>();
  const list: string[] = [];

  const add = (g: string) => {
    if (set.has(g)) return;
    set.add(g);
    list.push(g);
  };

  add("Use bash for file operations like ls, rg, find");
  add("Use read to examine files instead of cat or sed.");
  add("Use edit for precise changes (edits[].oldText must match exactly)");
  add(
    "When changing multiple separate locations in one file, use one edit call with multiple entries in edits[] instead of multiple edit calls",
  );
  add(
    "Each edits[].oldText is matched against the original file, not after earlier edits are applied. Do not emit overlapping or nested edits. Merge nearby changes into one edit.",
  );
  add(
    "Keep edits[].oldText as small as possible while still being unique in the file. Do not pad with large unchanged regions.",
  );
  add("Use write only for new files or complete rewrites.");
  add("Be concise in your responses");
  add("Show file paths clearly when working with files");

  // Extra guidelines
  for (const g of extra) {
    const normalized = g.trim();
    if (normalized) add(normalized);
  }

  return list.map((g) => `- ${g}`).join("\n");
}

// ============================================================================
// Helpers
// ============================================================================

function escapeXml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}
