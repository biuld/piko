import { type Component, getKeybindings, truncateToWidth } from "@earendil-works/pi-tui";
import { getSearchableText, type SessionTreeNode } from "piko-host-runtime";
import { getTheme } from "../../theme.js";
import { getEntryDisplayText, hasTextContent, isSettingsEntry } from "./display.js";
import { flattenTree } from "./flatten.js";
import type { FilterMode, FlatNode, GutterInfo } from "./types.js";

interface ToolCallInfo {
  name: string;
  arguments: Record<string, unknown>;
}

export class TreeList implements Component {
  private toolCallMap = new Map<string, ToolCallInfo>();
  private flatNodes: FlatNode[] = [];
  private filteredNodes: FlatNode[] = [];
  private selectedIndex = 0;
  private maxVisibleLines: number;
  private filterMode: FilterMode = "default";
  private searchQuery = "";
  private currentLeafId: string | null;
  private activePathIds = new Set<string>();
  private foldedNodes = new Set<string>();
  private multipleRoots = false;
  private lastSelectedId: string | null = null;

  onSelect?: (entryId: string) => void;
  onCancel?: () => void;

  constructor(tree: SessionTreeNode[], currentLeafId: string | null, maxVisibleLines: number) {
    this.currentLeafId = currentLeafId;
    this.maxVisibleLines = maxVisibleLines;
    this.multipleRoots = tree.length > 1;

    const { nodes, toolCallMap } = flattenTree(tree, currentLeafId);
    this.flatNodes = nodes;
    this.toolCallMap = toolCallMap;
    this.buildActivePath();
    this.applyFilter();
    this.selectedIndex = this.findNearestVisibleIndex(currentLeafId);
  }

  private buildActivePath(): void {
    this.activePathIds.clear();
    if (!this.currentLeafId) return;
    const byId = new Map(this.flatNodes.map((n) => [n.node.entry.id, n]));
    let current: string | null = this.currentLeafId;
    while (current) {
      this.activePathIds.add(current);
      current = byId.get(current)?.node.entry.parentId ?? null;
    }
  }

  private findNearestVisibleIndex(entryId: string | null): number {
    if (this.filteredNodes.length === 0) return 0;
    const entryMap = new Map(this.flatNodes.map((n) => [n.node.entry.id, n]));
    const visibleIdToIndex = new Map(this.filteredNodes.map((n, i) => [n.node.entry.id, i]));
    let currentId = entryId;
    while (currentId !== null) {
      const idx = visibleIdToIndex.get(currentId);
      if (idx !== undefined) return idx;
      currentId = entryMap.get(currentId)?.node.entry.parentId ?? null;
    }
    return this.filteredNodes.length - 1;
  }

  private applyFilter(): void {
    if (this.filteredNodes.length > 0) {
      this.lastSelectedId =
        this.filteredNodes[this.selectedIndex]?.node.entry.id ?? this.lastSelectedId;
    }

    const searchTokens = this.searchQuery.toLowerCase().split(/\s+/).filter(Boolean);

    this.filteredNodes = this.flatNodes
      .filter((fn) => {
        const entry = fn.node.entry;
        const isCurrentLeaf = entry.id === this.currentLeafId;
        if (entry.type === "message" && entry.message.role === "assistant" && !isCurrentLeaf) {
          if (!hasTextContent(entry.message.content)) return false;
        }
        switch (this.filterMode) {
          case "user-only":
            return entry.type === "message" && entry.message.role === "user";
          case "no-tools":
            return (
              !isSettingsEntry(entry) &&
              !(entry.type === "message" && entry.message.role === "toolResult")
            );
          case "labeled-only":
            return fn.node.label !== undefined;
          case "all":
            return true;
          default:
            return !isSettingsEntry(entry);
        }
      })
      .filter((fn) => {
        if (searchTokens.length === 0) return true;
        const text = getSearchableText(fn.node).toLowerCase();
        return searchTokens.every((t) => text.includes(t));
      });

    if (this.foldedNodes.size > 0) {
      const skipSet = new Set<string>();
      for (const fn of this.flatNodes) {
        const { id, parentId } = fn.node.entry;
        if (parentId != null && (this.foldedNodes.has(parentId) || skipSet.has(parentId))) {
          skipSet.add(id);
        }
      }
      this.filteredNodes = this.filteredNodes.filter((fn) => !skipSet.has(fn.node.entry.id));
    }

    this.recalculateVisualStructure();

    if (this.lastSelectedId) {
      this.selectedIndex = this.findNearestVisibleIndex(this.lastSelectedId);
    } else {
      this.selectedIndex = Math.min(this.selectedIndex, Math.max(0, this.filteredNodes.length - 1));
    }
    if (this.filteredNodes.length > 0) {
      this.lastSelectedId =
        this.filteredNodes[this.selectedIndex]?.node.entry.id ?? this.lastSelectedId;
    }
  }

