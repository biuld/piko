// ============================================================================
// ApprovalBar — shows a confirmation bar when a tool needs user approval.
// Appears at the top of the screen, above the timeline.
// ============================================================================

import { TextAttributes } from "@opentui/core";
import { Match, Switch } from "solid-js";
import type { TuiState } from "../../state/state.js";
import type { ActionService } from "./action-service.js";
import { useTheme } from "./theme-context.js";

export interface ApprovalBarProps {
  state: () => TuiState;
  actionSvc: ActionService;
}

export function ApprovalBar(props: ApprovalBarProps) {
  const theme = useTheme();
  const pending = () => props.state().approval?.pending;

  return (
    <Switch>
      <Match when={pending() != null}>
        <box flexShrink={0} flexDirection="column">
          <box height={1} />
          <box
            backgroundColor={theme.color("surface.toolPending")}
            paddingLeft={2}
            paddingRight={2}
            paddingTop={1}
            paddingBottom={1}
            flexDirection="row"
            justifyContent="space-between"
          >
            <box>
              <text fg={theme.color("text.warning")} attributes={TextAttributes.BOLD}>
                Approve tool:{" "}
              </text>
              <text fg={theme.color("text.primary")}>{pending()?.toolName ?? "unknown"}</text>
            </box>
            <box gap={1}>
              {/* Accept — Enter or click */}
              <box backgroundColor={theme.color("surface.accept")} paddingLeft={1} paddingRight={1}>
                <text fg={theme.color("text.success")} attributes={TextAttributes.BOLD}>
                  [Enter] Accept
                </text>
              </box>
              {/* Decline — Escape or click */}
              <box
                backgroundColor={theme.color("surface.decline")}
                paddingLeft={1}
                paddingRight={1}
              >
                <text fg={theme.color("text.error")} attributes={TextAttributes.BOLD}>
                  [Esc] Decline
                </text>
              </box>
            </box>
          </box>
        </box>
      </Match>
    </Switch>
  );
}
