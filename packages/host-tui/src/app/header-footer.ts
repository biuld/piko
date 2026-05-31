import { Text } from "@earendil-works/pi-tui";
import { getContextPercent } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { getContextHints } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import type { BaseApp } from "./base.js";

export interface HeaderFooterDeps extends BaseApp {}

export function doUpdateHeader(app: HeaderFooterDeps): void {
  app.headerBox.clear();
  const t = getTheme();
  app.headerBox.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
  const name = app.sessionName ?? app.host.sessionId.slice(-8);
  const activeTools = app.host.getActiveToolNames();
  const toolsInfo = activeTools?.length ? ` tools:${activeTools.join(",")} ` : "";
  app.headerBox.addChild(
    new Text(
      t.fg(
        "accent",
        ` piko  ${app.currentModel.provider}/${app.currentModel.id}  session ${name}  ${app.transcript.length} msgs${toolsInfo} `,
      ),
      1,
      0,
    ),
  );
  app.headerBox.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
}

export function doUpdateFooter(app: HeaderFooterDeps): void {
  const statuses = [...app.statusLine.getEntries()];
  const hints = app.running
    ? getContextHints("streaming")
    : app.activeOverlay
      ? getContextHints("overlay")
      : getContextHints("normal");
  app.footerComponent.update({
    model: app.currentModel,
    sessionName: app.sessionName,
    messageCount: app.transcript.length,
    cwd: app.host.cwd,
    totalInputTokens: app.cumulativeInput || undefined,
    totalOutputTokens: app.cumulativeOutput || undefined,
    totalCacheRead: app.cumulativeCacheRead || undefined,
    totalCacheWrite: app.cumulativeCacheWrite || undefined,
    totalCost: app.cumulativeCost || undefined,
    contextWindow: (app.currentModel as { contextWindow?: number }).contextWindow,
    contextPercent: (app.currentModel as { contextWindow?: number }).contextWindow
      ? getContextPercent(
          app.cumulativeInput,
          (app.currentModel as { contextWindow?: number }).contextWindow!,
        )
      : undefined,
    extensionStatuses: statuses.length > 0 ? statuses : undefined,
    keyHints: hints || undefined,
  });
}
