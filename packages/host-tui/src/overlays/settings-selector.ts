/**
 * Settings selector overlay — displays and toggles boolean settings.
 */

import { Container, type SelectItem, SelectList, Spacer, Text } from "@earendil-works/pi-tui";
import type { SettingsManager } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getSelectListTheme, getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

export async function openSettingsSelector(
  ctx: OverlayContext,
  settingsManager: SettingsManager,
): Promise<void> {
  const t = getTheme();

  function buildItems(): SelectItem[] {
    const s = settingsManager.settings;
    const items: SelectItem[] = [];

    items.push({
      value: "compaction",
      label: "Compaction",
      description: (s.compaction?.enabled ?? true) ? t.fg("success", "on") : t.fg("muted", "off"),
    });
    items.push({
      value: "retry",
      label: "Retry on error",
      description: (s.retry?.enabled ?? true) ? t.fg("success", "on") : t.fg("muted", "off"),
    });
    items.push({
      value: "thinking",
      label: "Default thinking level",
      description: t.fg("muted", s.defaultThinkingLevel ?? "off"),
    });
    items.push({
      value: "theme",
      label: "Theme",
      description: t.fg("muted", s.theme ?? "dark"),
    });
    items.push({
      value: "sessiondir",
      label: "Session directory",
      description: t.fg("muted", s.sessionDir ?? "default"),
    });

    return items;
  }

  return new Promise<void>((resolve) => {
    const selectList = new SelectList(buildItems(), 8, getSelectListTheme());

    let overlayHandle: { hide(): void } | undefined;
    selectList.onSelect = (item) => {
      switch (item.value) {
        case "compaction": {
          const current = settingsManager.getCompactionSettings().enabled;
          settingsManager.setCompactionEnabled(!current);
          break;
        }
        case "retry": {
          const current = settingsManager.getRetrySettings().enabled;
          settingsManager.setRetryEnabled(!current);
          break;
        }
        case "thinking": {
          const levels: Array<"off" | "minimal" | "low" | "medium" | "high" | "xhigh"> = [
            "off", "minimal", "low", "medium", "high", "xhigh",
          ];
          const current = settingsManager.getDefaultThinkingLevel() ?? "off";
          const idx = levels.indexOf(current);
          const next = levels[(idx + 1) % levels.length];
          settingsManager.setDefaultThinkingLevel(next);
          break;
        }
        case "theme": {
          const current = settingsManager.getTheme() ?? "dark";
          settingsManager.setTheme(current === "dark" ? "light" : "dark");
          break;
        }
      }
      // Rebuild and refresh
      ctx.render();
    };
    selectList.onCancel = () => {
      overlayHandle?.hide();
      resolve();
    };

    const container = new Container();
    container.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
    container.addChild(new Text(t.fg("accent", t.bold(" Settings")), 1, 0));
    container.addChild(new Spacer(1));
    container.addChild(selectList);
    container.addChild(new Spacer(1));
    container.addChild(
      new Text(
        `${keyHint("tui.select.confirm", "toggle")}  ${keyHint("tui.select.cancel", "close")}`,
        1,
        0,
      ),
    );
    container.addChild(new DynamicBorder((s: string) => t.fg("border", s)));

    const component = makeFocusable(container);
    Object.assign(component, {
      handleInput(data: string) {
        selectList.handleInput(data);
        ctx.tui.requestRender();
      },
    });

    overlayHandle = ctx.tui.showOverlay(component, {
      anchor: "center",
      width: "50%",
      maxHeight: "50%",
    });
  });
}
