import { Box, Text } from "@earendil-works/pi-tui";
import { getContextPercent } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { getContextHints } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import type { TuiContext } from "./context.js";

export function createHeaderBox(ctx: TuiContext): Box {
  const headerBox = new Box(0, 0);
  ctx.updateHeader = () => {
    headerBox.clear();
    const t = getTheme();
    headerBox.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
    const name = ctx.sessionName ?? ctx.host.sessionId.slice(-8);
    const headerText = ` piko  ${ctx.currentModel.provider}/${ctx.currentModel.id}  session ${name}  ${ctx.transcript.length} msgs `;
    headerBox.addChild(new Text(t.fg("accent", headerText), 1, 0));
    headerBox.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
  };
  ctx.updateHeader();
  return headerBox;
}

export function createUpdateFooter(ctx: TuiContext): () => void {
  return () => {
    const statuses = [...ctx.statusLine.getEntries()];
    const keyHints = ctx.running
      ? getContextHints("streaming")
      : ctx.activeOverlay
        ? getContextHints("overlay")
        : getContextHints("normal");
    ctx.footerComponent.update({
      model: ctx.currentModel,
      sessionName: ctx.sessionName,
      messageCount: ctx.transcript.length,
      cwd: ctx.host.cwd,
      totalInputTokens: ctx.cumulativeInput || undefined,
      totalOutputTokens: ctx.cumulativeOutput || undefined,
      totalCacheRead: ctx.cumulativeCacheRead || undefined,
      totalCacheWrite: ctx.cumulativeCacheWrite || undefined,
      totalCost: ctx.cumulativeCost || undefined,
      contextWindow: (ctx.currentModel as { contextWindow?: number }).contextWindow,
      contextPercent: (ctx.currentModel as { contextWindow?: number }).contextWindow
        ? getContextPercent(ctx.cumulativeInput, (ctx.currentModel as { contextWindow?: number }).contextWindow!)
        : undefined,
      extensionStatuses: statuses.length > 0 ? statuses : undefined,
      keyHints: keyHints || undefined,
    });
  };
}
