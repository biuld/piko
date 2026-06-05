// ============================================================================
// Resume Session Selector — uses SelectListView + keyboard through focus
// ============================================================================

import { createSignal, createMemo, onCleanup, onMount } from "solid-js";
import type { SessionMeta } from "piko-host-runtime";
import type { ActionService } from "../action-service.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectorShell } from "./SelectorShell.js";
import { SelectListView } from "./SelectListView.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import {
  createSelectableListState,
  filterSelectableItems,
  getSelectedItem,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import { selectorBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";

export interface ResumeSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  onClose: () => void;
}

export function ResumeSelector(props: ResumeSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose } = props;
  const [sessions, setSessions] = createSignal<SessionMeta[]>([]);
  const [listState, setListState] = createSignal<SelectableListState>(
    createSelectableListState(),
  );
  const [loading, setLoading] = createSignal(true);
  const [switching, setSwitching] = createSignal(false);

  // Load sessions and register keyboard handler
  onMount(async () => {
    try {
      const all = await actionSvc.host.listSessions({});
      setSessions(all);
    } catch {
      // Ignore
    } finally {
      setLoading(false);
    }
  });

  const allItems = createMemo<SelectItem<string>[]>(() =>
    sessions().map((session) => {
      const name = session.name ?? session.id.slice(0, 12);
      const date = new Date(session.modified).toLocaleDateString();
      return {
        id: session.id,
        label: name,
        description: `${date} — ${session.model} — ${session.messageCount} msgs`,
        value: session.path,
      };
    }),
  );

  const items = createMemo<SelectItem<string>[]>(() =>
    filterSelectableItems(allItems(), listState().query),
  );

  async function confirm(): Promise<void> {
    if (switching()) return;
    const item = getSelectedItem(items(), listState().selectedIndex);
    if (!item) return;

    setSwitching(true);
    try {
      await actionSvc.switchSession(item.value);
    } catch {
      // Session switch may fail
    } finally {
      setSwitching(false);
      onClose();
    }
  }

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): SurfaceKeyResult {
        const { nextState, result } = selectorBehavior(event, listState(), items().length);
        setListState(nextState);
        return result;
      },
      onConfirm() {
        confirm();
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

  if (loading()) {
    return (
      <SelectorShell title="Resume Session" onClose={onClose}>
        <text>Loading sessions...</text>
      </SelectorShell>
    );
  }

  if (switching()) {
    return (
      <SelectorShell title="Resume Session" onClose={onClose}>
        <text>Switching session...</text>
      </SelectorShell>
    );
  }

  return (
    <SelectorShell
      title="Resume Session"
      onClose={onClose}
      hints={[
        controller.keymap.formatHintLine([
          ["tui.select.up", "navigate"],
          ["tui.select.down", ""],
          ["tui.select.confirm", "select"],
          ["tui.select.cancel", "cancel"],
        ]) + "  Type to filter",
      ]}
    >
      <box height={1} paddingBottom={1}>
        <text>{listState().query || "Type to filter sessions..."}</text>
      </box>

      <SelectListView
        items={items()}
        selectedIndex={listState().selectedIndex}
        width={actionSvc.getState().layout.viewport.width}
        maxHeight={12}
        onSelect={() => {}}
      />
    </SelectorShell>
  );
}
