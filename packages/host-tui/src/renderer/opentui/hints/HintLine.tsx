// ============================================================================
// HintLine — packs hint strings to available width
// ============================================================================

import { useTheme } from "../theme-context.js";

export interface HintLineProps {
  hints: string[];
  width?: number;
}

/**
 * Pack hints into available width, dropping low-priority ones first.
 */
function packHints(hints: string[], maxWidth: number): string[] {
  if (hints.length === 0) return [];
  const sep = "  ";
  const result = [...hints];
  let totalLen = result.reduce((s, h) => s + h.length, 0) + (result.length - 1) * sep.length;

  while (totalLen > maxWidth && result.length > 1) {
    const removed = result.pop()!;
    totalLen -= removed.length + sep.length;
  }

  return result;
}

export function HintLine(props: HintLineProps) {
  const theme = useTheme();
  const { hints, width = 80 } = props;

  if (hints.length === 0) return null;

  const packed = packHints(hints, width);

  return (
    <box height={1}>
      <text fg={theme.color("text.dim")}>{packed.join("  ")}</text>
    </box>
  );
}
