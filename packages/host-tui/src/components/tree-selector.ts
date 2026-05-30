import {
  type Component,
  type Focusable,
  getKeybindings,
  truncateToWidth,
} from "@earendil-works/pi-tui";
import {
  type FlatNode,
  flattenTree,
  getEntryLabel,
  getSearchableText,
  type SessionTreeNode,
} from "./session-tree-node.js";

// ---- Types ----

type FilterMode = "all" | "no-tools" | "user-only";

// ---- TreeList ----

class TreeList implements Component {
  private flatNodes: FlatNode[] = [];
  private filteredNodes: FlatNode[] = [];
  private selectedIndex = 0;
  private maxVisibleLines: number;
  private filterMode: FilterMode = "all";
  private searchQuery = "";
  private currentLeafId: string | null;
  private activePathIds = new Set<string>();
  private foldedNodes = new Set<string>();
  private multipleRoots = false;

  onSelect?: (entryId: string) => void;
  onCancel?: () => void;

  constructor(tree: SessionTreeNode[], currentLeafId: string | null, maxVisibleLines: number) {
    this.currentLeafId = currentLeafId;
    this.maxVisibleLines = maxVisibleLines;
    this.multipleRoots = tree.length > 1;

    this.flatNodes = flattenTree(tree);
    this.buildActivePath();
    this.applyFilter();

    if (currentLeafId) {
      this.selectedIndex = this.indexOfId(currentLeafId);
    }
  }

  private buildActivePath(): void {
    this.activePathIds.clear();
    if (!this.currentLeafId) return;

    const byId = new Map<string, FlatNode>();
    for (const fn of this.flatNodes) {
      byId.set(fn.node.entry.id, fn);
    }

    let current: string | null = this.currentLeafId;
    while (current) {
      this.activePathIds.add(current);
      const node = byId.get(current);
      current = node?.node.entry.parentId ?? null;
    }
  }

  private indexOfId(id: string): number {
    const idx = this.filteredNodes.findIndex((n) => n.node.entry.id === id);
    return idx >= 0 ? idx : this.filteredNodes.length > 0 ? this.filteredNodes.length - 1 : 0;
  }

  private applyFilter(): void {
    const searchTokens = this.searchQuery.toLowerCase().split(/\s+/).filter(Boolean);

    this.filteredNodes = this.flatNodes.filter((fn) => {
      const entry = fn.node.entry;

      // Filter mode
      switch (this.filterMode) {
        case "user-only":
          if (entry.type !== "message" || entry.message.role !== "user") return false;
          break;
        case "no-tools":
          if (entry.type === "message" && entry.message.role === "toolResult") return false;
          break;
        // "all" shows everything
      }

      // Search
      if (searchTokens.length > 0) {
        const text = getSearchableText(fn.node).toLowerCase();
        return searchTokens.every((t) => text.includes(t));
      }

      return true;
    });

    // Hide folded descendants
    if (this.foldedNodes.size > 0) {
      const hidden = new Set<string>();
      for (const fn of this.flatNodes) {
        const { id, parentId } = fn.node.entry;
        if (parentId && (this.foldedNodes.has(parentId) || hidden.has(parentId))) {
          hidden.add(id);
        }
      }
      this.filteredNodes = this.filteredNodes.filter((fn) => !hidden.has(fn.node.entry.id));
    }

    // Recalculate visual structure for filtered set
    if (this.filteredNodes.length > 0) {
      this.recalculateVisuals();
    }

    this.selectedIndex = Math.min(this.selectedIndex, Math.max(0, this.filteredNodes.length - 1));
  }

  private recalculateVisuals(): void {
    const visibleIds = new Set(this.filteredNodes.map((n) => n.node.entry.id));
    const byId = new Map(this.flatNodes.map((n) => [n.node.entry.id, n]));

    // Find nearest visible ancestor
    const visibleParent = (nodeId: string): string | null => {
      let current = byId.get(nodeId)?.node.entry.parentId ?? null;
      while (current) {
        if (visibleIds.has(current)) return current;
        current = byId.get(current)?.node.entry.parentId ?? null;
      }
      return null;
    };

    // Group by visible parent
    const childrenByParent = new Map<string | null, FlatNode[]>();
    for (const fn of this.filteredNodes) {
      const pid = visibleParent(fn.node.entry.id);
      if (!childrenByParent.has(pid)) childrenByParent.set(pid, []);
      childrenByParent.get(pid)!.push(fn);
    }

    // Build new indentation
    for (const fn of this.filteredNodes) {
      let indent = 0;
      let pid = visibleParent(fn.node.entry.id);
      while (pid) {
        const siblings =
          childrenByParent.get(visibleParent(pid)) ?? childrenByParent.get(null) ?? [];
        indent += siblings.length > 1 ? 1 : 0;
        if (siblings.length <= 1) indent += 1;
        pid = visibleParent(pid);
        if (indent > 20) break;
      }
      fn.indent = indent;
    }
  }

  getSearchQuery(): string {
    return this.searchQuery;
  }

