// ============================================================================
// LatestIndicator — shows pending new items count when user is scrolled away
// ============================================================================

import { useTheme } from "../theme-context.js";

export interface LatestIndicatorProps {
  count: number;
}

export function LatestIndicator(props: LatestIndicatorProps) {
  const theme = useTheme();
  const { count } = props;

  if (count <= 0) return null;

  return (
    <box height={1} paddingLeft={1} paddingRight={1}>
      <text fg={theme.color("text.warning")}>
        ▼ {count} new {count === 1 ? "item" : "items"} below
      </text>
    </box>
  );
}
