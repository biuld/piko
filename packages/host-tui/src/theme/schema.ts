// ============================================================================
// Theme schema — type definitions for palette, tokens, and resolved theme
// ============================================================================

// ============================================================================
// Color values
// ============================================================================

/** A raw color: hex string, 256-color index, or reference to another palette entry. */
export type TuiColorValue = `#${string}` | number | { ref: string };

/** Truecolor hex string */
export type HexColor = `#${string}`;

// ============================================================================
// Terminal capability
// ============================================================================

export type TuiColorMode = "truecolor" | "256" | "16" | "none";

// ============================================================================
// Palette
// ============================================================================

export interface TuiPalette {
  name: string;
  colors: Record<string, TuiColorValue>;
}

export interface DefaultPalette {
  neutral0: TuiColorValue;
  neutral1: TuiColorValue;
  neutral2: TuiColorValue;
  neutral3: TuiColorValue;
  neutral4: TuiColorValue;
  neutral5: TuiColorValue;
  neutral6: TuiColorValue;
  neutral7: TuiColorValue;
  accent: TuiColorValue;
  accentMuted: TuiColorValue;
  green: TuiColorValue;
  yellow: TuiColorValue;
  red: TuiColorValue;
  blue: TuiColorValue;
  purple: TuiColorValue;
  cyan: TuiColorValue;
}

// ============================================================================
// Semantic tokens
// ============================================================================

export interface TuiThemeTokens {
  text: {
    primary: TuiColorValue;
    muted: TuiColorValue;
    dim: TuiColorValue;
    inverse: TuiColorValue;
    accent: TuiColorValue;
    success: TuiColorValue;
    warning: TuiColorValue;
    error: TuiColorValue;
  };
  surface: {
    base: TuiColorValue;
    selected: TuiColorValue;
    editor: TuiColorValue;
    overlay: TuiColorValue;
    toolPending: TuiColorValue;
    toolSuccess: TuiColorValue;
    toolError: TuiColorValue;
  };
  border: {
    normal: TuiColorValue;
    muted: TuiColorValue;
    accent: TuiColorValue;
    error: TuiColorValue;
  };
  markdown: {
    heading: TuiColorValue;
    link: TuiColorValue;
    linkUrl: TuiColorValue;
    inlineCode: TuiColorValue;
    codeBlock: TuiColorValue;
    codeBlockBorder: TuiColorValue;
    quote: TuiColorValue;
    quoteBorder: TuiColorValue;
    listBullet: TuiColorValue;
    rule: TuiColorValue;
  };
  diff: {
    added: TuiColorValue;
    removed: TuiColorValue;
    context: TuiColorValue;
    hunk: TuiColorValue;
  };
  tool: {
    title: TuiColorValue;
    args: TuiColorValue;
    path: TuiColorValue;
    output: TuiColorValue;
    duration: TuiColorValue;
  };
  thinking: {
    text: TuiColorValue;
    hiddenLabel: TuiColorValue;
    off: TuiColorValue;
    low: TuiColorValue;
    medium: TuiColorValue;
    high: TuiColorValue;
  };
}

// ============================================================================
// Token paths for component consumption
// ============================================================================

/** Dot-separated path into TuiThemeTokens, e.g. "text.muted" or "border.accent" */
export type TuiTokenPath = string;

export type TuiForegroundToken = TuiTokenPath;
export type TuiBackgroundToken = TuiTokenPath;

// ============================================================================
// Theme definition (JSON-serializable)
// ============================================================================

export interface TuiThemeDefinition {
  $schema?: string;
  name: string;
  extends?: string;
  palette?: Record<string, TuiColorValue>;
  tokens?: Partial<{
    text: Partial<TuiThemeTokens["text"]>;
    surface: Partial<TuiThemeTokens["surface"]>;
    border: Partial<TuiThemeTokens["border"]>;
    markdown: Partial<TuiThemeTokens["markdown"]>;
    diff: Partial<TuiThemeTokens["diff"]>;
    tool: Partial<TuiThemeTokens["tool"]>;
    thinking: Partial<TuiThemeTokens["thinking"]>;
  }>;
}

// ============================================================================
// Resolved theme (ready for consumption by components)
// ============================================================================

export interface ResolvedTuiTheme {
  name: string;
  palette: Record<string, HexColor>;
  tokens: TuiThemeTokens;
  colorMode: TuiColorMode;

  /** Resolve a token path to a concrete color string for the current terminal. */
  color(path: TuiTokenPath): string;

  /** Convenience: resolve a foreground text color. */
  fg(token: TuiTokenPath): string;

  /** Convenience: resolve a background color. */
  bg(token: TuiTokenPath): string;
}
