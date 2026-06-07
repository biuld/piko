// ============================================================================
// Tree Selector — pi-style session tree with filter modes + search
// ============================================================================

import { createSignal, createMemo, onCleanup, onMount } from "solid-js";
import type { PikoHost, FlatTreeEntry, FlattenedTreeItem } from "piko-host-runtime";
import {
  flattenSessionTree,
  getSearchableText,
  recalculateVisibleFlatTree,
  renderFlatTree,
} from "piko-host-runtime";
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
  nearestSelectableIndex,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import { selectorBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";

// ============================================================================
// Filter modes (matching pi's tree filter modes)
// ============================================================================

export type TreeFilterMode = "messages" | "no-tools" | "user-only" | "labeled-only" | "all";

const FILTER_MODES: TreeFilterMode[] = ["no-tools", "messages", "user-only", "labeled-only", "all"];

// Settings/bookkeeping entry types hidden in "messages" and "no-tools" modes
const SETTINGS_TYPES = new Set([
  "active_tools_change",
  "model_change",
  "thinking_level_change",
  "session_info",
  "label",
  "custom",
]);

function hasTextContent(content: unknown): boolean {
  if (typeof content === "string") return content.trim().length > 0;
  if (Array.isArray(content)) {
    return content.some(
      (c) =>
        typeof c === "object" &&
        c !== null &&
        "type" in c &&
        c.type === "text" &&
        typeof (c as { text?: unknown }).text === "string" &&
        (c as { text: string }).text.trim().length > 0,
    );
  }
  return false;
}

function applyFilterMode(nodes: FlatTreeEntry[], mode: TreeFilterMode): FlatTreeEntry[] {
  const visibleNodes = nodes.filter((node) => {
    const entry = node.node.entry;
    const isCurrentLeaf = Boolean((entry as { isLeaf?: boolean }).isLeaf);
    if (entry.type !== "message" || entry.message.role !== "assistant" || isCurrentLeaf) {
      return true;
    }

    const msg = entry.message as { content?: unknown; stopReason?: string; errorMessage?: string };
    const isErrorOrAborted =
      Boolean(msg.errorMessage) ||
      (msg.stopReason !== undefined && msg.stopReason !== "stop" && msg.stopReason !== "toolUse");
    return hasTextContent(msg.content) || isErrorOrAborted;
  });

  switch (mode) {
    case "messages":
      return visibleNodes.filter((node) => {
        const entry = node.node.entry;
        if (SETTINGS_TYPES.has(entry.type)) return false;
        return true;
      });
    case "no-tools":
      return visibleNodes.filter((node) => {
        const entry = node.node.entry;
        if (SETTINGS_TYPES.has(entry.type)) return false;
        if (entry.type === "message" && entry.message.role === "toolResult") return false;
        return true;
      });
    case "user-only":
      return visibleNodes.filter((node) => {
        const entry = node.node.entry;
        return entry.type === "message" && entry.message.role === "user";
      });
    case "labeled-only":
      return visibleNodes.filter((node) => node.node.label !== undefined);
    case "all":
      return visibleNodes;
  }
}

function applySearch(nodes: FlatTreeEntry[], query: string): FlatTreeEntry[] {
  const tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return nodes;
  return nodes.filter((node) => {
    const text = getSearchableText(node.node).toLowerCase();
    return tokens.every((token) => text.includes(token));
  });
}

function renderVisibleItems(
  flatNodes: FlatTreeEntry[],
  mode: TreeFilterMode,
  query: string,
): FlattenedTreeItem[] {
  const modeFiltered = applyFilterMode(flatNodes, mode);
  const queryFiltered = applySearch(modeFiltered, query);
  const { flat, multipleRoots } = recalculateVisibleFlatTree(queryFiltered, flatNodes);
  return renderFlatTree(flat, multipleRoots, flatNodes);
}

function findNearestVisibleIndex(
  entryId: string | null | undefined,
  visibleItems: FlattenedTreeItem[],
  flatNodes: FlatTreeEntry[],
  isSelectableIndex?: (index: number) => boolean,
): number {
  if (visibleItems.length === 0) return 0;
  const visibleIdToIndex = new Map(visibleItems.map((item, index) => [item.id, index]));
  const nodeById = new Map(flatNodes.map((node) => [node.node.entry.id, node]));

  let currentId = entryId ?? null;
  while (currentId !== null) {
    const index = visibleIdToIndex.get(currentId);
    if (index !== undefined) {
      return nearestSelectableIndex(index, visibleItems.length, isSelectableIndex);
    }
    currentId = nodeById.get(currentId)?.node.entry.parentId ?? null;
  }

  return nearestSelectableIndex(visibleItems.length - 1, visibleItems.length, isSelectableIndex);
}

function clampSelectedIndex(index: number, total: number): number {
  if (total <= 0) return 0;
  return Math.max(0, Math.min(index, total - 1));
}

function filterModeLabel(mode: TreeFilterMode): string {
  switch (mode) {
    case "messages":
      return "";
    case "no-tools":
      return "[no-tools]";
    case "user-only":
      return "[user]";
    case "labeled-only":
      return "[labeled]";
    case "all":
      return "[all]";
  }
}

function isUserMessageItem(item: FlattenedTreeItem | undefined): boolean {
  const entry = item?.value;
  return entry?.type === "message" && entry.message.role === "user";
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

  const [allFlatNodes, setAllFlatNodes] = createSignal<FlatTreeEntry[]>([]);
  const [allItems, setAllItems] = createSignal<FlattenedTreeItem[]>([]);
  const [filterMode, setFilterMode] = createSignal<TreeFilterMode>("no-tools");
  const [listState, setListState] = createSignal<SelectableListState>({
    ...createSelectableListState(),
    query: initialQuery || "",
  });
  const [loading, setLoading] = createSignal(true);

  // Load tree data
  onMount(() => {
    const h = host as any;
    if (h?.getTreeEntries) {
      // Resolve current leaf ID before loading to seed initial selection
      const leafPromise = typeof h.getLeafId === "function"
        ? (h.getLeafId() as Promise<string | null> | string | null)
        : Promise.resolve(null);

      Promise.all([
        leafPromise,
        h.getTreeEntries() as Promise<any[]>,
      ])
        .then(([leafId, entries]) => {
          const { flat, multipleRoots } = flattenSessionTree(entries, leafId ?? null);
          setAllFlatNodes(flat);
          const items = renderFlatTree(flat, multipleRoots);
          setAllItems(items);

          // Default selection to current leaf position
          const initialVisibleItems = renderVisibleItems(flat, filterMode(), listState().query);
          const selectedIndex = findNearestVisibleIndex(
            leafId,
            initialVisibleItems,
            flat,
            (index) => isUserMessageItem(initialVisibleItems[index]),
          );
          setListState((prev) => ({ ...prev, selectedIndex }));
        })
        .catch(() => setAllItems([]))
        .finally(() => setLoading(false));
    } else {
      setLoading(false);
    }
  });

  const visibleItems = createMemo(() => {
    return renderVisibleItems(allFlatNodes(), filterMode(), listState().query);
  });

  // Apply search query (uses the same filterSelectableItems utility)
  const selectItems = createMemo<SelectItem[]>(() =>
    visibleItems().map((item) => ({
      id: item.id,
      label: item.label,
      segments: item.segments,
      value: item,
    })),
  );

  const items = createMemo<SelectItem[]>(() => filterSelectableItems(selectItems(), ""));
  const isTreeSelectableIndex = (index: number) =>
    isUserMessageItem(items()[index]?.value as FlattenedTreeItem | undefined);

  // Confirm: navigate session tree to selected entry
  async function confirm() {
    const item = getSelectedItem(items(), listState().selectedIndex);
    if (!item) return;
    const treeItem = item.value as FlattenedTreeItem;
    if (!isUserMessageItem(treeItem)) return;
    const entryId = treeItem.value.id;

    // No-op if already at this entry
    const leafId = (await (host as any).getLeafId?.()) as string | null;
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
    const selectedEntryId = getSelectedItem(items(), listState().selectedIndex)?.id ?? null;
    setFilterMode((prev) => {
      const idx = FILTER_MODES.indexOf(prev);
      const nextMode = FILTER_MODES[(idx + direction + FILTER_MODES.length) % FILTER_MODES.length];
      const nextItems = renderVisibleItems(allFlatNodes(), nextMode, listState().query);
      const selectedIndex = findNearestVisibleIndex(
        selectedEntryId,
        nextItems,
        allFlatNodes(),
        (index) => isUserMessageItem(nextItems[index]),
      );
      setListState((state) => ({ ...state, selectedIndex }));
      return nextMode;
    });
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
        const selectedEntryId = getSelectedItem(items(), listState().selectedIndex)?.id ?? null;
        const { nextState, result } = selectorBehavior(event, listState(), items().length, {
          isSelectableIndex: isTreeSelectableIndex,
        });
        if (nextState.query !== listState().query) {
          onQueryChange?.(nextState.query);
          const nextVisibleItems = renderVisibleItems(allFlatNodes(), filterMode(), nextState.query);
          const selectedIndex = findNearestVisibleIndex(
            selectedEntryId,
            nextVisibleItems,
            allFlatNodes(),
            (index) => isUserMessageItem(nextVisibleItems[index]),
          );
          setListState({
            ...nextState,
            selectedIndex,
          });
          return result;
        }
        setListState({
          ...nextState,
          selectedIndex: nearestSelectableIndex(
            clampSelectedIndex(nextState.selectedIndex, items().length),
            items().length,
            isTreeSelectableIndex,
          ),
        });
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
    if (placement() === "full") {
      return Math.max(15, viewportHeight() - 7);
    }
    return 12;
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
            scrollPolicy="center"
            showDescriptions={false}
            onSelect={() => {}}
          />

        </box>
      )}
    </box>
  );
}
