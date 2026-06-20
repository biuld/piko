// ============================================================================
// Tree Selector — pi-style session tree with filter modes + search
// ============================================================================

import type { FlatTreeEntry, FlattenedTreeItem } from "piko-host-runtime";
import { getSearchableText, recalculateVisibleFlatTree, renderFlatTree } from "piko-host-runtime";
import { createEffect, createMemo, createSignal, onCleanup, onMount, untrack } from "solid-js";
import type { KeyEvent } from "../../../focus/types.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import { type SurfaceKeyResult, selectorBehavior } from "../../../surfaces/index.js";
import {
  createSelectableListState,
  filterSelectableItems,
  getSelectedItem,
  nearestSelectableIndex,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import { FilterBar, ListBody, StatusText } from "../primitives/index.js";
import { useTheme } from "../theme-context.js";
import type { SelectItem } from "./selector-controller.js";

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

function isUserMessageItem(item: FlattenedTreeItem | undefined): boolean {
  const entry = item?.value;
  return entry?.type === "message" && entry.message.role === "user";
}

// ============================================================================
// Props
// ============================================================================

export interface TreeSelectorProps {
  entries: FlatTreeEntry[];
  leafId: string | null;
  loading: boolean;
  onSelect(entryId: string): Promise<void>;
  onCancel(): void;
  controller: TuiController;
  surfaceId: string;
  maxHeight?: number;
  availableWidth?: number;
  availableHeight?: number;
  initialQuery?: string;
  onQueryChange?: (query: string) => void;
}

// ============================================================================
// Component
// ============================================================================

export function TreeSelector(props: TreeSelectorProps) {
  const {
    controller,
    surfaceId,
    initialQuery,
    onQueryChange,
    maxHeight,
    availableHeight,
    availableWidth,
  } = props;

  const w = availableWidth ?? 80;
  const totalH = maxHeight ?? availableHeight ?? 15;
  const listMaxH = () => Math.max(1, totalH - 4); // FilterBar(1) + mode(1) + gap(2)

  const [filterMode, setFilterMode] = createSignal<TreeFilterMode>("no-tools");
  const [listState, setListState] = createSignal<SelectableListState>({
    ...createSelectableListState(),
    query: initialQuery || "",
  });
  const [submitting, setSubmitting] = createSignal(false);

  // Derive visibleItems from props.entries
  const visibleItems = createMemo(() => {
    return renderVisibleItems(props.entries, filterMode(), listState().query);
  });

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

  // Set initial selectedIndex based on props.leafId on load
  createEffect(() => {
    const entries = props.entries;
    const leafId = props.leafId;
    const loading = props.loading;
    if (loading || entries.length === 0) return;

    untrack(() => {
      const initialVisibleItems = visibleItems();
      const selectedIndex = findNearestVisibleIndex(leafId, initialVisibleItems, entries, (index) =>
        isUserMessageItem(initialVisibleItems[index]),
      );
      setListState((prev) => ({ ...prev, selectedIndex }));
    });
  });

  // Confirm: navigate session tree to selected entry
  async function confirm() {
    if (submitting()) return;
    const item = getSelectedItem(items(), listState().selectedIndex);
    if (!item) return;
    const treeItem = item.value as FlattenedTreeItem;
    if (!isUserMessageItem(treeItem)) return;
    const entryId = treeItem.value.id;

    setSubmitting(true);
    try {
      await props.onSelect(entryId);
    } finally {
      setSubmitting(false);
    }
  }

  // Cycle filter mode
  function cycleFilterMode(direction: 1 | -1) {
    const selectedEntryId = getSelectedItem(items(), listState().selectedIndex)?.id ?? null;
    setFilterMode((prev) => {
      const idx = FILTER_MODES.indexOf(prev);
      const nextMode = FILTER_MODES[(idx + direction + FILTER_MODES.length) % FILTER_MODES.length];
      const nextItems = renderVisibleItems(props.entries, nextMode, listState().query);
      const selectedIndex = findNearestVisibleIndex(
        selectedEntryId,
        nextItems,
        props.entries,
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
        if (submitting()) return { type: "handled" };

        if (event.name === "escape") {
          props.onCancel();
          return { type: "handled" };
        }

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
          const nextVisibleItems = renderVisibleItems(props.entries, filterMode(), nextState.query);
          const selectedIndex = findNearestVisibleIndex(
            selectedEntryId,
            nextVisibleItems,
            props.entries,
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

  const theme = useTheme();

  return (
    <box flexDirection="column">
      {props.loading ? (
        <StatusText text="Loading tree..." />
      ) : props.entries.length === 0 ? (
        <StatusText text="No entries in session tree" />
      ) : (
        <box flexDirection="column">
          <FilterBar query={listState().query} placeholder="Search entries..." />
          <box height={1} flexDirection="row" paddingLeft={1}>
            <text fg={theme.color("text.dim")}>{`  [${filterMode()}]`}</text>
          </box>
          <box height={1} />
          <ListBody
            items={items()}
            selectedIndex={listState().selectedIndex}
            width={w}
            maxHeight={listMaxH()}
            scrollPolicy="center"
            showDescriptions={false}
          />
        </box>
      )}
    </box>
  );
}
