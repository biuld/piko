/**
 * Prompt template loader — loads .md files from .piko/prompts/
 * and formats them for use with slash-command-like expansion.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { getPikoDir } from "../session/index.js";
import { parseFrontmatter } from "../utils/index.js";

// ============================================================================
// Types
// ============================================================================

/** A loaded prompt template. */
export interface PromptTemplate {
  /** Template name (derived from filename without .md extension). */
  name: string;
  /** Short description. */
  description: string;
  /** Optional argument hint for the template. */
  argumentHint?: string;
  /** Template body (after frontmatter). */
  content: string;
  /** Absolute path to the template file. */
  filePath: string;
}

// ============================================================================
// Argument parsing / substitution
// ============================================================================

/**
 * Parse command arguments respecting quoted strings (bash-style).
 */
export function parseCommandArgs(argsString: string): string[] {
  const args: string[] = [];
  let current = "";
  let inQuote: string | null = null;

  for (let i = 0; i < argsString.length; i++) {
    const char = argsString[i];

    if (inQuote) {
      if (char === inQuote) {
        inQuote = null;
      } else {
        current += char;
      }
    } else if (char === '"' || char === "'") {
      inQuote = char;
    } else if (/\s/.test(char)) {
      if (current) {
        args.push(current);
        current = "";
      }
    } else {
      current += char;
    }
  }

  if (current) {
    args.push(current);
  }

  return args;
}

/**
 * Substitute argument placeholders in template content.
 * Supports $1, $2, ... for positional args; $@ and $ARGUMENTS for all args;
 * ${@:N} for args from Nth onwards; ${@:N:L} for L args starting from Nth.
 */
export function substituteArgs(content: string, args: string[]): string {
  let result = content;

  // Replace $1, $2, etc. FIRST
  result = result.replace(/\$(\d+)/g, (_, num) => {
    const index = parseInt(num, 10) - 1;
    return args[index] ?? "";
  });

  // Replace ${@:start} or ${@:start:length}
  result = result.replace(/\$\{@:(\d+)(?::(\d+))?\}/g, (_, startStr, lengthStr) => {
    let start = parseInt(startStr, 10) - 1;
    if (start < 0) start = 0;
    if (lengthStr) {
      return args.slice(start, start + parseInt(lengthStr, 10)).join(" ");
    }
    return args.slice(start).join(" ");
  });

  // Replace $ARGUMENTS and $@
  const allArgs = args.join(" ");
  result = result.replace(/\$ARGUMENTS/g, allArgs);
  result = result.replace(/\$@/g, allArgs);

  return result;
}

// ============================================================================
// Loading
// ============================================================================

const CONFIG_DIR_NAME = ".piko";

function loadTemplateFromFile(filePath: string): PromptTemplate | null {
  try {
    const rawContent = readFileSync(filePath, "utf-8");
    const { frontmatter, body } = parseFrontmatter<Record<string, string>>(rawContent);

    const name = basename(filePath).replace(/\.md$/, "");

    let description = frontmatter.description || "";
    if (!description) {
      const firstLine = body.split("\n").find((line) => line.trim());
      if (firstLine) {
        description = firstLine.slice(0, 60);
        if (firstLine.length > 60) description += "...";
      }
    }

    return {
      name,
      description,
      ...(frontmatter["argument-hint"] && { argumentHint: frontmatter["argument-hint"] }),
      content: body,
      filePath,
    };
  } catch {
    return null;
  }
}

function loadTemplatesFromDir(dir: string): PromptTemplate[] {
  const templates: PromptTemplate[] = [];

  if (!existsSync(dir)) return templates;

  try {
    const entries = readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
      if (!entry.name.endsWith(".md")) continue;
      const fullPath = join(dir, entry.name);

      const isFile = entry.isFile() || (entry.isSymbolicLink() && statSync(fullPath).isFile());
      if (!isFile) continue;

      const template = loadTemplateFromFile(fullPath);
      if (template) templates.push(template);
    }
  } catch {
    // Ignore read failures
  }

  return templates;
}

export interface LoadPromptTemplatesOptions {
  /** Working directory for project-local templates. */
  cwd: string;
}

/**
 * Load all prompt templates from:
 * 1. Project: cwd/.piko/prompts/
 * 2. Global: ~/.piko/prompts/
 *
 * Project templates take precedence (loaded first).
 */
export function loadPromptTemplates(options: LoadPromptTemplatesOptions): PromptTemplate[] {
  const resolvedCwd = resolve(options.cwd);

  const projectDir = resolve(resolvedCwd, CONFIG_DIR_NAME, "prompts");
  const globalDir = join(getPikoDir(), "prompts");

  const seen = new Set<string>();
  const templates: PromptTemplate[] = [];

  // Project templates first
  for (const t of loadTemplatesFromDir(projectDir)) {
    if (seen.has(t.name)) continue;
    seen.add(t.name);
    templates.push(t);
  }

  // Then global templates
  for (const t of loadTemplatesFromDir(globalDir)) {
    if (seen.has(t.name)) continue;
    seen.add(t.name);
    templates.push(t);
  }

  return templates;
}

/**
 * If the text starts with "/<templateName>", expand it using the matching template.
 * Returns the expanded content or the original text if no match.
 */
export function expandPromptTemplate(text: string, templates: PromptTemplate[]): string {
  if (!text.startsWith("/")) return text;

  const match = text.match(/^\/([^\s]+)(?:\s+([\s\S]*))?$/);
  if (!match) return text;

  const templateName = match[1];
  const argsString = match[2] ?? "";

  const template = templates.find((t) => t.name === templateName);
  if (template) {
    const args = parseCommandArgs(argsString);
    return substituteArgs(template.content, args);
  }

  return text;
}
