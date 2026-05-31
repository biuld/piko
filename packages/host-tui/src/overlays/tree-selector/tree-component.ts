import { Container, type Focusable, Spacer, Text } from "@earendil-works/pi-tui";
import type { SessionTreeNode } from "piko-host-runtime";
import { buildSessionTree } from "piko-host-runtime";
import { DynamicBorder } from "../../components/dynamic-border.js";
import { keyHint } from "../../components/key-hints.js";
import { getTheme } from "../../theme.js";
import type { OverlayContext } from "../index.js";
import { TreeList } from "./tree-list.js";

// ---- TreeSelectorComponent ----

export class TreeSelectorComponent extends Container implements Focusable {
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
    super();
    const t = getTheme();
    const borderColor = (s: string) => t.fg("border", s);
    const maxVisibleLines = Math.max(8, Math.floor(terminalHeight * 0.6));

    this.treeList = new TreeList(tree, currentLeafId, maxVisibleLines);
    this.treeList.onSelect = onSelect;
    this.treeList.onCancel = onCancel;

    this.addChild(new DynamicBorder(borderColor));
    this.addChild(new Text(t.fg("accent", t.bold(" Session Tree")), 1, 0));
    this.addChild(new Spacer(1));
    this.addChild(this.treeList);
    this.addChild(new Spacer(1));
    this.addChild(
      new Text(
        `${keyHint("tui.select.up", "")}${keyHint("tui.select.down", "move")}  ${keyHint("tui.select.confirm", "select")}  ${keyHint("tui.select.cancel", "cancel")}  f fold  t filter  type search`,
        1,
        0,
      ),
    );
    this.addChild(new DynamicBorder(borderColor));
  }

  handleInput(keyData: string): void {
    this.treeList.handleInput(keyData);
  }
}

// ---- openTreeSelector ----

function closeOverlay(ctx: OverlayContext): void {
  ctx.getActiveOverlay()?.hide();
  ctx.setActiveOverlay(null);
}

export async function openTreeSelector(ctx: OverlayContext): Promise<void> {
  const treeEntries = await ctx.host.getTreeEntries();
  if (treeEntries.length === 0) {
    ctx.msg("system", "Current session has no saved entries yet");
    ctx.render();
    return;
  }

  const tree = buildSessionTree(treeEntries as any);
  const component = new TreeSelectorComponent(
    tree,
    ctx.host.getLeafId(),
    process.stdout.rows ?? 40,
    (entryId: string) => {
      const oldLeafId = ctx.host.getLeafId();
      void ctx.host
        .getDivergentMessages(oldLeafId, entryId)
        .then((skipped) => {
          const summary = `Branching from entry ${entryId.slice(0, 8)}; ${skipped} messages on the abandoned path are preserved in the session tree.`;
          return ctx.host.branchToEntryWithSummary(entryId, summary);
        })
        .then(async () => {
          closeOverlay(ctx);
          await ctx.resync(`Switched branch to ${ctx.host.getLeafId()}`);
        })
        .catch((error: unknown) => {
          closeOverlay(ctx);
          ctx.msg("system", error instanceof Error ? error.message : String(error));
          ctx.render();
        });
    },
    () => closeOverlay(ctx),
  );
  ctx.setActiveOverlay(ctx.showReplacement(component));
}

// ---- Re-exports ----

export type { SessionTreeNode } from "piko-host-runtime";
export { getEntryLabel } from "piko-host-runtime";
