// ============================================================================
// SyntaxStyle factory — creates a SyntaxStyle instance for markdown rendering
// ============================================================================

import { SyntaxStyle } from "@opentui/core";

let _cachedStyle: SyntaxStyle | null = null;

/**
 * Create a SyntaxStyle with piko's dark palette colors for code highlighting.
 * Returns cached instance on repeated calls.
 */
export function getSyntaxStyle(): SyntaxStyle {
  if (_cachedStyle) return _cachedStyle;

  _cachedStyle = SyntaxStyle.fromStyles({
    source: { fg: "#d4d4d4" },
    comment: { fg: "#808098", italic: true },
    keyword: { fg: "#9575cd", bold: true },
    function: { fg: "#5f87ff" },
    variable: { fg: "#d4d4d4" },
    string: { fg: "#b5bd68" },
    number: { fg: "#f0c674" },
    type: { fg: "#00d7ff" },
    operator: { fg: "#d4d4d4" },
    punctuation: { fg: "#808098" },
    // Markdown-specific styles
    heading: { fg: "#f0c674", bold: true },
    link: { fg: "#5f87ff", underline: true },
    linkUrl: { fg: "#808098" },
    inlineCode: { fg: "#8abeb7" },
    codeBlock: { fg: "#d4d4d4" },
    codeBorder: { fg: "#505068" },
    quote: { fg: "#808098", italic: true },
    quoteBorder: { fg: "#505068" },
    listBullet: { fg: "#8abeb7" },
    rule: { fg: "#505068" },
  });

  return _cachedStyle;
}

/**
 * Dispose cached SyntaxStyle. Call on theme change.
 */
export function clearSyntaxStyleCache(): void {
  _cachedStyle = null;
}
