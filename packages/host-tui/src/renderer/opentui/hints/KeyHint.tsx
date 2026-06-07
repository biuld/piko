// ============================================================================
// KeyHint — renders a single key hint
// ============================================================================

import { useTheme } from "../theme-context.js";

export interface KeyHintProps {
  keyLabel: string;
  description: string;
}

export function KeyHint(props: KeyHintProps) {
  const theme = useTheme();

  return (
    <text fg={theme.color("text.dim")}>
      {props.keyLabel} {props.description}
    </text>
  );
}
