// ============================================================================
// Theme resolver — resolves palette refs, merges overrides, handles capability
// ============================================================================

import { darkPalette } from "./palettes.js";
import type {
  HexColor,
  ResolvedTuiTheme,
  TuiColorMode,
  TuiColorValue,
  TuiThemeDefinition,
  TuiThemeTokens,
  TuiTokenPath,
} from "./schema.js";
import { buildDarkTokens } from "./themes/dark.js";

// ============================================================================
// Color capability conversion
// ============================================================================

/**
 * Convert a hex color to the nearest 256-color ANSI index.
 * Uses simple Euclidean distance in RGB space.
 */
function hexTo256(hex: HexColor): number {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);

  // 6x6x6 color cube (16-231)
  const r6 = Math.round((r / 255) * 5);
  const g6 = Math.round((g / 255) * 5);
  const b6 = Math.round((b / 255) * 5);
  return 16 + 36 * r6 + 6 * g6 + b6;
}

/**
 * Convert a hex color to string for given color mode.
 */
function hexForMode(hex: HexColor, mode: TuiColorMode): string {
  switch (mode) {
    case "truecolor":
      return hex;
    case "256":
      return String(hexTo256(hex));
    case "16":
      return mapTo16Color(hex);
    case "none":
    default:
      return "";
  }
}

function mapTo16Color(hex: HexColor): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const lum = 0.299 * r + 0.587 * g + 0.114 * b;

  if (lum < 50) return "0"; // black
  if (lum < 120) return "8"; // bright black
  if (lum < 180) return "7"; // white
  return "15"; // bright white
}

// ============================================================================
// Detect terminal capability
// ============================================================================

export function detectColorMode(): TuiColorMode {
  if (process.env.NO_COLOR) return "none";
  if (process.env.FORCE_COLOR === "3") return "truecolor";
  if (process.env.FORCE_COLOR === "2") return "256";
  if (process.env.FORCE_COLOR === "1") return "16";

  const term = process.env.TERM ?? "";
  const colorTerm = process.env.COLORTERM ?? "";

  if (colorTerm === "truecolor" || colorTerm === "24bit") return "truecolor";
  if (term.includes("256color")) return "256";

  return "truecolor"; // default for modern terminals
}

// ============================================================================
// Palette resolution
// ============================================================================

/**
 * Resolve a TuiColorValue to a concrete hex string, following refs.
 * Throws on cycles.
 */
function resolveColorValue(
  value: TuiColorValue,
  palette: Record<string, HexColor>,
  seen: Set<string> = new Set(),
): HexColor {
  if (typeof value === "number") {
    return `#${value.toString(16).padStart(6, "0")}` as HexColor;
  }
  if (typeof value === "string" && value.startsWith("#")) {
    return value as HexColor;
  }
  if (typeof value === "object" && "ref" in value) {
    if (seen.has(value.ref)) {
      throw new Error(`Cyclic palette reference: ${value.ref}`);
    }
    seen.add(value.ref);
    const target = palette[value.ref];
    if (!target) {
      throw new Error(`Unknown palette reference: ${value.ref}`);
    }
    return resolveColorValue(target, palette, seen);
  }
  throw new Error(`Invalid color value: ${JSON.stringify(value)}`);
}

/**
 * Resolve all palette entries to hex colors.
 */
function resolvePalette(
  colors: Record<string, TuiColorValue>,
  basePalette?: Record<string, HexColor>,
): Record<string, HexColor> {
  const merged: Record<string, HexColor> = { ...basePalette };

  // First pass: collect all hex values
  for (const [key, value] of Object.entries(colors)) {
    if (typeof value === "string" && value.startsWith("#")) {
      merged[key] = value as HexColor;
    }
  }

  // Second pass: resolve refs
  for (const [key, value] of Object.entries(colors)) {
    if (typeof value === "string" && value.startsWith("#")) continue;
    merged[key] = resolveColorValue(value, merged);
  }

  return merged;
}

// ============================================================================
// Token resolution
// ============================================================================

/**
 * Resolve tokens: replace palette refs with concrete colors.
 */
