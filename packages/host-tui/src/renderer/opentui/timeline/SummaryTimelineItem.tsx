// ============================================================================
// SummaryTimelineItem — branch/compaction summary rendering
// ============================================================================

import type { TimelineItem } from "../../../timeline/types.js";
import { useTheme } from "../theme-context.js";

export interface SummaryTimelineItemProps {
  item: TimelineItem;
}

export function SummaryTimelineItem(props: SummaryTimelineItemProps) {
  const theme = useTheme();
  const { item } = props;

  const label = item.kind === "branch-summary" ? "Branch" : "Compaction";

  return (
    <box paddingLeft={1} paddingRight={1}>
      <text fg={theme.color("thinking.text")}>
        {label}: {item.text}
      </text>
    </box>
  );
}
