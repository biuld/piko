// ============================================================================
// ToolBlock — renders a single tool call with icon, summary, expandable details
// ============================================================================

import { createSignal } from "solid-js";
import type { ToolBlockViewModel } from "../../../state/state.js";
import { formatToolDisplay, type ToolStatus } from "./display.js";
import { useTheme } from "../theme-context.js";

export interface ToolBlockProps {
  block: ToolBlockViewModel;
}

export function ToolBlock(props: ToolBlockProps) {
  const theme = useTheme();
  const { block } = props;
  const [expanded, setExpanded] = createSignal(false);

  const display = formatToolDisplay({
    name: block.name,
    args: (block.args as Record<string, unknown>) ?? {},
    result: block.result,
    status: block.status as ToolStatus,
    isExpanded: expanded(),
  });

  const statusColor = String(theme.color(display.statusColor));

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
      {/* Collapsed summary line — always visible */}
      <box flexDirection="row" height={1}>
        <text fg={statusColor}>{display.icon} </text>
        <text fg={theme.color("tool.title")}>{display.summary}</text>
        {display.details ? (
          <text fg={theme.color("text.dim")}>
            {" "}{expanded() ? "▲" : "▶"}
          </text>
        ) : null}
      </box>

      {/* Expanded details */}
      {expanded() && display.details ? (
        <box paddingLeft={4} paddingTop={1} flexDirection="column">
          <text fg={theme.color("tool.output")}>{display.details}</text>
        </box>
      ) : null}
    </box>
  );
}
