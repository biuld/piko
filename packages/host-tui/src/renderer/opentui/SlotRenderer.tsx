// ============================================================================
// SlotRenderer — renders base slots (timeline, status, editor, bottom-bar)
// extracted from App.tsx to keep the shell lean.
// ============================================================================

import type { TuiStore } from "./store.js";
import type { TuiController } from "../../runtime/tui-controller.js";
import type { ActionService } from "./action-service.js";
import { TimelineView } from "./timeline/TimelineView.js";
import { StatusLine } from "./StatusLine.js";
import { Editor } from "./Editor.js";
import { BottomBar } from "./BottomBar.js";

export interface SlotContext {
  timelineItems: () => any[];
  layout: () => any;
  state: () => any;
  statusEntries: () => any[];
  statusHeight: () => number;
  isRunning: () => boolean;
  blocking: () => boolean;
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
        <box flexShrink={0} height={ctx.statusHeight()}>
          <StatusLine
            entries={ctx.statusEntries()}
            visible={ctx.statusEntries().length > 0}
          />
        </box>
      );

    case "editor":
      return (
        <box flexShrink={0}>
          <Editor
            actionSvc={ctx.actionSvc}
            controller={ctx.ctrl}
            disabled={ctx.isRunning()}
            unfocused={ctx.blocking()}
          />
        </box>
      );

    case "bottom-bar":
      return (
        <box
          flexShrink={0}
          height={ctx.layout().mode === "minimal" ? 1 : 2}
        >
          <BottomBar store={ctx.store} />
        </box>
      );

    default:
      return null;
  }
}
