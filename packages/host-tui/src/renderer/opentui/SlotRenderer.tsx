// ============================================================================
// SlotRenderer — renders base slots (timeline, status, editor, bottom-bar)
// extracted from App.tsx to keep the shell lean.
// ============================================================================

import type { PikoHost } from "piko-host-runtime";
import type { OrchState } from "piko-orchestrator-protocol";
import type { TuiController } from "../../runtime/tui-controller.js";
import type { ActionService } from "./action-service.js";
import { BottomBar } from "./BottomBar.js";
import { Editor } from "./Editor.js";
import { StatusPanel } from "./status/StatusPanel.js";
import type { TuiStore } from "./store.js";
import { TimelineView } from "./timeline/TimelineView.js";

export interface SlotContext {
  timelineItems: () => any[];
  layout: () => any;
  state: () => any;
  statusContract: () => any;
  orchestratorSnapshot: () => OrchState | undefined;
  spinnerFrame: () => number;
  isRunning: () => boolean;
  store: TuiStore;
  actionSvc: ActionService;
  ctrl: TuiController;
  host: PikoHost;
}

export function renderSlot(slotId: string, ctx: SlotContext) {
  const s = ctx.state;

  switch (slotId) {
    case "timeline":
      return (
        <box flexGrow={1} flexShrink={1} overflow="hidden">
          <TimelineView
            projection={s().projection}
            layout={{
              width: ctx.layout().viewport.width,
              height: ctx.layout().viewport.height,
              mode: ctx.layout().mode,
            }}
            pendingNewItems={s().timeline.pendingNewItems}
            stickyBottom={s().timeline.anchor === "bottom"}
            streamRunning={s().stream.status === "running"}
            scrollCommand={s().scrollCommand ?? null}
            onScrollStateChange={(atBottom) => {
              ctx.store.dispatch({
                type: "chat_scrolled",
                anchor: atBottom ? "bottom" : "manual",
              });
            }}
            onScrollCommandDone={() => {
              ctx.store.setState((st) => ({ ...st, scrollCommand: null }));
            }}
            expandedItemIds={s().timeline.expandedItemIds}
            collapsedToolCallIds={s().timeline.collapsedToolCallIds}
            host={ctx.host}
          />
        </box>
      );

    case "status":
      return (
        <StatusPanel
          status={ctx.statusContract()}
          snapshot={ctx.orchestratorSnapshot()}
          currentAgentId={s().currentAgentId}
          viewedAgentId={s().viewedAgentId}
          expandedAgentId={s().expandedAgentId}
          width={ctx.layout().viewport.width}
          spinnerFrame={ctx.spinnerFrame()}
          onViewedAgentChange={(agentId) =>
            ctx.store.dispatch({ type: "viewed_agent_changed", agentId })
          }
          onToggleExpand={() => ctx.store.dispatch({ type: "agent_expansion_toggled" })}
        />
      );

    case "editor":
      return (
        <box flexShrink={0}>
          <Editor
            actionSvc={ctx.actionSvc}
            controller={ctx.ctrl}
            disabled={ctx.isRunning()}
            draft={s().input.draft}
            draftRevision={s().input.revision}
            onDraftChange={(text) => ctx.store.dispatch({ type: "editor_draft_changed", text })}
          />
        </box>
      );

    case "bottom-bar":
      return (
        <box flexShrink={0} height={ctx.layout().mode === "minimal" ? 1 : 2}>
          <BottomBar store={ctx.store} />
        </box>
      );

    default:
      return null;
  }
}
