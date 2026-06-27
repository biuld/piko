import { type Accessor, createMemo, onCleanup, untrack } from "solid-js";
import type { TuiHostFacade } from "../../app/tui-host.js";
import type { RunTuiOptions } from "../../app/types.js";
import type { PendingApproval } from "../../approval-bridge.js";
import { ApprovalStore } from "../../approval-store.js";
import { HostdClient } from "../../client/index.js";
import { TuiController } from "../../runtime/tui-controller.js";
import { ActionService } from "./action-service.js";
import type { TuiStore } from "./store.js";

export interface AppRuntimeServicesProps {
  store: TuiStore;
  host: TuiHostFacade;
  options?: RunTuiOptions;
  shutdown: () => void;
  controller?: TuiController;
  actionSvc?: ActionService;
  approvalBridge?: {
    onPending(listener: (pending: PendingApproval) => void): void;
  };
}

export interface AppRuntimeServices {
  actionSvc: Accessor<ActionService>;
  ctrl: Accessor<TuiController>;
}

export function createAppRuntimeServices(props: AppRuntimeServicesProps): AppRuntimeServices {
  let hostdClient: HostdClient | undefined;

  const svc = createMemo(
    () => {
      if (props.actionSvc) return props.actionSvc;
      if (!props.options) {
        throw new Error("RunTuiOptions are required when App creates its own ActionService");
      }

      const service = new ActionService(
        props.host,
        props.store,
        props.options.preferences,
        props.options.modelCatalog,
        props.shutdown,
      );

      if (props.approvalBridge) {
        service.setApprovalBridge(props.approvalBridge);
      }

      if (props.options.hostd?.enabled && !hostdClient) {
        // Reuse the client from the facade if available
        const facadeClient = (props.host as any)?._client as HostdClient | undefined;
        hostdClient =
          facadeClient ??
          new HostdClient({
            command: props.options.hostd.command,
            args: props.options.hostd.args,
          });
        service.setHostdClient(hostdClient);
      }

      service.approvalStore = new ApprovalStore(props.host.cwd);
      return service;
    },
    { equals: false },
  );

  const controller = createMemo(
    () => {
      if (props.controller) return props.controller;
      return untrack(() => {
        const ctrl = new TuiController(props.host, props.store, props.shutdown);
        ctrl.setActionService(svc());
        return ctrl;
      });
    },
    { equals: false },
  );

  onCleanup(() => {
    void hostdClient?.close();
  });

  return {
    actionSvc: () => svc(),
    ctrl: () => controller(),
  };
}
