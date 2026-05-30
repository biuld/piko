import type {
  EditorTheme,
  MarkdownTheme,
  SelectListTheme,
  SettingsListTheme,
} from "@earendil-works/pi-tui";
import chalk from "chalk";
import { getHighlightTheme, highlight, supportsLanguage } from "./utils/syntax-highlight.js";

// ============================================================================
// Color tokens — hardcoded dark theme palette (same values as pi's dark.json)
// ============================================================================

type ColorValue = string; // hex "#rrggbb"

const PALETTE: Record<string, ColorValue> = {
  // Core
  accent: "#8abeb7",
  border: "#5f87ff",
  borderAccent: "#00d7ff",
  borderMuted: "#505050",
  success: "#b5bd68",
  error: "#cc6666",
  warning: "#ffff00",
  muted: "#808080",
  dim: "#666666",
  text: "#d4d4d4",
  thinkingText: "#808080",

  // Backgrounds
  selectedBg: "#3a3a4a",
  userMessageBg: "#343541",
  userMessageText: "#d4d4d4",
  customMessageBg: "#2d2838",
  customMessageText: "#d4d4d4",
  customMessageLabel: "#9575cd",
  toolPendingBg: "#282832",
  toolSuccessBg: "#283228",
  toolErrorBg: "#3c2828",
  toolTitle: "#d4d4d4",
  toolOutput: "#808080",

  // Markdown
  mdHeading: "#f0c674",
  mdLink: "#81a2be",
  mdLinkUrl: "#666666",
  mdCode: "#8abeb7",
  mdCodeBlock: "#b5bd68",
  mdCodeBlockBorder: "#808080",
  mdQuote: "#808080",
  mdQuoteBorder: "#808080",
  mdHr: "#808080",
  mdListBullet: "#8abeb7",

  // Diffs
  toolDiffAdded: "#b5bd68",
  toolDiffRemoved: "#cc6666",
  toolDiffContext: "#808080",

  // Syntax highlighting
  syntaxComment: "#6A9955",
  syntaxKeyword: "#569CD6",
  syntaxFunction: "#DCDCAA",
  syntaxVariable: "#9CDCFE",
  syntaxString: "#CE9178",
  syntaxNumber: "#B5CEA8",
  syntaxType: "#4EC9B0",
  syntaxOperator: "#D4D4D4",
  syntaxPunctuation: "#D4D4D4",

  // Thinking levels
  thinkingOff: "#505050",
  thinkingMinimal: "#6e6e6e",
  thinkingLow: "#5f87af",
  thinkingMedium: "#81a2be",
  thinkingHigh: "#b294bb",
  thinkingXhigh: "#d183e8",

  // Bash mode
  bashMode: "#b5bd68",
};

// ============================================================================
// ANSI helpers
// ============================================================================

function hexToRgb(hex: string): { r: number; g: number; b: number } {
  const cleaned = hex.replace("#", "");
  return {
    r: parseInt(cleaned.substring(0, 2), 16),
    g: parseInt(cleaned.substring(2, 4), 16),
    b: parseInt(cleaned.substring(4, 6), 16),
  };
}

function fgAnsi(hex: string): string {
  const { r, g, b } = hexToRgb(hex);
  return `\x1b[38;2;${r};${g};${b}m`;
}

function bgAnsi(hex: string): string {
  const { r, g, b } = hexToRgb(hex);
  return `\x1b[48;2;${r};${g};${b}m`;
}

// ============================================================================
// Theme class
// ============================================================================

export class Theme {
  private fgCache: Map<string, string> = new Map();
  private bgCache: Map<string, string> = new Map();

  constructor(colors: Record<string, ColorValue> = PALETTE) {
    for (const [key, hex] of Object.entries(colors)) {
      this.fgCache.set(key, fgAnsi(hex));
      this.bgCache.set(key, bgAnsi(hex));
    }
  }

  /** Apply foreground color. Resets fg after the text. */
  fg(color: string, text: string): string {
    const ansi = this.fgCache.get(color);
    if (!ansi) return text;
    return `${ansi}${text}\x1b[39m`;
  }

  /** Apply background color. Resets bg after the text. */
  bg(color: string, text: string): string {
    const ansi = this.bgCache.get(color);
    if (!ansi) return text;
    return `${ansi}${text}\x1b[49m`;
  }

  bold(text: string): string {
    return chalk.bold(text);
  }

  italic(text: string): string {
    return chalk.italic(text);
  }

  underline(text: string): string {
    return chalk.underline(text);
  }

  strikethrough(text: string): string {
    return chalk.strikethrough(text);
  }
}

// ============================================================================
// Singleton
// ============================================================================

let currentTheme = new Theme();

export function getTheme(): Theme {
  return currentTheme;
}

export function setTheme(t: Theme): void {
  currentTheme = t;
}

// ============================================================================
// TUI theme factories
// ============================================================================

export function getSelectListTheme(): SelectListTheme {
  const t = currentTheme;
  return {
    selectedPrefix: (text: string) => t.fg("accent", text),
    selectedText: (text: string) => t.fg("accent", text),
    description: (text: string) => t.fg("muted", text),
    scrollInfo: (text: string) => t.fg("muted", text),
    noMatch: (text: string) => t.fg("muted", text),
  };
}

export function getEditorTheme(): EditorTheme {
  const t = currentTheme;
  return {
    borderColor: (text: string) => t.fg("borderMuted", text),
    selectList: getSelectListTheme(),
  };
}

export function getMarkdownTheme(): MarkdownTheme {
  const t = currentTheme;
  return {
    heading: (text: string) => t.fg("mdHeading", text),
    link: (text: string) => t.fg("mdLink", text),
    linkUrl: (text: string) => t.fg("mdLinkUrl", text),
    code: (text: string) => t.fg("mdCode", text),
    codeBlock: (text: string) => t.fg("mdCodeBlock", text),
    codeBlockBorder: (text: string) => t.fg("mdCodeBlockBorder", text),
    quote: (text: string) => t.fg("mdQuote", text),
    quoteBorder: (text: string) => t.fg("mdQuoteBorder", text),
    hr: (text: string) => t.fg("mdHr", text),
    listBullet: (text: string) => t.fg("mdListBullet", text),
    bold: (text: string) => t.bold(text),
    italic: (text: string) => t.italic(text),
    underline: (text: string) => t.underline(text),
    strikethrough: (text: string) => t.strikethrough(text),
    highlightCode: (code: string, lang?: string): string[] => {
      const validLang = lang && supportsLanguage(lang) ? lang : undefined;
      if (!validLang) {
        return code.split("\n").map((line) => t.fg("mdCodeBlock", line));
      }
      try {
        return highlight(code, {
          language: validLang,
          ignoreIllegals: true,
          theme: getHighlightTheme(),
        }).split("\n");
      } catch {
        return code.split("\n").map((line) => t.fg("mdCodeBlock", line));
      }
    },
  };
}

export function getSettingsListTheme(): SettingsListTheme {
  const t = currentTheme;
  return {
    label: (text: string, selected: boolean) => (selected ? t.fg("accent", text) : text),
    value: (text: string, selected: boolean) =>
      selected ? t.fg("accent", text) : t.fg("muted", text),
    description: (text: string) => t.fg("dim", text),
    cursor: t.fg("accent", "→ "),
    hint: (text: string) => t.fg("dim", text),
  };
}
