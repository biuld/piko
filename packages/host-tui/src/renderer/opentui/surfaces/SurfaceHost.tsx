// ============================================================================
// SurfaceHost — renders a surface based on its mount strategy
// ============================================================================

import type { JSX } from "solid-js";
import type { TuiSurfaceState } from "../../../surfaces/types.js";
import { AnchoredSurfaceHost } from "./AnchoredSurfaceHost.js";
import { InsertedSurfaceHost } from "./InsertedSurfaceHost.js";
import { DrawerSurfaceHost } from "./DrawerSurfaceHost.js";
import { StatusSurfaceHost } from "./StatusSurfaceHost.js";

export interface SurfaceHostProps {
  surface: TuiSurfaceState;
  children: JSX.Element;
}

export function SurfaceHost(props: SurfaceHostProps) {
  const { surface, children } = props;

  switch (surface.mount) {
    case "anchored":
      return <AnchoredSurfaceHost surface={surface}>{children}</AnchoredSurfaceHost>;

    case "insert-between":
      return <InsertedSurfaceHost surface={surface}>{children}</InsertedSurfaceHost>;

    case "side-drawer":
      return <DrawerSurfaceHost surface={surface}>{children}</DrawerSurfaceHost>;

    case "status-line":
      return <StatusSurfaceHost surface={surface}>{children}</StatusSurfaceHost>;

    case "replace-slot":
      // Render in place — no special wrapper
      return <>{children}</>;

    default:
      return <>{children}</>;
  }
}