function resolveTokens(tokens: TuiThemeTokens, palette: Record<string, HexColor>): TuiThemeTokens {
  const resolve = (v: TuiColorValue): HexColor => resolveColorValue(v, palette);

  return {
    text: {
      primary: resolve(tokens.text.primary),
      muted: resolve(tokens.text.muted),
      dim: resolve(tokens.text.dim),
      inverse: resolve(tokens.text.inverse),
      accent: resolve(tokens.text.accent),
      success: resolve(tokens.text.success),
      warning: resolve(tokens.text.warning),
      error: resolve(tokens.text.error),
    },
    surface: {
      base: resolve(tokens.surface.base),
      selected: resolve(tokens.surface.selected),
      editor: resolve(tokens.surface.editor),
      overlay: resolve(tokens.surface.overlay),
      toolPending: resolve(tokens.surface.toolPending),
      toolSuccess: resolve(tokens.surface.toolSuccess),
      toolError: resolve(tokens.surface.toolError),
    },
    border: {
      normal: resolve(tokens.border.normal),
      muted: resolve(tokens.border.muted),
      accent: resolve(tokens.border.accent),
      error: resolve(tokens.border.error),
    },
    markdown: {
      heading: resolve(tokens.markdown.heading),
      link: resolve(tokens.markdown.link),
      linkUrl: resolve(tokens.markdown.linkUrl),
      inlineCode: resolve(tokens.markdown.inlineCode),
      codeBlock: resolve(tokens.markdown.codeBlock),
      codeBlockBorder: resolve(tokens.markdown.codeBlockBorder),
      quote: resolve(tokens.markdown.quote),
      quoteBorder: resolve(tokens.markdown.quoteBorder),
      listBullet: resolve(tokens.markdown.listBullet),
      rule: resolve(tokens.markdown.rule),
    },
    diff: {
      added: resolve(tokens.diff.added),
      removed: resolve(tokens.diff.removed),
      context: resolve(tokens.diff.context),
      hunk: resolve(tokens.diff.hunk),
    },
    tool: {
      title: resolve(tokens.tool.title),
      args: resolve(tokens.tool.args),
      path: resolve(tokens.tool.path),
      output: resolve(tokens.tool.output),
      duration: resolve(tokens.tool.duration),
    },
    thinking: {
      text: resolve(tokens.thinking.text),
      hiddenLabel: resolve(tokens.thinking.hiddenLabel),
      off: resolve(tokens.thinking.off),
      low: resolve(tokens.thinking.low),
      medium: resolve(tokens.thinking.medium),
      high: resolve(tokens.thinking.high),
    },
  };
}

// ============================================================================
// Token path lookup
// ============================================================================

/**
 * Navigate a dot-separated token path to find a color value.
 */
function getTokenValue(tokens: TuiThemeTokens, path: TuiTokenPath): TuiColorValue {
  const parts = path.split(".");
  let current: unknown = tokens;
  for (const part of parts) {
    if (current && typeof current === "object" && part in current) {
      current = (current as Record<string, unknown>)[part];
    } else {
      throw new Error(`Unknown token path: ${path}`);
    }
  }
  return current as TuiColorValue;
}

// ============================================================================
// Theme construction
// ============================================================================

/**
 * Build a resolved theme from a definition.
 */
export function resolveTheme(
  definition: TuiThemeDefinition,
  colorMode?: TuiColorMode,
): ResolvedTuiTheme {
  const mode = colorMode ?? detectColorMode();

  // Build base palette
  const basePalette = resolvePalette(darkPalette as unknown as Record<string, TuiColorValue>);

  // Apply definition palette overrides
  const palette = definition.palette
    ? resolvePalette(definition.palette, basePalette)
    : basePalette;

  // Build base tokens
  const baseTokens = buildDarkTokens(darkPalette);

  // Apply definition token overrides
  const tokens = definition.tokens
    ? mergeTokenOverrides(baseTokens, definition.tokens, palette)
    : baseTokens;

  // Resolve all token values through palette
  const resolvedTokens = resolveTokens(tokens, palette);

  // Create the theme object
  const theme: ResolvedTuiTheme = {
    name: definition.name,
    palette,
    tokens: resolvedTokens,
    colorMode: mode,

    color(path: TuiTokenPath): string {
      const value = getTokenValue(resolvedTokens, path);
      return hexForMode(value as HexColor, mode);
    },

    fg(token: TuiTokenPath): string {
      return theme.color(token);
    },

    bg(token: TuiTokenPath): string {
      return theme.color(token);
    },
  };

  return theme;
}

function mergeTokenOverrides(
  base: TuiThemeTokens,
  overrides: TuiThemeDefinition["tokens"] = {},
  _palette: Record<string, HexColor>,
): TuiThemeTokens {
  return {
    text: { ...base.text, ...overrides.text },
    surface: { ...base.surface, ...overrides.surface },
    border: { ...base.border, ...overrides.border },
    markdown: { ...base.markdown, ...overrides.markdown },
    diff: { ...base.diff, ...overrides.diff },
    tool: { ...base.tool, ...overrides.tool },
    thinking: { ...base.thinking, ...overrides.thinking },
  };
}

// ============================================================================
// Built-in themes
// ============================================================================

let _defaultTheme: ResolvedTuiTheme | null = null;

export function getDefaultTheme(): ResolvedTuiTheme {
  if (!_defaultTheme) {
    _defaultTheme = resolveTheme({ name: "piko-dark" });
  }
  return _defaultTheme;
}

export function setDefaultTheme(theme: ResolvedTuiTheme): void {
  _defaultTheme = theme;
}
