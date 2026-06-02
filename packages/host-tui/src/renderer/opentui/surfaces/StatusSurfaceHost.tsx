// ============================================================================
// StatusSurfaceHost — renders a status-line surface
// ============================================================================

import type { JSX } from "solid-js";
import type { TuiSurfaceState } from "../../../surfaces/types.js";

export interface StatusSurfaceHostProps {
  surface: TuiSurfaceState;
  children: JSX.Element;
}

export function StatusSurfaceHost(props: StatusSurfaceHostProps) {
  return (
    <box flexShrink={0} height={1}>
      {props.children}
    </box>
  );
}
