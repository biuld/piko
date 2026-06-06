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
  
  // Basic filter row implementation
  const filterRow = () => {
    const theme = useTheme();
    const caps = route().capabilities;
    const filterCap = caps.find(c => c.kind === "filter") as any;
    if (!filterCap) return null;

    const query = props.surface.panel.state.filterText || "";
    return (
      <box paddingX={1} paddingTop={1} height={2} flexDirection="row">
        <text fg={theme.color("text.warning")}>/ </text>
        {query ? (
          <text fg={theme.color("text.primary")}>{query}</text>
        ) : (
          <text fg={theme.color("text.dim")}>{filterCap.placeholder || "Filter..."}</text>
        )}
      </box>
    );
  };

  const body = () => (
    <PanelFrame chrome={chrome()} filterRow={filterRow()} placement={props.surface.placement}>
      <PanelBodyRegistry
        surfaceId={props.surface.id}
        body={route().body}
        runtime={runtime()}
        store={props.store}
        controller={props.controller}
        actionSvc={props.actionSvc}
        host={props.host}
        settingsManager={props.settingsManager}
      />
    </PanelFrame>
  );

  return props.surface.placement === "full" ? (
    <FullPanelHost title={chrome().title}>{body()}</FullPanelHost>
  ) : (
    <PartialPanelHost height={14} title={chrome().title}>{body()}</PartialPanelHost>
  );
}