  private recalculateVisualStructure(): void {
    if (this.filteredNodes.length === 0) return;

    const visibleIds = new Set(this.filteredNodes.map((n) => n.node.entry.id));
    const entryMap = new Map(this.flatNodes.map((n) => [n.node.entry.id, n]));

    const findVisibleAncestor = (nodeId: string): string | null => {
      let currentId = entryMap.get(nodeId)?.node.entry.parentId ?? null;
      while (currentId !== null) {
        if (visibleIds.has(currentId)) return currentId;
        currentId = entryMap.get(currentId)?.node.entry.parentId ?? null;
      }
      return null;
    };

    const visibleChildren = new Map<string | null, string[]>();
    visibleChildren.set(null, []);
    for (const fn of this.filteredNodes) {
      const nodeId = fn.node.entry.id;
      const ancestorId = findVisibleAncestor(nodeId);
      if (!visibleChildren.has(ancestorId)) visibleChildren.set(ancestorId, []);
      visibleChildren.get(ancestorId)!.push(nodeId);
    }

    const visibleRootIds = visibleChildren.get(null)!;
    this.multipleRoots = visibleRootIds.length > 1;

    const filteredNodeMap = new Map(this.filteredNodes.map((n) => [n.node.entry.id, n]));

    type StackItem = [string, number, boolean, boolean, boolean, GutterInfo[], boolean];
    const stack: StackItem[] = [];
    for (let i = visibleRootIds.length - 1; i >= 0; i--) {
      const isLast = i === visibleRootIds.length - 1;
      stack.push([
        visibleRootIds[i],
        this.multipleRoots ? 1 : 0,
        this.multipleRoots,
        this.multipleRoots,
        isLast,
        [],
        this.multipleRoots,
      ]);
    }

    while (stack.length > 0) {
      const [nodeId, indent, justBranched, showConnector, isLast, gutters, isVirtualRootChild] =
        stack.pop()!;
      const fn = filteredNodeMap.get(nodeId)!;
      fn.indent = indent;
      fn.showConnector = showConnector;
      fn.isLast = isLast;
      fn.gutters = gutters;
      fn.isVirtualRootChild = isVirtualRootChild;

      const childIds = visibleChildren.get(nodeId) ?? [];
      const multipleChildren = childIds.length > 1;

      const childIndent = multipleChildren || (justBranched && indent > 0) ? indent + 1 : indent;

      const connectorDisplayed = showConnector && !isVirtualRootChild;
      const currentDisplayIndent = this.multipleRoots ? Math.max(0, indent - 1) : indent;
      const connectorPosition = Math.max(0, currentDisplayIndent - 1);
      const childGutters: GutterInfo[] = connectorDisplayed
        ? [...gutters, { position: connectorPosition, show: !isLast }]
        : gutters;

      for (let i = childIds.length - 1; i >= 0; i--) {
        const childIsLast = i === childIds.length - 1;
        stack.push([
          childIds[i],
          childIndent,
          multipleChildren,
          multipleChildren,
          childIsLast,
          childGutters,
          false,
        ]);
      }
    }
  }

  getSearchQuery(): string {
    return this.searchQuery;
  }
  getFilterLabel(): string {
    return this.filterMode === "default" ? "" : ` [${this.filterMode}]`;
  }

