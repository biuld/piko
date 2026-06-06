// ============================================================================
// Status — dedicated system-state component between timeline and editor.
//
// Renders:
//   idle           → hidden (height 0)
//   idle + queue   → 1+ rows showing queued messages + dequeue hint
//   working        → 1 row: spinner + "Working..."
//   compacting     → 1 row: spinner + "Compacting..." (reserved, not yet wired)
//
// A bottom border separates the status area from the editor below.
// ============================================================================

import { useTheme } from "./theme-context.js";
import { Show, For } from "solid-js";
import { Spinner } from "./status/Spinner.js";
import type { StatusContract } from "./status/types.js";

export interface StatusLineProps {
  status: StatusContract;
}

export function StatusLine(props: StatusLineProps) {
  const theme = useTheme();
  const { status } = props;
  const visible = () => status.state !== "idle" || hasQueue(status);
  const height = () => computeHeight(status);
  const showSeparator = () => visible();

  return (
    <Show when={visible()}>
      <box flexDirection="column" flexShrink={0}>
        {/* Queue display (idle + queued messages) */}
        <Show when={status.state === "idle" && hasQueue(status)}>
          <For each={status.queue!.steering}>
            {(msg) => (
              <box height={1} paddingLeft={1} paddingRight={1}>
                <text fg={theme.color("text.muted")}>
                  Steering: {msg.preview}
                </text>
              </box>
            )}
          </For>
          <For each={status.queue!.followUp}>
            {(msg) => (
              <box height={1} paddingLeft={1} paddingRight={1}>
                <text fg={theme.color("text.muted")}>
                  Follow-up: {msg.preview}
                </text>
              </box>
            )}
          </For>
          <box height={1} paddingLeft={1} paddingRight={1}>
            <text fg={theme.color("text.dim")}>
              ↳ Alt+↑ to edit all queued messages
            </text>
          </box>
        </Show>

        {/* Working / compacting spinner */}
        <Show when={status.state === "working" || status.state === "compacting"}>
          <box height={1} paddingLeft={1} paddingRight={1}>
            <Spinner />
            <text fg={theme.color("text.muted")}>
              {status.state === "compacting" ? " Compacting..." : ` ${status.label ?? "Working..."}`}
            </text>
          </box>
        </Show>

        {/* Separator line between status and editor */}
        <Show when={showSeparator()}>
          <box height={1} border={["bottom"]} borderColor={theme.color("border.muted")} />
        </Show>
      </box>
    </Show>
  );
}

function hasQueue(status: StatusContract): boolean {
  const q = status.queue;
  if (!q) return false;
  return q.steering.length > 0 || q.followUp.length > 0 || q.nextTurnCount > 0;
}

function computeHeight(status: StatusContract): number {
  if (status.state === "working" || status.state === "compacting") {
    return 2; // content row + separator row
  }
  if (status.state === "idle" && hasQueue(status)) {
    const q = status.queue!;
    const rows = q.steering.length + q.followUp.length + 1; // messages + hint
    return rows + 1; // + separator row
  }
  return 0;
}
