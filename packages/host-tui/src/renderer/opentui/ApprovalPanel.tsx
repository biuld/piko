import type { TuiApprovalState } from "../../state/state.js";
import { PartialShell } from "./panels/PartialShell.js";
import { useTheme } from "./theme-context.js";

export interface ApprovalPanelProps {
  approval: TuiApprovalState;
}

function summarizeArgs(toolName: string, args: unknown): string {
  if (!args || typeof args !== "object") return "";
  const values = args as Record<string, unknown>;
  const preferred =
    toolName === "bash"
      ? values.command
      : toolName === "edit" || toolName === "write" || toolName === "read"
        ? values.path
        : undefined;
  let text: string;
  if (typeof preferred === "string") {
    text = preferred;
  } else {
    try {
      text = JSON.stringify(values);
    } catch {
      text = "[unserializable arguments]";
    }
  }
  const singleLine = text.replaceAll(/\s+/g, " ").trim();
  return singleLine.length > 220 ? `${singleLine.slice(0, 217)}...` : singleLine;
}

export function ApprovalPanel(props: ApprovalPanelProps) {
  const theme = useTheme();
  const pending = () => props.approval.pending;
  const waiting = () => props.approval.queue.length;
  const detail = () => {
    const request = pending();
    return request ? summarizeArgs(request.toolName, request.toolArgs) : "";
  };

  return (
    <PartialShell height={8} title="Tool Approval" hints={["Enter accept", "Esc decline"]}>
      <box flexDirection="column" paddingLeft={2} paddingRight={2} paddingTop={1} gap={1}>
        <box flexDirection="row" justifyContent="space-between">
          <text fg={theme.color("text.warning")}>Permission required</text>
          <text fg={theme.color("text.dim")}>
            {waiting() > 0 ? `${waiting()} more queued` : ""}
          </text>
        </box>
        <text fg={theme.color("text.primary")}>{pending()?.toolName ?? "unknown tool"}</text>
        <box height={1} overflow="hidden">
          <text fg={theme.color("text.muted")}>{detail()}</text>
        </box>
      </box>
    </PartialShell>
  );
}
