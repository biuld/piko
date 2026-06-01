// ============================================================================
// Overlay Container — reusable modal wrapper, theme-aware
// ============================================================================

import type { JSX } from "solid-js";
import type { TuiOverlayKind } from "../../../state/state.js";
import { useTheme } from "../theme-context.js";

export interface OverlayContainerProps {
  kind: TuiOverlayKind;
  title: string;
  children: JSX.Element;
  onClose: () => void;
}

export function OverlayContainer(props: OverlayContainerProps) {
  const theme = useTheme();

  return (
    <box
      border
      borderColor={theme.color("border.accent")}
      flexDirection="column"
      width="70%"
      padding={1}
    >
      {/* Title bar */}
      <box flexDirection="row" justifyContent="space-between" height={1}>
        <text fg={theme.color("text.accent")}>
          <strong>{props.title}</strong>
        </text>
        <text fg={theme.color("text.dim")}>Esc to close</text>
      </box>

      {/* Content */}
      <box flexDirection="column" paddingTop={1} paddingBottom={1}>
        {props.children}
      </box>

      {/* Footer hints */}
      <box height={1}>
        <text fg={theme.color("text.dim")}>↑↓ navigate  Enter select  Esc cancel</text>
      </box>
    </box>
  );
}
