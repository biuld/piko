// ============================================================================
// Tree Selector — pi-style session tree with filter modes + search
// ============================================================================

import { createSignal, createMemo, onCleanup, onMount } from "solid-js";
import type { PikoHost, FlattenedTreeItem } from "piko-host-runtime";
import { flattenSessionTree, renderFlatTree } from "piko-host-runtime";
import type { ActionService } from "../action-service.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectListView } from "./SelectListView.js";
import { useTheme } from "../theme-context.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import {
  createSelectableListState,
  filterSelectableItems,
  getSelectedItem,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import { selectorBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";

// ============================================================================
// Filter modes (matching pi's tree filter modes)
// ============================================================================

export type TreeFilterMode = "default" | "no-tools" | "user-only" | "all";

const FILTER_MODES: TreeFilterMode[] = ["default", "no-tools", "user-only", "all"];

// Settings/bookkeeping entry types hidden in "default" mode
const SETTINGS_TYPES = new Set([
  "model_change",
  "thinking_level_change",
  "session_info",
  "label",
  "custom",
]);

function applyFilterMode(items: FlattenedTreeItem[], mode: TreeFilterMode): FlattenedTreeItem[] {
  switch (mode) {
    case "default":
      return items.filter((item) => {
        const entry = item.value;
        if (SETTINGS_TYPES.has(entry.type)) return false;
        return true;
      });
    case "no-tools":
      return items.filter((item) => {
        const entry = item.value;
        if (SETTINGS_TYPES.has(entry.type)) return false;
        if (entry.type === "message" && entry.message.role === "toolResult") return false;
        return true;
      });
    case "user-only":
      return items.filter((item) => {
        const entry = item.value;
        return entry.type === "message" && entry.message.role === "user";
      });
    case "all":
      return items;
  }
}

function filterModeLabel(mode: TreeFilterMode): string {
  switch (mode) {
    case "default":
      return "";
    case "no-tools":
      return "[no-tools]";
    case "user-only":
      return "[user]";
    case "all":
      return "[all]";
  }
}

// ============================================================================
// Props
// ============================================================================

export interface TreeSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  host: PikoHost;
  surfaceId: string;
  initialQuery?: string;
  onQueryChange?: (query: string) => void;
  onClose: () => void;
}

// ============================================================================
// Component
// ============================================================================

export function TreeSelector(props: TreeSelectorProps) {
  const { actionSvc, controller, host, surfaceId, onClose, initialQuery, onQueryChange } =
    props;

  const [allItems, setAllItems] = createSignal<FlattenedTreeItem[]>([]);
  const [filterMode, setFilterMode] = createSignal<TreeFilterMode>("default");
  const [listState, setListState] = createSignal<SelectableListState>({
    ...createSelectableListState(),
    query: initialQuery || "",
  });
  const [loading, setLoading] = createSignal(true);

  // Load tree data
  onMount(() => {
    const h = host as any;
    if (h?.getTreeEntries) {
      h.getTreeEntries()
        .then((entries: any[]) => {
          const { flat, multipleRoots } = flattenSessionTree(entries);
          const items = renderFlatTree(flat, multipleRoots);
          setAllItems(items);
        })
        .catch(() => setAllItems([]))
        .finally(() => setLoading(false));
    } else {
      setLoading(false);
    }
  });

  // Apply filter mode
  const modeFiltered = createMemo(() => applyFilterMode(allItems(), filterMode()));

  // Apply search query (uses the same filterSelectableItems utility)
  const selectItems = createMemo<SelectItem[]>(() =>
    modeFiltered().map((item) => ({
      id: item.id,
      label: item.label,
      segments: item.segments,
      description: item.description,
      value: item,
    })),
  );

  const items = createMemo<SelectItem[]>(() =>
    filterSelectableItems(selectItems(), listState().query),
  );

  // Confirm: navigate session tree to selected entry
  async function confirm() {
    const item = getSelectedItem(items(), listState().selectedIndex);
    if (!item) return;
    const entryId = (item.value as FlattenedTreeItem).value.id;

    // No-op if already at this entry
    const leafId = (host as any).getLeafId?.() as string | null;
    if (entryId === leafId) {
      controller.notifications.notify({
        message: "Already at this entry",
        severity: "info",
        source: "session",
      });
      return;
    }

    try {
      const h = host as any;
      if (h?.navigateToEntry) {
        await h.navigateToEntry(entryId);
        controller.notifications.notify({
          message: "Navigated to entry",
          severity: "success",
          source: "session",
        });
      }
    } catch (e: any) {
      controller.notifications.notify({
        message: `Navigation failed: ${e.message}`,
        severity: "error",
        source: "session",
      });
    }
    onClose();
  }

  // Cycle filter mode
  function cycleFilterMode(direction: 1 | -1) {
    setFilterMode((prev) => {
      const idx = FILTER_MODES.indexOf(prev);
      return FILTER_MODES[(idx + direction + FILTER_MODES.length) % FILTER_MODES.length];
    });
    setListState((prev) => ({ ...prev, selectedIndex: 0 }));
  }

  // Register surface controller for keyboard
  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): SurfaceKeyResult {
        // Filter mode cycling: f / Shift+F
        if (event.name === "f") {
          cycleFilterMode(event.shift ? -1 : 1);
          return { type: "handled" };
        }
        // Standard selector behavior (up/down, page, home/end, backspace, typing)
        const { nextState, result } = selectorBehavior(event, listState(), items().length);
        if (nextState.query !== listState().query) {
          onQueryChange?.(nextState.query);
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
  const theme = useTheme();

  const maxHeight = () => {
    // Reserve 1 row for our status line below SelectListView
    const statusLine = 1;
    if (placement() === "full") {
      return Math.max(15, viewportHeight() - 8) - statusLine;
    }
    return 12 - statusLine;
  };

  return (
    <box flexDirection="column">
      {loading() ? (
        <box padding={1}>
          <text>Loading tree...</text>
        </box>
      ) : allItems().length === 0 ? (
        <box padding={1}>
          <text>No entries in session tree</text>
        </box>
      ) : (
        <box flexDirection="column">
          <SelectListView
            items={items()}
            selectedIndex={listState().selectedIndex}
            width={actionSvc.getState().layout.viewport.width}
            maxHeight={maxHeight()}
            showDescriptions={false}
            onSelect={() => {}}
          />
          {/* Status line: position counter + active filter mode */}
          <box height={1}>
            <text fg={theme.color("text.dim")}>
              ({listState().selectedIndex + 1}/{items().length}){" "}
              {filterModeLabel(filterMode())}
            </text>
          </box>
        </box>
      )}
    </box>
  );
}
