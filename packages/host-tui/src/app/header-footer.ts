import { Text } from "@earendil-works/pi-tui";
import { getContextPercent } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { getContextHints } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import type { AppConstructor, BaseApp } from "./base.js";

export function HeaderFooterMixin<TBase extends AppConstructor<BaseApp>>(Base: TBase) {
  return class extends Base {
    updateHeader(): void {
      this.headerBox.clear();
      const t = getTheme();
      this.headerBox.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
      const name = this.sessionName ?? this.host.sessionId.slice(-8);
      this.headerBox.addChild(new Text(t.fg("accent", ` piko  ${this.currentModel.provider}/${this.currentModel.id}  session ${name}  ${this.transcript.length} msgs `), 1, 0));
      this.headerBox.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
    }

    updateFooter(): void {
      const statuses = [...this.statusLine.getEntries()];
      const hints = this.running ? getContextHints("streaming") : this.activeOverlay ? getContextHints("overlay") : getContextHints("normal");
      this.footerComponent.update({
        model: this.currentModel, sessionName: this.sessionName, messageCount: this.transcript.length, cwd: this.host.cwd,
        totalInputTokens: this.cumulativeInput || undefined,
        totalOutputTokens: this.cumulativeOutput || undefined,
        totalCacheRead: this.cumulativeCacheRead || undefined,
        totalCacheWrite: this.cumulativeCacheWrite || undefined,
        totalCost: this.cumulativeCost || undefined,
        contextWindow: (this.currentModel as { contextWindow?: number }).contextWindow,
        contextPercent: (this.currentModel as { contextWindow?: number }).contextWindow
          ? getContextPercent(this.cumulativeInput, (this.currentModel as { contextWindow?: number }).contextWindow!) : undefined,
        extensionStatuses: statuses.length > 0 ? statuses : undefined,
        keyHints: hints || undefined,
      });
    }
  };
}
