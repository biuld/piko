// ============================================================================
// DescriptionBox — selected-item description text.
//
// Multi-line auto-wrapped dim text for the currently selected item.
// ============================================================================

import { createMemo } from "solid-js";
import { useTheme } from "../theme-context.js";

export interface DescriptionBoxProps {
  text: string;
  width: number;
}

function wordWrap(text: string, maxWidth: number): string[] {
  const lines: string[] = [];
  let remaining = text;
  while (remaining.length > 0) {
    if (remaining.length <= maxWidth) {
      lines.push(remaining);
      break;
    }
    let cut = maxWidth;
    while (cut > 0 && remaining[cut] !== " ") cut--;
    if (cut === 0) cut = maxWidth;
    lines.push(remaining.slice(0, cut));
    remaining = remaining.slice(cut).trimStart();
  }
  return lines;
}

export function DescriptionBox(props: DescriptionBoxProps) {
  const theme = useTheme();

  const lines = createMemo(() => {
    const maxW = Math.max(20, props.width - 4);
    return wordWrap(props.text, maxW);
  });

  return (
    <box flexDirection="column" paddingLeft={1} paddingTop={1}>
      {lines().map((line) => (
        <text fg={theme.color("text.dim")}>{`  ${line}`}</text>
      ))}
    </box>
  );
}
