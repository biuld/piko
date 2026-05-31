/**
 * Model Scope Selector — overlay to select which models are in the cycling scope.
 *
 * Lists all available models with checkboxes. Saves to SettingsManager.enabledModels.
 */

import {
  Container,
  getKeybindings,
  Spacer,
  Text,
} from "@earendil-works/pi-tui";
import { listAvailableModels, type SettingsManager } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

export async function openModelScopeSelector(
  ctx: OverlayContext,
  settingsManager: SettingsManager,
): Promise<void> {
  const t = getTheme();
  const borderColor = (s: string) => t.fg("border", s);

  const allProviders = listAvailableModels();
  const allModels = allProviders.flatMap((p) =>
    p.models.map((m) => ({ provider: p.provider, id: m.id, name: m.name })),
  );

  const enabledSet = new Set(settingsManager.getEnabledModels() ?? []);
  let selectedIndex = 0;

  function toggleCurrent(): void {
    const m = allModels[selectedIndex];
    if (!m) return;
    const key = `${m.provider}/${m.id}`;
    if (enabledSet.has(key)) {
      enabledSet.delete(key);
    } else {
      enabledSet.add(key);
    }
  }

  function saveAndClose(): void {
    const enabled = enabledSet.size > 0 && enabledSet.size < allModels.length
      ? Array.from(enabledSet)
      : undefined;
    settingsManager.setEnabledModels(enabled);
    ctx.getActiveOverlay()?.hide();
    ctx.setActiveOverlay(null);
    ctx.msg("system", `Model scope: ${enabled ? `${enabled.length} models` : "all models"}`);
    ctx.render();
  }

  const overlayComp = new Container();

  function rebuild(): void {
    overlayComp.clear();
    overlayComp.addChild(new DynamicBorder(borderColor));
    overlayComp.addChild(new Text(t.fg("accent", t.bold(" Model Scope")), 1, 0));
    overlayComp.addChild(new Text(
      t.fg("dim", "  Select which models appear in Ctrl+P/N cycling. Empty = all models."),
      1,
      0,
    ));
    overlayComp.addChild(new Spacer(1));

    const maxVisible = Math.min(allModels.length, 15);
    const startIdx = Math.max(0, Math.min(selectedIndex - Math.floor(maxVisible / 2), allModels.length - maxVisible));
    const endIdx = Math.min(startIdx + maxVisible, allModels.length);

    for (let i = startIdx; i < endIdx; i++) {
      const m = allModels[i];
      const key = `${m.provider}/${m.id}`;
      const checked = enabledSet.has(key);
      const prefix = i === selectedIndex ? "→" : " ";
      const check = checked ? "☑" : "☐";
      const label = `${prefix} ${check} ${key}`;
      if (i === selectedIndex) {
        overlayComp.addChild(new Text(t.fg("accent", label), 1, 0));
      } else {
        overlayComp.addChild(new Text(t.fg("dim", label), 1, 0));
      }
    }

    overlayComp.addChild(new Spacer(1));
    const scopeInfo = enabledSet.size === 0
      ? "Scope: all models"
      : `Scope: ${enabledSet.size} of ${allModels.length} models`;
    overlayComp.addChild(new Text(t.fg("dim", scopeInfo), 1, 0));
    overlayComp.addChild(new Spacer(1));
    overlayComp.addChild(new Text(
      `${keyHint("tui.input.submit", "save")}  ${keyHint("tui.select.cancel", "cancel")}  Space: toggle`,
      1,
      0,
    ));
    overlayComp.addChild(new DynamicBorder(borderColor));
  }

  rebuild();

  const component = makeFocusable(overlayComp, null as any);
  Object.assign(component, {
    handleInput(data: string) {
      const kb = getKeybindings();

      if (kb.matches(data, "tui.select.up")) {
        selectedIndex = selectedIndex === 0 ? allModels.length - 1 : selectedIndex - 1;
        rebuild();
        ctx.tui.requestRender();
        return;
      }
      if (kb.matches(data, "tui.select.down")) {
        selectedIndex = selectedIndex === allModels.length - 1 ? 0 : selectedIndex + 1;
        rebuild();
        ctx.tui.requestRender();
        return;
      }
      if (data === " ") {
        toggleCurrent();
        rebuild();
        ctx.tui.requestRender();
        return;
      }
      if (kb.matches(data, "tui.input.submit")) {
        saveAndClose();
        return;
      }
      if (kb.matches(data, "tui.select.cancel")) {
        ctx.getActiveOverlay()?.hide();
        ctx.setActiveOverlay(null);
        return;
      }
    },
  });

  ctx.setActiveOverlay(
    ctx.tui.showOverlay(component, { anchor: "center", width: "60%", maxHeight: "60%" }),
  );
}
