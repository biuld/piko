// ============================================================================
// Resume Session Selector — uses SelectListView + keyboard through focus
// ============================================================================

import { createSignal, createMemo, onCleanup, onMount, Show } from "solid-js";
import type { SessionMeta } from "piko-host-runtime";
import type { ActionService } from "../action-service.js";
import type { SelectItem } from "./selector-controller.js";
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
  initialQuery?: string;
  maxHeight?: number;
  onQueryChange?: (query: string) => void;
  onClose: () => void;
}

export function ResumeSelector(props: ResumeSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose, initialQuery } = props;
  const [sessions, setSessions] = createSignal<SessionMeta[]>([]);
  const [listState, setListState] = createSignal<SelectableListState>({
    ...createSelectableListState(),
    query: initialQuery || "",
  });
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
        if (nextState.query !== listState().query) {
          props.onQueryChange?.(nextState.query);
        }
        setListState(nextState);
        return result;
      },
      onConfirm() {
        confirm();
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

  const surface = () => controller.store.state().surfaces.find((s) => s.id === surfaceId);
  const placement = () => surface()?.placement ?? "partial";
  const viewportHeight = () => controller.store.state().layout.viewport.height;

  const maxHeight = () => {
    if (props.maxHeight !== undefined) return props.maxHeight;
    if (placement() === "full") {
      return Math.max(15, viewportHeight() - 6);
    }
    return 9; // 12 - 1 (hints) - 2 (filterRow)
  };

  return (
    <box flexDirection="column">
      {loading() || switching() ? (
        <box flexDirection="column">
          <text>{switching() ? "Switching session..." : "Loading sessions..."}</text>
        </box>
      ) : (
        <box flexDirection="column">
          <SelectListView
            items={items()}
            selectedIndex={listState().selectedIndex}
            width={actionSvc.getState().layout.viewport.width}
            maxHeight={maxHeight()}
            onSelect={() => {}}
          />
        </box>
      )}
    </box>
  );
}
