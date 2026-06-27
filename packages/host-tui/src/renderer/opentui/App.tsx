// ============================================================================
// OpenTUI App Shell — composition only: providers, layout, keyboard bridge.
// Render plan computed by surface subsystem; slot/surface rendering delegated.
// ============================================================================

import { useKeyboard, useTerminalDimensions } from "@opentui/solid";
import { For } from "solid-js";
import type { TuiHostFacade } from "../../app/tui-host.js";
import type { RunTuiOptions } from "../../app/types.js";
import type { TuiController } from "../../runtime/tui-controller.js";
import { selectStatus } from "../../state/selectors.js";
import { computeRenderPlan } from "../../surfaces/render-plan.js";
import type { ActionService } from "./action-service.js";
import {
  useKeyboardBridge,
  useLayoutPolicies,
  useOrchestratorSnapshot,
  usePiTheme,
  useRenderTrace,
  useSpinnerFrame,
  useStatusClock,
  useViewportSync,
} from "./app-hooks.js";
import { createAppRuntimeServices } from "./app-runtime.js";
import { LayoutProvider } from "./layout-context.js";
import { PanelRenderer } from "./panels/PanelRenderer.js";
import { renderSlot } from "./SlotRenderer.js";
import type { TuiStore } from "./store.js";
import { ThemeProvider } from "./theme-context.js";

// ============================================================================
// Props
// ============================================================================

export interface AppProps {
  store: TuiStore;
  host: TuiHostFacade;
  options?: RunTuiOptions;
  shutdown: () => void;
  controller?: TuiController;
  actionSvc?: ActionService;
  /** Approval bridge created before Host, used to wire pending approvals into ActionService. */
  approvalBridge?: {
    onPending(
      listener: (pending: import("../../approval-bridge.js").PendingApproval) => void,
    ): void;
  };
}

// ============================================================================
// App component
// ============================================================================

export function App(props: AppProps) {
  const { store, host } = props;
  const dims = useTerminalDimensions();

  const { actionSvc, ctrl } = createAppRuntimeServices({
    store,
    host,
    options: props.options,
    shutdown: props.shutdown,
    controller: props.controller,
    actionSvc: props.actionSvc,
    approvalBridge: props.approvalBridge,
  });

  const state = store.state;
  const layout = () => state().layout;
  const statusClock = useStatusClock(state);
  const spinnerFrame = useSpinnerFrame();
  const orchestratorSnapshot = useOrchestratorSnapshot(host);
  const currentTheme = usePiTheme(() => state().layout.theme);
  const statusContract = () => selectStatus(state(), statusClock());
  const isRunning = () => state().stream.status === "running";
  const timelineItems = () => state().timeline.items;
  const plan = () => computeRenderPlan(state());

  useViewportSync(store, dims);
  useKeyboardBridge(useKeyboard, ctrl);
  useLayoutPolicies(store);
  useRenderTrace(state);

  return (
    <LayoutProvider value={state().layout}>
      <ThemeProvider value={currentTheme()}>
        <box flexDirection="column" width="100%" height="100%">
          <For each={plan().inline}>
            {(entry) => {
              if (entry.kind === "slot") {
                return renderSlot(entry.id, {
                  timelineItems,
                  layout,
                  state,
                  statusContract,
                  orchestratorSnapshot,
                  spinnerFrame,
                  isRunning,
                  store,
                  actionSvc: actionSvc(),
                  ctrl: ctrl(),
                  host: props.host,
                });
              }
              if (entry.kind === "surface") {
                return (
                  <PanelRenderer
                    surface={entry.surface! as any}
                    store={store}
                    controller={ctrl()}
                    actionSvc={actionSvc()}
                    host={host}
                    settingsManager={props.options?.settingsManager}
                  />
                );
              }
              return null;
            }}
          </For>
        </box>
      </ThemeProvider>
    </LayoutProvider>
  );
}
