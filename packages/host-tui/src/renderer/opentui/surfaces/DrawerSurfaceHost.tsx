// ============================================================================
// DrawerSurfaceHost — renders a side-drawer surface
// ============================================================================

import type { JSX } from "solid-js";
import type { TuiSurfaceState } from "../../../surfaces/types.js";

export interface DrawerSurfaceHostProps {
  surface: TuiSurfaceState;
  children: JSX.Element;
}

export function DrawerSurfaceHost(props: DrawerSurfaceHostProps) {
  // Drawer surfaces render alongside the main content
  return (
    <box flexDirection="row" flexGrow={1}>
      {/* Drawer content */}
      <box
        flexDirection="column"
        border={["left"]}
        borderColor="#444444"
        paddingLeft={1}
        width="40%"
      >
        {props.children}
      </box>
    </box>
  );
}
