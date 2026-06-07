import { createMemo, untrack } from "solid-js";
import type { PikoHost } from "piko-host-runtime";
import type { SurfaceState } from "../../../surfaces/types.js";
import { PanelRuntime } from "../../../panels/panel-runtime.js";
import type { TuiStore } from "../store.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { ActionService } from "../action-service.js";
import { PartialPanelHost } from "./PartialPanelHost.js";
import { FullPanelHost } from "./FullPanelHost.js";
import { PanelFrame } from "./PanelFrame.js";
import { PanelBodyRegistry } from "./PanelBodyRegistry.js";
import type { KeyEvent } from "../../../focus/types.js";
import { formBehavior } from "../../../surfaces/index.js";
import { useTheme } from "../theme-context.js";

export interface PanelRendererProps {
  surface: SurfaceState;
  store: TuiStore;
  controller: TuiController;
  actionSvc: ActionService;
  host: PikoHost;
  settingsManager?: any;
}

export function PanelRenderer(props: PanelRendererProps) {
  // Use untrack to avoid re-creating the runtime when state changes.
  const runtime = createMemo(() => {
    return untrack(() => new PanelRuntime(
      props.surface.panel,
      () => {
        // trigger re-render
        props.store.dispatch({ type: "surface_updated" } as any);
      },
      () => {
        props.controller.closeSurface(props.surface.id);
      }
    ));
  });

  const route = () => runtime().currentRoute;
  const chrome = () => route().chrome;
  const viewportHeight = () => props.store.state().layout.viewport.height;
  const bottomBarRows = () => props.store.state().layout.mode === "minimal" ? 1 : 2;
  const bodyAvailableHeight = () => {
    const c = chrome();
    if (props.surface.placement === "full") {
      const headerRows = c.title || (c.hints?.length ?? 0) > 0 ? 2 : 0;
      const filterRows = route().capabilities.some((c) => c.kind === "filter") ? 3 : 1;
      const panelBordersAndPadding = 4;
      return Math.max(1, viewportHeight() - bottomBarRows() - headerRows - filterRows - panelBordersAndPadding);
    }

    const partialRows = 14;
    const frameHints = (c.hints?.length ?? 0) > 0 ? 1 : 0;
    const frameFilter = route().capabilities.some((c) => c.kind === "filter") ? 1 : 0;
    const panelBorders = 2;
    return Math.max(1, partialRows - panelBorders - frameHints - frameFilter);
  };
  
  // Basic filter row implementation
  const filterRow = () => {
    const theme = useTheme();
    const caps = route().capabilities;
    const filterCap = caps.find(c => c.kind === "filter") as any;
    if (!filterCap) return null;

    const query = props.surface.panel.state.filterText || "";
    return (
      <box paddingLeft={1} height={1} flexDirection="row">
        <text fg={theme.color("text.warning")}>/ </text>
        {query ? (
          <text fg={theme.color("text.primary")}>{query}</text>
        ) : (
          <text fg={theme.color("text.dim")}>{filterCap.placeholder || "Filter..."}</text>
        )}
      </box>
    );
  };

  const body = (hideHints?: boolean) => (
    <PanelFrame chrome={hideHints ? { ...chrome(), hints: undefined } : chrome()} filterRow={hideHints ? null : filterRow()} placement={props.surface.placement}>
      <PanelBodyRegistry
        surfaceId={props.surface.id}
        body={route().body}
        runtime={runtime()}
        store={props.store}
        controller={props.controller}
        actionSvc={props.actionSvc}
        host={props.host}
        settingsManager={props.settingsManager}
        availableHeight={bodyAvailableHeight()}
      />
    </PanelFrame>
  );

  return props.surface.placement === "full" ? (
    <FullPanelHost title={chrome().title} hints={chrome().hints} filterRow={filterRow()}>
      {body(true)}
    </FullPanelHost>
  ) : (
    <PartialPanelHost height={14} title={chrome().title}>{body()}</PartialPanelHost>
  );
}
