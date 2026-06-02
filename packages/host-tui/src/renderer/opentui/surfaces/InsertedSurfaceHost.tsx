// ============================================================================
// InsertedSurfaceHost — renders a surface inserted between layout slots
// ============================================================================

import type { JSX } from "solid-js";
import type { TuiSurfaceState } from "../../../surfaces/types.js";

export interface InsertedSurfaceHostProps {
  surface: TuiSurfaceState;
  children: JSX.Element;
}

export function InsertedSurfaceHost(props: InsertedSurfaceHostProps) {
  // Inserted surfaces stay in text flow between slots
  return (
    <box flexDirection="column" flexShrink={0}>
      {props.children}
    </box>
  );
}