  getFilterLabel(): string {
    if (this.filterMode !== "all") return ` [${this.filterMode}]`;
    return "";
  }

  handleInput(keyData: string): void {
    const kb = getKeybindings();

    if (kb.matches(keyData, "tui.select.up")) {
      this.selectedIndex =
        this.selectedIndex > 0 ? this.selectedIndex - 1 : this.filteredNodes.length - 1;
    } else if (kb.matches(keyData, "tui.select.down")) {
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
      } else {
        this.onCancel?.();
      }
    } else if (keyData === "f" || keyData === "F") {
      // Toggle fold
      const node = this.filteredNodes[this.selectedIndex];
      if (node) {
        const id = node.node.entry.id;
        if (this.foldedNodes.has(id)) {
          this.foldedNodes.delete(id);
        } else if (node.node.children.length > 0) {
          this.foldedNodes.add(id);
        }
        this.applyFilter();
      }
    } else if (keyData === "t") {
      // Cycle filter: all → user-only → no-tools → all
      const modes: FilterMode[] = ["all", "user-only", "no-tools"];
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
      // Printable character: append to search
      this.searchQuery += keyData;
      this.foldedNodes.clear();
      this.applyFilter();
    }
  }

  invalidate(): void {}

  render(width: number): string[] {
    if (this.filteredNodes.length === 0) {
      return [truncateToWidth("  No entries found", width)];
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

      const cursor = isSelected ? "› " : "  ";
      const activeMarker = isActive ? "• " : "  ";
      const label = getEntryLabel(fn.node.entry);

      // Build tree prefix
      const prefix = this.renderPrefix(fn);

      let line = `${cursor}${activeMarker}${prefix}${label}`;
      line = truncateToWidth(line, width);
      lines.push(line);
    }

    // Status bar
    const status = `  (${this.selectedIndex + 1}/${this.filteredNodes.length})${this.getFilterLabel()}${this.searchQuery ? `  search: "${this.searchQuery}"` : ""}`;
    lines.push(truncateToWidth(status, width));

    return lines;
  }

  private renderPrefix(fn: FlatNode): string {
    const indent = this.multipleRoots ? Math.max(0, fn.indent - 1) : fn.indent;
    const chars: string[] = [];

    for (let level = 0; level < indent; level++) {
      const gutter = fn.gutters.find((g) => g.position === level);
      if (gutter) {
        chars.push(gutter.show ? "│  " : "   ");
      } else if (level === indent - 1 && fn.showConnector) {
        const isFolded = this.foldedNodes.has(fn.node.entry.id);
        if (isFolded) {
          chars.push(fn.isLast ? "└⊞ " : "├⊞ ");
        } else if (fn.node.children.length > 0) {
          chars.push(fn.isLast ? "└⊟ " : "├⊟ ");
        } else {
          chars.push(fn.isLast ? "└─ " : "├─ ");
        }
      } else {
        chars.push("   ");
      }
    }

    // For indent=0 nodes that have children
    if (indent === 0 && fn.showConnector) {
      const isFolded = this.foldedNodes.has(fn.node.entry.id);
      if (isFolded) {
        chars.push("⊞ ");
      } else if (fn.node.children.length > 0) {
        chars.push("⊟ ");
      }
    }

    return chars.join("");
  }
}

// ---- TreeSelectorContainer ----

export class TreeSelectorComponent implements Component, Focusable {
  private treeList: TreeList;
  private _focused = false;

  get focused(): boolean {
    return this._focused;
  }

  set focused(value: boolean) {
    this._focused = value;
  }

  constructor(
    tree: SessionTreeNode[],
    currentLeafId: string | null,
    terminalHeight: number,
    onSelect: (entryId: string) => void,
    onCancel: () => void,
  ) {
    const maxVisibleLines = Math.max(8, Math.floor(terminalHeight * 0.6));
    this.treeList = new TreeList(tree, currentLeafId, maxVisibleLines);
    this.treeList.onSelect = onSelect;
    this.treeList.onCancel = onCancel;
  }

  handleInput(keyData: string): void {
    this.treeList.handleInput(keyData);
  }

  invalidate(): void {
    this.treeList.invalidate();
  }

  render(width: number): string[] {
    const innerWidth = Math.max(24, width - 4);
    const lines: string[] = [
      `┌${"─".repeat(innerWidth + 2)}┐`,
      `│ ${truncateToWidth("Session Tree", innerWidth)} │`,
      `├${"─".repeat(innerWidth + 2)}┤`,
      ...this.treeList.render(innerWidth).map((l) => `│ ${truncateToWidth(l, innerWidth)} │`),
      `├${"─".repeat(innerWidth + 2)}┤`,
      `│ ${truncateToWidth("↑↓ move  ↵ select  Esc cancel  f fold  t filter  type search", innerWidth)} │`,
      `└${"─".repeat(innerWidth + 2)}┘`,
    ];
    return lines;
  }
}

// ---- Re-export for consumers ----

export type { SessionTreeNode } from "./session-tree-node.js";
export { buildSessionTree, getEntryLabel } from "./session-tree-node.js";
