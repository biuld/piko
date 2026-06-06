// ============================================================================
// MarkdownContent — shared markdown rendering for timeline items
//
// Props:
//   content      - markdown text to render
//   fg           - foreground color for base text (default theme text.primary)
//   bg           - background color (default transparent)
//   streaming    - whether content is still streaming (for incremental updates)
//   conceal      - whether to conceal markdown syntax markers (default true)
// ============================================================================

import { createMemo } from "solid-js";
import { useTheme } from "../theme-context.js";
import { getSyntaxStyle } from "./syntax-style.js";

export interface MarkdownContentProps {
  content: string;
  fg?: string;
  bg?: string;
  streaming?: boolean;
  conceal?: boolean;
}

export function MarkdownContent(props: MarkdownContentProps) {
  const theme = useTheme();
  const syntaxStyle = getSyntaxStyle();

  const resolvedFg = createMemo(() => props.fg ?? theme.color("text.primary"));
  const conceal = props.conceal ?? true;
  const streaming = props.streaming ?? false;

  // Only pass bg when explicitly set — default is terminal background (transparent).
  // Passing surface.base by default would paint a visible block behind text.
  if (props.bg !== undefined) {
    return (
      <markdown
        content={props.content}
        syntaxStyle={syntaxStyle}
        fg={resolvedFg()}
        bg={props.bg}
        conceal={conceal}
        streaming={streaming}
        internalBlockMode="top-level"
      />
    );
  }

  return (
    <markdown
      content={props.content}
      syntaxStyle={syntaxStyle}
      fg={resolvedFg()}
      conceal={conceal}
      streaming={streaming}
      internalBlockMode="top-level"
    />
  );
}
