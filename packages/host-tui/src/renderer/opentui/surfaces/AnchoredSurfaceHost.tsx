// ============================================================================
// AnchoredSurfaceHost — renders a surface anchored to editor/target
// ============================================================================

import type { JSX } from "solid-js";
import type { TuiSurfaceState } from "../../../surfaces/types.js";

export interface AnchoredSurfaceHostProps {
  surface: TuiSurfaceState;
  children: JSX.Element;
}

export function AnchoredSurfaceHost(props: AnchoredSurfaceHostProps) {
  // Anchored surfaces render as inline overlays near their anchor
  return (
    <box flexDirection="column" flexShrink={0}>
      {props.children}
    </box>
  );
}