  handleInput(keyData: string): void {
    const kb = getKeybindings();

    if (kb.matches(keyData, "tui.select.up")) {
      if (this.filteredNodes.length === 0) return;
      this.selectedIndex =
        this.selectedIndex > 0 ? this.selectedIndex - 1 : this.filteredNodes.length - 1;
    } else if (kb.matches(keyData, "tui.select.down")) {
      if (this.filteredNodes.length === 0) return;
      this.selectedIndex =
        this.selectedIndex < this.filteredNodes.length - 1 ? this.selectedIndex + 1 : 0;
    } else if (kb.matches(keyData, "tui.select.confirm")) {
      const node = this.filteredNodes[this.selectedIndex];
      if (node) this.onSelect?.(node.node.entry.id);
    } else if (kb.matches(keyData, "tui.select.cancel")) {
      if (this.searchQuery) {
        this.searchQuery = "";
        this.foldedNodes.clear();
        this.applyFilter();
      } else this.onCancel?.();
    } else if (keyData === "f" || keyData === "F") {
      const node = this.filteredNodes[this.selectedIndex];
      if (node) {
        const id = node.node.entry.id;
        if (this.foldedNodes.has(id)) this.foldedNodes.delete(id);
        else if (node.node.children.length > 0) this.foldedNodes.add(id);
        this.applyFilter();
      }
    } else if (keyData === "t") {
      const modes: FilterMode[] = ["default", "user-only", "no-tools", "labeled-only", "all"];
      const idx = modes.indexOf(this.filterMode);
      this.filterMode = modes[(idx + 1) % modes.length];
      this.foldedNodes.clear();
      this.applyFilter();
    } else if (kb.matches(keyData, "tui.editor.deleteCharBackward")) {
      if (this.searchQuery.length > 0) {
        this.searchQuery = this.searchQuery.slice(0, -1);
        this.foldedNodes.clear();
        this.applyFilter();
      }
    } else if (kb.matches(keyData, "tui.editor.cursorLeft")) {
      this.selectedIndex = Math.max(0, this.selectedIndex - this.maxVisibleLines);
    } else if (kb.matches(keyData, "tui.editor.cursorRight")) {
      this.selectedIndex = Math.min(
        this.filteredNodes.length - 1,
        this.selectedIndex + this.maxVisibleLines,
      );
    } else if (keyData.length === 1 && keyData.charCodeAt(0) >= 32) {
      this.searchQuery += keyData;
      this.foldedNodes.clear();
      this.applyFilter();
    }
  }

  invalidate(): void {}

  render(width: number): string[] {
    const t = getTheme();
    if (this.filteredNodes.length === 0) {
      return [truncateToWidth(t.fg("muted", "  No entries found"), width)];
    }

    const half = Math.floor(this.maxVisibleLines / 2);
    const start = Math.max(
      0,
      Math.min(this.selectedIndex - half, this.filteredNodes.length - this.maxVisibleLines),
    );
    const end = Math.min(start + this.maxVisibleLines, this.filteredNodes.length);
    const lines: string[] = [];

    for (let i = start; i < end; i++) {
      const fn = this.filteredNodes[i];
      const isSelected = i === this.selectedIndex;
      const isActive = this.activePathIds.has(fn.node.entry.id);
      const cursor = isSelected ? t.fg("accent", "› ") : "  ";
      const activeMarker = isActive ? t.fg("success", "• ") : "  ";
      const label = getEntryDisplayText(fn.node.entry, this.toolCallMap);
      let line = `${cursor}${activeMarker}${this.renderPrefix(fn)}${label}`;
      if (isSelected) line = t.fg("accent", line);
      lines.push(truncateToWidth(line, width));
    }

    const status = `  (${this.selectedIndex + 1}/${this.filteredNodes.length})${this.getFilterLabel()}${this.searchQuery ? `  search: "${this.searchQuery}"` : ""}`;
    lines.push(truncateToWidth(t.fg("dim", status), width));
    return lines;
  }

  private renderPrefix(fn: FlatNode): string {
    const indent = this.multipleRoots ? Math.max(0, fn.indent - 1) : fn.indent;
    const chars: string[] = [];
    for (let level = 0; level < indent; level++) {
      const gutter = fn.gutters.find((g) => g.position === level);
      if (gutter) {
        chars.push(gutter.show ? "│  " : "   ");
      } else if (level === indent - 1 && fn.showConnector && !fn.isVirtualRootChild) {
        const isFolded = this.foldedNodes.has(fn.node.entry.id);
        if (isFolded) chars.push(fn.isLast ? "└⊞ " : "├⊞ ");
        else if (fn.node.children.length > 0) chars.push(fn.isLast ? "└⊟ " : "├⊟ ");
        else chars.push(fn.isLast ? "└─ " : "├─ ");
      } else {
        chars.push("   ");
      }
    }
    if (indent === 0 && fn.showConnector && !fn.isVirtualRootChild) {
      const isFolded = this.foldedNodes.has(fn.node.entry.id);
      if (isFolded) chars.push("⊞ ");
      else if (fn.node.children.length > 0) chars.push("⊟ ");
    }
    return chars.join("");
  }
}
