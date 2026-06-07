// ============================================================================
// Resume Session Selector — uses SelectListView + keyboard through focus
// ============================================================================

import { createSignal, createMemo, onCleanup, onMount } from "solid-js";
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

// ---- Helpers ----

/** Format a date as a relative time string (e.g. "2h", "3d"). */
function formatSessionDate(date: Date): string {
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return "now";
  if (diffMins < 60) return `${diffMins}m`;
  if (diffHours < 24) return `${diffHours}h`;
  if (diffDays < 7) return `${diffDays}d`;
  if (diffDays < 30) return `${Math.floor(diffDays / 7)}w`;
  if (diffDays < 365) return `${Math.floor(diffDays / 30)}mo`;
  return `${Math.floor(diffDays / 365)}y`;
}

export interface ResumeSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  initialQuery?: string;
  maxHeight?: number;
  availableWidth?: number;
  availableHeight?: number;
  onQueryChange?: (query: string) => void;
  onClose: () => void;
}

export function ResumeSelector(props: ResumeSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose, initialQuery, availableHeight, maxHeight } = props;
  const listMaxH = () => maxHeight ?? availableHeight ?? 12;
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
      const rawTitle = session.name || session.preview || session.id.slice(0, 12);
      const title = rawTitle.replace(/[\x00-\x1f\x7f]/g, " ").trim() || session.id.slice(0, 12);
      const age = formatSessionDate(new Date(session.modified));
      return {
        id: session.id,
        label: title,
        meta: `${session.messageCount} msgs · ${age}`,
        value: session.id,
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
        const total = items().length;
        const { nextState, result } = selectorBehavior(event, listState(), total);
        if (nextState.query !== listState().query) {
          props.onQueryChange?.(nextState.query);
        }
        // Safety clamp: ensure selectedIndex never exceeds bounds
        const clamped = Math.max(0, Math.min(nextState.selectedIndex, Math.max(0, total - 1)));
        setListState({ ...nextState, selectedIndex: clamped });
        return result;
      },
      onConfirm() {
        confirm();
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

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
            maxHeight={listMaxH()}
            scrollPolicy="edge"
            itemSpacing={1}
            onSelect={() => {}}
          />
        </box>
      )}
    </box>
  );
}
