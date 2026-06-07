// ============================================================================
// FilterBar — search/query input display.
//
// Props in → render out. No state, no keyboard.
// The parent component manages query state and keyboard handling.
// ============================================================================

import { useTheme } from "../theme-context.js";

export interface FilterBarProps {
  query: string;
  placeholder?: string;
}

export function FilterBar(props: FilterBarProps) {
  const theme = useTheme();

  return (
    <box paddingLeft={1} height={1} flexDirection="row">
      <text fg={theme.color("text.warning")}>/ </text>
      {props.query ? (
        <text fg={theme.color("text.primary")}>{props.query}</text>
      ) : (
        <text fg={theme.color("text.dim")}>{props.placeholder ?? "Filter..."}</text>
      )}
    </box>
  );
}
