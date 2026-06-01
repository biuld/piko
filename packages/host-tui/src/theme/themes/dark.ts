// ============================================================================
// Built-in Dark Theme
// Maps palette entries to semantic tokens
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
      muted: p.neutral5,
      dim: p.neutral4,
      inverse: p.neutral0,
      accent: p.accent,
      success: p.green,
      warning: p.yellow,
      error: p.red,
    },
    surface: {
      base: p.neutral0,
      selected: p.neutral2,
      editor: p.neutral1,
      overlay: p.neutral1,
      toolPending: p.neutral2,
      toolSuccess: "#283228",
      toolError: "#3c2828",
    },
    border: {
      normal: p.neutral4,
      muted: p.neutral3,
      accent: p.accent,
      error: p.red,
    },
    markdown: {
      heading: p.yellow,
      link: p.blue,
      linkUrl: p.neutral5,
      inlineCode: p.accent,
      codeBlock: p.neutral7,
      codeBlockBorder: p.neutral4,
      quote: p.neutral5,
      quoteBorder: p.neutral5,
      listBullet: p.accent,
      rule: p.neutral4,
    },
    diff: {
      added: p.green,
      removed: p.red,
      context: p.neutral5,
      hunk: p.blue,
    },
    tool: {
      title: p.neutral7,
      args: p.neutral5,
      path: p.accent,
      output: p.neutral5,
      duration: p.neutral4,
    },
    thinking: {
      text: p.neutral5,
      hiddenLabel: p.neutral4,
      off: p.neutral4,
      low: p.blue,
      medium: p.accent,
      high: p.purple,
    },
  };
}
