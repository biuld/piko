// ============================================================================
// Built-in Dark Theme
// Maps palette entries to semantic tokens.
// Color values match pi's dark.json theme exactly.
// ============================================================================

import type { DefaultPalette, TuiThemeTokens } from "../schema.js";

/**
 * Build the dark theme tokens from a palette.
 * Components consume these tokens, never raw palette entries.
 */
export function buildDarkTokens(p: DefaultPalette): TuiThemeTokens {
  return {
    text: {
      primary: p.neutral7,
      muted: "#808080",
      dim: p.neutral4,
      inverse: p.neutral0,
      accent: p.accent,
      success: p.green,
      warning: p.yellow,
      error: p.red,
      // pi dark: customMessageLabel = #9575cd
      customLabel: "#9575cd",
    },
    surface: {
      base: p.neutral0,
      selected: "#3a3a4a",
      editor: p.neutral1,
      overlay: p.neutral1,
      // pi dark: toolPendingBg = #282832
      toolPending: "#282832",
      toolSuccess: "#283228",
      toolError: "#3c2828",
      // pi dark: userMessageBg = #343541
      userMessage: "#343541",
      // pi dark: customMessageBg = #2d2838
      customMessage: "#2d2838",
    },
    border: {
      normal: p.neutral4,
      muted: "#505050",
      accent: p.accent,
      error: p.red,
    },
    markdown: {
      heading: "#f0c674",
      link: "#81a2be",
      linkUrl: p.neutral5,
      inlineCode: p.accent,
      codeBlock: p.neutral7,
      codeBlockBorder: p.neutral4,
      quote: "#808080",
      quoteBorder: p.neutral5,
      listBullet: p.accent,
      rule: p.neutral4,
    },
    diff: {
      added: p.green,
      removed: p.red,
      context: "#808080",
      hunk: p.blue,
    },
    tool: {
      // pi dark: toolTitle = text = #d4d4d4
      title: p.neutral7,
      args: p.neutral5,
      path: p.accent,
      // pi dark: toolOutput = gray = #808080
      output: "#808080",
      duration: p.neutral4,
    },
    thinking: {
      // pi dark: thinkingText = gray = #808080
      text: "#808080",
      hiddenLabel: p.neutral4,
      off: p.neutral4,
      low: p.blue,
      medium: p.accent,
      high: p.purple,
    },
  };
}
