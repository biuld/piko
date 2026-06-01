// ============================================================================
// Overlay Container — reusable modal wrapper for all overlays
// ============================================================================

import type { JSX } from "solid-js";
import type { TuiOverlayKind } from "../../../state/state.js";

export interface OverlayContainerProps {
  kind: TuiOverlayKind;
  title: string;
  children: JSX.Element;
  onClose: () => void;
}

/**
 * Wraps overlay content in a bordered box rendered via Portal.
 */
export function OverlayContainer(props: OverlayContainerProps) {
  return (
    <box
      border
      borderColor="#5f87ff"
      flexDirection="column"
      width="70%"
      padding={1}
    >
      {/* Title bar */}
      <box flexDirection="row" justifyContent="space-between" height={1}>
        <text fg="#8abeb7">
          <strong>{props.title}</strong>
        </text>
        <text fg="#808080">Esc to close</text>
      </box>

      {/* Separator */}
      <box height={1}>
        <text fg="#505050">──────────────────────────────</text>
      </box>

      {/* Content */}
      <box flexDirection="column" paddingTop={1} paddingBottom={1}>
        {props.children}
      </box>

      {/* Footer hints */}
      <box height={1}>
        <text fg="#505050">───────────────</text>
      </box>
      <box height={1}>
        <text fg="#666666">↑↓ navigate  Enter select  Esc cancel</text>
      </box>
    </box>
  );
}
