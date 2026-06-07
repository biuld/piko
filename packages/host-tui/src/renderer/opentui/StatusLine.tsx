// ============================================================================
// Status — dedicated system-state component between timeline and editor.
//
// Renders:
//   idle           → short rule + rotating placeholder
//   idle + queue   → short rule + queued messages + dequeue hint
//   working        → short rule + spinner + "Working..."
//   compacting     → short rule + spinner + "Compacting..."
//
// The editor/editor panel owns its own border; status stays compact and uses
// muted text plus severity accents instead of panel-like backgrounds.
// ============================================================================

import { useTheme } from "./theme-context.js";
import { createSignal, onCleanup, onMount, Show, For } from "solid-js";
import { Spinner } from "./status/Spinner.js";
import type { StatusContract } from "./status/types.js";
import { middleTruncate, visibleLength } from "../../layout/bottom-bar-packer.js";

export interface StatusLineProps {
  status: StatusContract;
  sessionTitle: string;
  width: number;
}

const PLACEHOLDERS = ["Ready", "Standing by", "Idle"];
const PLACEHOLDER_INTERVAL_MS = 8_000;
const SESSION_TITLE_MAX_WIDTH = 32;

export function StatusLine(props: StatusLineProps) {
  const theme = useTheme();
  const height = () => computeHeight(props.status);
  const [placeholderIndex, setPlaceholderIndex] = createSignal(0);

  onMount(() => {
    const timer = setInterval(() => {
      setPlaceholderIndex((idx) => (idx + 1) % PLACEHOLDERS.length);
    }, PLACEHOLDER_INTERVAL_MS);
    onCleanup(() => clearInterval(timer));
  });

  const placeholder = () => PLACEHOLDERS[placeholderIndex()];
  const rule = () => buildSessionRule(props.sessionTitle, props.width);

  return (
    <box flexDirection="column" flexShrink={0} height={height()} overflow="hidden">
      <box height={1}>
        <text fg={theme.color("border.muted")}>{rule()}</text>
      </box>

      {/* Notification display (idle + latest unexpired notification) */}
      <Show when={props.status.state === "idle" && props.status.notification}>
        {(notification) => (
          <box height={1} paddingLeft={1} paddingRight={1}>
            <text fg={theme.color(notificationColorToken(notification().severity))}>
              {notificationLabel(notification().severity)}{" "}
            </text>
            <text fg={theme.color("text.muted")}>{notification().message}</text>
          </box>
        )}
      </Show>

      {/* Queue display (idle + queued messages) */}
      <Show when={props.status.state === "idle" && !props.status.notification && hasQueue(props.status)}>
        <For each={props.status.queue!.steering}>
          {(msg) => (
            <box height={1} paddingLeft={1} paddingRight={1}>
              <text fg={theme.color("text.muted")}>
                Steering: {msg.preview}
              </text>
            </box>
          )}
        </For>
        <For each={props.status.queue!.followUp}>
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
      <Show when={props.status.state === "working" || props.status.state === "compacting"}>
        <box height={1} paddingLeft={1} paddingRight={1}>
          <Spinner />
          <text fg={theme.color("text.muted")}>
            {props.status.state === "compacting" ? " Compacting..." : ` ${props.status.label ?? "Working..."}`}
          </text>
        </box>
      </Show>

      <Show when={props.status.state === "idle" && !props.status.notification && !hasQueue(props.status)}>
        <box height={1} paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.dim")}>{placeholder()}</text>
        </box>
      </Show>
    </box>
  );
}

function hasQueue(status: StatusContract): boolean {
  const q = status.queue;
  if (!q) return false;
  return q.steering.length > 0 || q.followUp.length > 0 || q.nextTurnCount > 0;
}

function computeHeight(status: StatusContract): number {
  if (status.state === "working" || status.state === "compacting") {
    return 2;
  }
  if (status.state === "idle" && status.notification) {
    return 2;
  }
  if (status.state === "idle" && hasQueue(status)) {
    const q = status.queue!;
    const rows = q.steering.length + q.followUp.length + 1; // messages + hint
    return rows + 1; // + short rule row
  }
  return 2;
}

function buildSessionRule(title: string, width: number): string {
  const safeWidth = Math.max(0, width);
  if (safeWidth <= 0) return "";
  if (safeWidth < 8) return "─".repeat(safeWidth);

  const maxTitleWidth = Math.min(SESSION_TITLE_MAX_WIDTH, Math.max(0, safeWidth - 4));
  const safeTitle = middleTruncate(title.trim() || "session", maxTitleWidth);
  const prefix = `─ ${safeTitle} `;
  const remaining = Math.max(0, safeWidth - visibleLength(prefix));
  return `${prefix}${"─".repeat(remaining)}`.slice(0, safeWidth);
}

type NotificationSeverity = NonNullable<StatusContract["notification"]>["severity"];

function notificationColorToken(severity: NotificationSeverity): string {
  switch (severity) {
    case "success":
      return "text.success";
    case "warning":
      return "text.warning";
    case "error":
      return "text.error";
    case "info":
      return "text.accent";
  }
}

function notificationLabel(severity: NotificationSeverity): string {
  switch (severity) {
    case "success":
      return "success";
    case "warning":
      return "warning";
    case "error":
      return "error";
    case "info":
      return "info";
  }
}
