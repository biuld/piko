/**
 * Model Scope Selector — overlay to select which models appear in Ctrl+P/N cycling.
 *
 * Features:
 * - Interactive fuzzy search via Input component
 * - Space to toggle individual models
 * - Ctrl+A to enable all, Ctrl+X to clear all
 * - Up/down wrap-around navigation
 * - Rich display with checkboxes, provider grouping
 */

import {
  Container,
  fuzzyFilter,
  getKeybindings,
  Input,
  matchesKey,
  Spacer,
  Text,
} from "@earendil-works/pi-tui";
import { listAvailableModels, type SettingsManager } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint, rawKeyHint } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

interface ModelEntry {
  provider: string;
  id: string;
  name: string;
}

export async function openModelScopeSelector(
  ctx: OverlayContext,
  settingsManager: SettingsManager,
): Promise<void> {
  const t = getTheme();
  const borderColor = (s: string) => t.fg("border", s);

  const allProviders = listAvailableModels();
  const allModels: ModelEntry[] = allProviders.flatMap((p) =>
    p.models.map((m) => ({ provider: p.provider, id: m.id, name: m.name })),
  );

  const enabledSet = new Set<string>(settingsManager.getEnabledModels() ?? []);
  let filteredModels: ModelEntry[] = [...allModels];
  let selectedIndex = 0;
  let searchQuery = "";

  const searchInput = new Input();

  function applyFilter(): void {
    const query = searchInput.getValue().trim();
    searchQuery = query;
    if (query) {
      filteredModels = fuzzyFilter(allModels, query, (m) => {
        return `${m.provider}/${m.id} ${m.provider} ${m.id} ${m.name}`;
      });
    } else {
      filteredModels = [...allModels];
    }
    selectedIndex = Math.min(selectedIndex, Math.max(0, filteredModels.length - 1));
    rebuild();
  }

  function toggleCurrent(): void {
    const m = filteredModels[selectedIndex];
    if (!m) return;
    const key = `${m.provider}/${m.id}`;
    if (enabledSet.has(key)) {
      enabledSet.delete(key);
    } else {
      enabledSet.add(key);
    }
    rebuild();
  }

  function enableAll(): void {
    for (const m of filteredModels) {
      enabledSet.add(`${m.provider}/${m.id}`);
    }
    rebuild();
  }

  function clearAll(): void {
    for (const m of filteredModels) {
      enabledSet.delete(`${m.provider}/${m.id}`);
    }
    rebuild();
  }

  function saveAndClose(): void {
    const enabled =
      enabledSet.size > 0 && enabledSet.size < allModels.length
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

    // Header
    const totalCount = allModels.length;
    const filteredCount = filteredModels.length;
    const enabledCount = enabledSet.size;
    const headerLine =
      searchQuery && filteredCount !== totalCount
        ? ` Model Scope (${enabledCount}/${filteredCount} of ${totalCount}, search: "${searchQuery}")`
        : ` Model Scope (${enabledCount}/${totalCount} enabled)`;
    overlayComp.addChild(new DynamicBorder(borderColor));
    overlayComp.addChild(new Text(t.fg("accent", t.bold(headerLine)), 1, 0));
    overlayComp.addChild(
      new Text(
        t.fg("dim", "  Select which models appear in Ctrl+P/N cycling. Empty = all models."),
        1,
        0,
      ),
    );
    overlayComp.addChild(new Spacer(1));

    // Search input
    overlayComp.addChild(searchInput);
    overlayComp.addChild(new Spacer(1));

    // Model list
    const maxVisible = Math.min(filteredModels.length, 12);
    const startIdx = Math.max(
      0,
      Math.min(selectedIndex - Math.floor(maxVisible / 2), filteredModels.length - maxVisible),
    );
    const endIdx = Math.min(startIdx + maxVisible, filteredModels.length);

    if (filteredModels.length === 0) {
      overlayComp.addChild(new Text(t.fg("muted", "  No matching models"), 1, 0));
    } else {
      for (let i = startIdx; i < endIdx; i++) {
        const m = filteredModels[i];
        const key = `${m.provider}/${m.id}`;
        const checked = enabledSet.has(key);
        const isSelected = i === selectedIndex;
        const prefix = isSelected ? "→" : " ";
        const check = checked ? "☑" : "☐";
        const label = `${prefix} ${check} ${key}`;
        const fullLine = isSelected ? t.fg("accent", label) : checked ? label : t.fg("dim", label);
        overlayComp.addChild(new Text(fullLine, 1, 0));
      }

      // Scroll position
      if (filteredModels.length > maxVisible) {
        overlayComp.addChild(new Spacer(1));
        overlayComp.addChild(
          new Text(t.fg("dim", `  (${selectedIndex + 1}/${filteredModels.length})`), 1, 0),
        );
      }
    }

    // Footer
    overlayComp.addChild(new Spacer(1));
    overlayComp.addChild(
      new Text(
        t.fg(
          "dim",
          searchQuery
            ? `Scope: ${enabledSet.size === 0 ? "all" : `${enabledSet.size} of ${totalCount}`} models  Filter: "${searchQuery}"`
            : `Scope: ${enabledSet.size === 0 ? "all" : `${enabledSet.size} of ${totalCount}`} models`,
        ),
        1,
        0,
      ),
    );
    overlayComp.addChild(new Spacer(1));
    overlayComp.addChild(
      new Text(
        `${keyHint("tui.input.submit", "save")}  ${keyHint("tui.select.cancel", "cancel")}  Space: toggle  ${rawKeyHint("Ctrl+A", "enable all")}  ${rawKeyHint("Ctrl+X", "clear all")}`,
        1,
        0,
      ),
    );
    overlayComp.addChild(new DynamicBorder(borderColor));
  }

  rebuild();

  const component = makeFocusable(overlayComp, null as any);
  Object.assign(component, {
    handleInput(data: string) {
      const kb = getKeybindings();

      // Up/down navigation
      if (kb.matches(data, "tui.select.up")) {
        if (filteredModels.length === 0) return;
        selectedIndex = selectedIndex === 0 ? filteredModels.length - 1 : selectedIndex - 1;
        rebuild();
        ctx.tui.requestRender();
        return;
      }
      if (kb.matches(data, "tui.select.down")) {
        if (filteredModels.length === 0) return;
        selectedIndex = selectedIndex === filteredModels.length - 1 ? 0 : selectedIndex + 1;
        rebuild();
        ctx.tui.requestRender();
        return;
      }

      // Space to toggle
      if (data === " ") {
        toggleCurrent();
        ctx.tui.requestRender();
        return;
      }

      // Ctrl+A to enable all visible
      if (matchesKey(data, "ctrl+a")) {
        enableAll();
        ctx.tui.requestRender();
        return;
      }

      // Ctrl+X to clear all visible
      if (matchesKey(data, "ctrl+x")) {
        clearAll();
        ctx.tui.requestRender();
        return;
      }

      // Submit
      if (kb.matches(data, "tui.input.submit")) {
        saveAndClose();
        return;
      }

      // Cancel
      if (kb.matches(data, "tui.select.cancel")) {
        ctx.getActiveOverlay()?.hide();
        ctx.setActiveOverlay(null);
        return;
      }

      // Everything else goes to search input
      searchInput.handleInput(data);
      applyFilter();
      ctx.tui.requestRender();
    },
  });

  // Focus on the overlay component itself, so our handleInput routes keys.
  // The overlay internally delegates text input to searchInput.
  ctx.setActiveOverlay(ctx.showReplacement(component));
}
