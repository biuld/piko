// ============================================================================
// PanelRenderer — surface → shell + body routing.
//
// Shells (PartialShell / FullShell) handle border + title + sizing.
// PanelBody handles all content rendering (owns filter, hints, descriptions).
// ============================================================================

import type { PikoHost } from "piko-host-runtime";
import { createMemo, untrack } from "solid-js";
import { PanelRuntime } from "../../../panels/panel-runtime.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { SurfaceState } from "../../../surfaces/types.js";
import type { ActionService } from "../action-service.js";
import type { TuiStore } from "../store.js";
import { FullShell } from "./FullShell.js";
import { PanelBody } from "./PanelBody.js";
import { PartialShell } from "./PartialShell.js";

export interface PanelRendererProps {
  surface: SurfaceState;
  store: TuiStore;
  controller: TuiController;
  actionSvc: ActionService;
  host: PikoHost;
  settingsManager?: any;
}

export function PanelRenderer(props: PanelRendererProps) {
  const runtime = createMemo(() => {
    return untrack(
      () =>
        new PanelRuntime(
          props.surface.panel,
          () => props.store.dispatch({ type: "surface_updated" } as any),
          () => props.controller.closeSurface(props.surface.id),
        ),
    );
  });

  const route = () => runtime().currentRoute;
  const chrome = () => route().chrome;
  const viewportHeight = () => props.store.state().layout.viewport.height;
  const viewportWidth = () => props.store.state().layout.viewport.width;
  const bottomBarRows = () => (props.store.state().layout.mode === "minimal" ? 1 : 2);

  // Content area available to body component (after shell decor).
  const shellHeight = () => chrome().height ?? 14;
  const contentHeight = () => {
    if (props.surface.placement === "full") {
      const headerRows = chrome().title || (chrome().hints?.length ?? 0) > 0 ? 2 : 0;
      const shellPadding = 3; // padding left+right+bottom in FullShell content area
      return Math.max(1, viewportHeight() - bottomBarRows() - headerRows - shellPadding);
    }
    const shellBorders = 2; // top + bottom border
    const hintsRow = chrome().hints?.length ? 1 : 0;
    return Math.max(1, shellHeight() - shellBorders - hintsRow);
  };

  const body = (
    <PanelBody
      surfaceId={props.surface.id}
      body={route().body}
      runtime={runtime()}
      store={props.store}
      controller={props.controller}
      actionSvc={props.actionSvc}
      host={props.host}
      settingsManager={props.settingsManager}
      availableHeight={contentHeight()}
      availableWidth={props.surface.placement === "full" ? viewportWidth() - 2 : viewportWidth()}
    />
  );

  return props.surface.placement === "full" ? (
    <FullShell title={chrome().title} hints={chrome().hints}>
      {body}
    </FullShell>
  ) : (
    <PartialShell height={shellHeight()} title={chrome().title} hints={chrome().hints}>
      {body}
    </PartialShell>
  );
}
