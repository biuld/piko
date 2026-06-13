// ============================================================================
// SummaryTimelineItem — branch/compaction summary rendering, pi-aligned
//
// Pi pattern:
//   - Single Box with customMessageBg background, paddingX=1 inside bg
//   - Bold label: [compaction] or [branch]
//   - Collapsed: "Compacted from N tokens (key to expand)"
//   - Expanded: Markdown-formatted summary
// ============================================================================

import { TextAttributes } from "@opentui/core";
import type { TimelineItem } from "../../../timeline/types.js";
import { useTheme } from "../theme-context.js";
import { MarkdownContent } from "./MarkdownContent.js";

export interface SummaryTimelineItemProps {
  item: TimelineItem;
  isExpanded: boolean;
}

export function SummaryTimelineItem(props: SummaryTimelineItemProps) {
  const theme = useTheme();
  const { item, isExpanded } = props;

  const isCompaction = item.kind === "compaction-summary";
  const label = isCompaction ? "compaction" : "branch";
  const summary = item.text ?? "";
  const tokensBefore = item.tokensBefore;

  return (
    <box flexDirection="column">
      <box height={1} />
      <box
        backgroundColor={theme.color("surface.customMessage")}
        paddingLeft={1}
        paddingRight={1}
        paddingTop={1}
        paddingBottom={1}
        flexDirection="column"
      >
        {/* Label — bold, pi uses customMessageLabel = #9575cd */}
        <text fg={theme.color("text.customLabel")} attributes={TextAttributes.BOLD}>
          [{label}]
        </text>

        {/* Content — collapsed or expanded */}
        {isExpanded && summary ? (
          <box paddingTop={1} flexDirection="column">
            <MarkdownContent
              content={summary}
              fg={theme.color("text.primary")}
              bg={theme.color("surface.customMessage")}
              conceal={true}
            />
          </box>
        ) : (
          <text fg={theme.color("text.muted")}>
            {isCompaction && tokensBefore
              ? `Compacted from ${tokensBefore.toLocaleString()} tokens (Ctrl+O to expand)`
              : `${label === "branch" ? "Branch summary" : "Summary"} (Ctrl+O to expand)`}
          </text>
        )}
      </box>
    </box>
  );
}
