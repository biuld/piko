// ============================================================================
// SlotRenderer — renders base slots (timeline, status, editor, bottom-bar)
// extracted from App.tsx to keep the shell lean.
// ============================================================================

import type { TuiController } from "../../runtime/tui-controller.js";
import type { ActionService } from "./action-service.js";
import { BottomBar } from "./BottomBar.js";
import { Editor } from "./Editor.js";
import { StatusLine } from "./StatusLine.js";
import type { TuiStore } from "./store.js";
import { TimelineView } from "./timeline/TimelineView.js";

export interface SlotContext {
  timelineItems: () => any[];
  layout: () => any;
  state: () => any;
  statusContract: () => any;
  isRunning: () => boolean;
  store: TuiStore;
  actionSvc: ActionService;
  ctrl: TuiController;
}

export function renderSlot(slotId: string, ctx: SlotContext) {
  const s = ctx.state;

  switch (slotId) {
    case "timeline":
      return (
        <box flexGrow={1} flexShrink={1} overflow="hidden">
          <TimelineView
            items={ctx.timelineItems()}
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
          />
        </box>
      );

    case "status":
      return (
        <StatusLine
          status={ctx.statusContract()}
          sessionTitle={sessionTitle(s().session)}
          width={ctx.layout().viewport.width}
        />
      );

    case "editor":
      return (
        <box flexShrink={0}>
          <Editor actionSvc={ctx.actionSvc} controller={ctx.ctrl} disabled={ctx.isRunning()} />
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

function sessionTitle(session: { sessionName?: string; cwd?: string }): string {
  if (session.sessionName?.trim()) return session.sessionName.trim();
  const cwd = session.cwd?.replace(/\/+$/, "");
  if (!cwd) return "session";
  return cwd.split("/").pop() || cwd;
}
