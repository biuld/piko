// ============================================================================
// HintBar — keybinding hint line.
//
// Single dim text line shown at the bottom of a panel body.
// ============================================================================

import { useTheme } from "../theme-context.js";

export interface HintBarProps {
  hints: string;
}

export function HintBar(props: HintBarProps) {
  const theme = useTheme();

  return (
    <box paddingLeft={1} paddingTop={1}>
      <text fg={theme.color("text.dim")}>{`  ${props.hints}`}</text>
    </box>
  );
}
