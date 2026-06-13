// ============================================================================
// Resume Session Selector — FilterBar + SelectListView + HintBar.
// ============================================================================

import type { SessionMeta } from "piko-host-runtime";
import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { KeyEvent } from "../../../focus/types.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import { type SurfaceKeyResult, selectorBehavior } from "../../../surfaces/index.js";
import {
  createSelectableListState,
  filterSelectableItems,
  getSelectedItem,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import type { ActionService } from "../action-service.js";
import { FilterBar, ListBody, StatusText } from "../primitives/index.js";
import type { SelectItem } from "./selector-controller.js";

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
  onClose: () => void;
}

export function ResumeSelector(props: ResumeSelectorProps) {
  const {
    actionSvc,
    controller,
    surfaceId,
    onClose,
    initialQuery,
    availableWidth,
    availableHeight,
    maxHeight,
  } = props;
  const w = availableWidth ?? actionSvc.getState().layout.viewport.width;
  const totalH = maxHeight ?? availableHeight ?? 12;
  // FilterBar (1) + gap (1) + list
  const listMaxH = () => Math.max(1, totalH - 2);

  const [sessions, setSessions] = createSignal<SessionMeta[]>([]);
  const [listState, setListState] = createSignal<SelectableListState>({
    ...createSelectableListState(),
    query: initialQuery || "",
  });
  const [loading, setLoading] = createSignal(true);
  const [switching, setSwitching] = createSignal(false);

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
        meta: `${session.messageCount} msgs \u00b7 ${age}`,
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
        <StatusText text={switching() ? "Switching session..." : "Loading sessions..."} />
      ) : (
        <box flexDirection="column">
          <FilterBar query={listState().query} placeholder="Search sessions..." />
          <box height={1} />
          <ListBody
            items={items()}
            selectedIndex={listState().selectedIndex}
            width={w}
            maxHeight={listMaxH()}
            scrollPolicy="edge"
            itemSpacing={1}
          />
        </box>
      )}
    </box>
  );
}
