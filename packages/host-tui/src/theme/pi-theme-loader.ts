// ============================================================================
// Pi theme loader — converts pi-format theme JSON to piko ResolvedTuiTheme
//
// Pi format:
//   { "name": "dark", "vars": { ... }, "colors": { "userMessageBg": "userMsgBg", ... } }
//
// Piko format:
//   { "name": "dark", "tokens": { "surface": { "userMessage": "#343541", ... } } }
// ============================================================================

import * as fs from "node:fs";
import * as path from "node:path";
import { resolveTheme } from "./resolve.js";
import type { ResolvedTuiTheme, TuiColorValue, TuiThemeDefinition } from "./schema.js";

// ============================================================================
// Pi JSON types
// ============================================================================

interface PiThemeJson {
  $schema?: string;
  name: string;
  vars?: Record<string, string>;
  colors: Record<string, string>;
  export?: Record<string, string>;
}

// ============================================================================
// Pi flat key → piko token path mapping
// ============================================================================

const PI_TO_PIKO_MAP: Record<string, string> = {
  // Text
  text: "text.primary",
  muted: "text.muted",
  dim: "text.dim",
  accent: "text.accent",
  success: "text.success",
  error: "text.error",
  warning: "text.warning",
  thinkingText: "thinking.text",
  customMessageLabel: "text.customLabel",
  userMessageText: "text.primary", // user message text uses primary
  customMessageText: "text.primary", // custom message text uses primary

  // Surface / backgrounds
  selectedBg: "surface.selected",
  userMessageBg: "surface.userMessage",
  customMessageBg: "surface.customMessage",
  toolPendingBg: "surface.toolPending",
  toolSuccessBg: "surface.toolSuccess",
  toolErrorBg: "surface.toolError",

  // Tool
  toolTitle: "tool.title",
  toolOutput: "tool.output",

  // Border
  border: "border.normal",
  borderMuted: "border.muted",
  borderAccent: "border.accent",

  // Markdown
  mdHeading: "markdown.heading",
  mdLink: "markdown.link",
  mdLinkUrl: "markdown.linkUrl",
  mdCode: "markdown.inlineCode",
  mdCodeBlock: "markdown.codeBlock",
  mdCodeBlockBorder: "markdown.codeBlockBorder",
  mdQuote: "markdown.quote",
  mdQuoteBorder: "markdown.quoteBorder",
  mdHr: "markdown.rule",
  mdListBullet: "markdown.listBullet",

  // Diff
  toolDiffAdded: "diff.added",
  toolDiffRemoved: "diff.removed",
  toolDiffContext: "diff.context",

  // Thinking borders
  thinkingOff: "thinking.off",
  thinkingMinimal: "thinking.off",
  thinkingLow: "thinking.low",
  thinkingMedium: "thinking.medium",
  thinkingHigh: "thinking.high",
  thinkingXhigh: "thinking.high",
};

// ============================================================================
// Var resolution
// ============================================================================

/**
 * Resolve a pi color value which may be a hex color or a var reference.
 * Var refs are strings that don't start with "#" (e.g. "userMsgBg" → "#343541").
 */
function resolvePiColor(
  value: string,
  vars: Record<string, string>,
  visited: Set<string> = new Set(),
): string {
  // Hex color or empty
  if (value.startsWith("#") || value === "") return value;

  // Var reference
  if (visited.has(value)) {
    throw new Error(`Circular variable reference: ${value}`);
  }

  const resolved = vars[value];
  if (resolved === undefined) {
    // Not a var ref — might be a raw color name. Treat as-is.
    return value;
  }

  visited.add(value);
  return resolvePiColor(resolved, vars, visited);
}

// ============================================================================
// Token path construction from flat pi colors
// ============================================================================

/**
 * Map pi flat colors to piko nested token structure,
 * resolving var references along the way.
 */
function piColorsToTokens(
  colors: Record<string, string>,
  vars: Record<string, string>,
): Partial<TuiThemeDefinition["tokens"]> {
  const tokens: Record<string, Record<string, TuiColorValue>> = {};

  for (const [piKey, piValue] of Object.entries(colors)) {
    const pikoPath = PI_TO_PIKO_MAP[piKey];
    if (!pikoPath) continue; // skip unmapped keys (syntax, bashMode, etc.)

    const resolved = resolvePiColor(piValue, vars);
    if (!resolved.startsWith("#")) continue; // skip non-color values

    const [section, key] = pikoPath.split(".") as [string, string];
    if (!tokens[section]) tokens[section] = {};
    tokens[section][key] = resolved as TuiColorValue;
  }

  return tokens as TuiThemeDefinition["tokens"];
}

// ============================================================================
// Loader
// ============================================================================

/**
 * Load a pi-format theme JSON file and convert to piko ResolvedTuiTheme.
 */
export function loadPiThemeFile(filePath: string): ResolvedTuiTheme {
  const raw = fs.readFileSync(filePath, "utf-8");
  const json: PiThemeJson = JSON.parse(raw);

  if (!json.colors || typeof json.colors !== "object") {
    throw new Error(`Invalid pi theme file: missing "colors" object in ${filePath}`);
  }

  const vars = json.vars ?? {};
  const tokens = piColorsToTokens(json.colors, vars);

  const definition: TuiThemeDefinition = {
    name: json.name ?? path.basename(filePath, ".json"),
    tokens,
  };

  return resolveTheme(definition);
}

// ============================================================================
// Theme discovery
// ============================================================================

/**
 * Scan directories for pi-format theme JSON files.
 * Returns a map of theme name → file path.
 */
export function findPiThemes(extraDirs: string[] = []): Map<string, string> {
  const themes = new Map<string, string>();

  const scanDir = (dir: string) => {
    if (!fs.existsSync(dir)) return;
    for (const entry of fs.readdirSync(dir)) {
      if (entry.endsWith(".json")) {
        const fullPath = path.join(dir, entry);
        try {
          const raw = fs.readFileSync(fullPath, "utf-8");
          const json = JSON.parse(raw);
          if (json.colors && typeof json.colors === "object") {
            const name = json.name ?? path.basename(entry, ".json");
            themes.set(name, fullPath);
          }
        } catch {
          // skip invalid files
        }
      }
    }
  };

  for (const dir of extraDirs) {
    scanDir(dir);
  }

  return themes;
}
