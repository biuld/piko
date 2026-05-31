import type { Model } from "@earendil-works/pi-ai";
import { modelsAreEqual } from "@earendil-works/pi-ai";
import {
  Container,
  type Focusable,
  fuzzyFilter,
  getKeybindings,
  Input,
  Spacer,
  Text,
  type TUI,
} from "@earendil-works/pi-tui";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import type { OverlayContext } from "./index.js";

export interface ModelSelectResult {
  model: Model<string>;
  providerConfig: import("piko-engine-protocol").EngineProviderConfig;
}

export type ModelSelectorEntry = {
  model: Model<string>;
  providerConfig: import("piko-engine-protocol").EngineProviderConfig;
};

type ModelScope = "all" | "scoped";

/**
 * Filter models by search query (non-fuzzy, used as fallback).
 */
export function filterModelSelectorEntries(
  models: ModelSelectorEntry[],
  search?: string,
): ModelSelectorEntry[] {
  const query = search?.trim().toLowerCase();
  if (!query) return models;
  return models.filter((entry) => {
    const provider = entry.model.provider.toLowerCase();
    const id = entry.model.id.toLowerCase();
    const name = entry.model.name.toLowerCase();
    return (
      provider.includes(query) ||
      id.includes(query) ||
      name.includes(query) ||
      `${provider}/${id}`.includes(query)
    );
  });
}

/**
 * Model selector component with interactive search, scope toggle, and rich display.
 * Mirrors pi's ModelSelectorComponent UX.
 */
class ModelSelectorComponent extends Container implements Focusable {
  private searchInput: Input;
  private _focused = false;
  get focused(): boolean {
    return this._focused;
  }
  set focused(value: boolean) {
    this._focused = value;
    this.searchInput.focused = value;
  }

  private allModels: ModelSelectorEntry[] = [];
  private scopedModels: ModelSelectorEntry[] = [];
  private activeModels: ModelSelectorEntry[] = [];
  private filteredModels: ModelSelectorEntry[] = [];
  private selectedIndex = 0;
  private currentModel?: Model<string>;
  private scope: ModelScope = "all";
  private onSelect: (model: ModelSelectorEntry) => void;
  private onCancel: () => void;
  private listContainer: Container;
  private scopeText?: Text;
  private scopeHintText?: Text;
  private tui: TUI;

  constructor(
    tui: TUI,
    allModels: ModelSelectorEntry[],
    scopedModels: ModelSelectorEntry[],
    currentModel: Model<string> | undefined,
    onSelect: (model: ModelSelectorEntry) => void,
    onCancel: () => void,
    initialSearch?: string,
  ) {
    super();
    this.tui = tui;

    this.allModels = [...allModels];
    this.scopedModels = scopedModels.length > 0 ? [...scopedModels] : [...allModels];
    const hasDistinctScoped = scopedModels.length > 0 && scopedModels.length !== allModels.length;
    this.scope = hasDistinctScoped ? "scoped" : "all";
    this.activeModels = this.scope === "scoped" ? this.scopedModels : this.allModels;
    this.filteredModels = [...this.activeModels];
    this.currentModel = currentModel;
    this.onSelect = onSelect;
    this.onCancel = onCancel;

    // Sort: current model first, then by provider
    this.sortModels();

    // Set initial selected index to current model
    if (currentModel) {
      const idx = this.filteredModels.findIndex((e) => modelsAreEqual(currentModel, e.model));
      this.selectedIndex = idx >= 0 ? idx : 0;
    }

    // ---- Build UI ----
    this.addChild(new DynamicBorder());
    this.addChild(new Spacer(1));

    // Scope info (only shown when scoped models differ from all)
    if (hasDistinctScoped) {
      this.scopeText = new Text(this.getScopeText(), 0, 0);
      this.addChild(this.scopeText);
      this.scopeHintText = new Text(this.getScopeHintText(), 0, 0);
      this.addChild(this.scopeHintText);
      this.addChild(new Spacer(1));
    }

    // Search input
    this.searchInput = new Input();
    if (initialSearch) {
      this.searchInput.setValue(initialSearch);
    }
    this.searchInput.onSubmit = () => {
      if (this.filteredModels[this.selectedIndex]) {
        this.handleSelect(this.filteredModels[this.selectedIndex]);
      }
    };
    this.addChild(this.searchInput);
    this.addChild(new Spacer(1));

    // List container
    this.listContainer = new Container();
    this.addChild(this.listContainer);
    this.addChild(new Spacer(1));

    // Footer with key hints
    const tabKey = hasDistinctScoped ? `  ${keyHint("tui.input.tab", "scope")}` : "";
    this.addChild(
      new Text(
        `${keyHint("tui.select.confirm", "select")}  ${keyHint("tui.select.cancel", "cancel")}  ${keyHint("tui.select.up", "")}${keyHint("tui.select.down", "navigate")}${tabKey}`,
        1,
        0,
      ),
    );
    this.addChild(new DynamicBorder());

    // Initial render
    if (initialSearch) {
      this.filterModels(initialSearch);
    } else {
      this.updateList();
    }
  }

  private sortModels(): void {
    const sortFn = (a: ModelSelectorEntry, b: ModelSelectorEntry) => {
      const aIsCurrent = modelsAreEqual(this.currentModel, a.model);
      const bIsCurrent = modelsAreEqual(this.currentModel, b.model);
      if (aIsCurrent && !bIsCurrent) return -1;
      if (!aIsCurrent && bIsCurrent) return 1;
      return a.model.provider.localeCompare(b.model.provider);
    };
    this.allModels.sort(sortFn);
    this.scopedModels.sort(sortFn);
    this.activeModels.sort(sortFn);
    this.filteredModels.sort(sortFn);
  }

  private getScopeText(): string {
    const t = getTheme();
    const allText = this.scope === "all" ? t.fg("accent", "all") : t.fg("muted", "all");
    const scopedText = this.scope === "scoped" ? t.fg("accent", "scoped") : t.fg("muted", "scoped");
    return `${t.fg("muted", "Scope: ")}${allText}${t.fg("muted", " | ")}${scopedText}`;
  }

  private getScopeHintText(): string {
    return keyHint("tui.input.tab", "scope") + getTheme().fg("muted", " (all/scoped)");
  }

  private setScope(scope: ModelScope): void {
    if (this.scope === scope) return;
    this.scope = scope;
    this.activeModels = scope === "scoped" ? this.scopedModels : this.allModels;
    // Try to keep selection on same model after scope change
    const currentEntry =
      this.selectedIndex < this.filteredModels.length
        ? this.filteredModels[this.selectedIndex]
        : undefined;
    const newIdx = currentEntry
      ? this.activeModels.findIndex((e) => modelsAreEqual(currentEntry.model, e.model))
      : -1;
    this.selectedIndex = Math.max(0, newIdx >= 0 ? newIdx : 0);
    this.filterModels(this.searchInput.getValue());
    if (this.scopeText) {
      this.scopeText.setText(this.getScopeText());
    }
  }

  private filterModels(query: string): void {
    this.filteredModels = query
      ? fuzzyFilter(this.activeModels, query, ({ model }) => {
          return `${model.id} ${model.provider} ${model.provider}/${model.id} ${
            model.provider
          } ${model.id}`;
        })
      : [...this.activeModels];
    this.selectedIndex = Math.min(this.selectedIndex, Math.max(0, this.filteredModels.length - 1));
    this.updateList();
  }

  private updateList(): void {
    const t = getTheme();
    this.listContainer.clear();

    const maxVisible = 10;
    const startIndex = Math.max(
      0,
      Math.min(
        this.selectedIndex - Math.floor(maxVisible / 2),
        this.filteredModels.length - maxVisible,
      ),
    );
    const endIndex = Math.min(startIndex + maxVisible, this.filteredModels.length);

    // Show visible slice of filtered models
    for (let i = startIndex; i < endIndex; i++) {
      const entry = this.filteredModels[i];
      if (!entry) continue;

      const isSelected = i === this.selectedIndex;
      const isCurrent = modelsAreEqual(this.currentModel, entry.model);

      let line: string;
      if (isSelected) {
        const prefix = t.fg("accent", "→ ");
        const modelText = `${entry.model.id}`;
        const providerBadge = t.fg("muted", `[${entry.model.provider}]`);
        const checkmark = isCurrent ? t.fg("success", " ✓") : "";
        line = `${prefix + t.fg("accent", modelText)} ${providerBadge}${checkmark}`;
      } else {
        const modelText = `  ${entry.model.id}`;
        const providerBadge = t.fg("muted", `[${entry.model.provider}]`);
        const checkmark = isCurrent ? t.fg("success", " ✓") : "";
        line = `${modelText} ${providerBadge}${checkmark}`;
      }
      this.listContainer.addChild(new Text(line, 0, 0));
    }

    // Scroll position indicator
    if (
      this.filteredModels.length > maxVisible &&
      (startIndex > 0 || endIndex < this.filteredModels.length)
    ) {
      const scrollInfo = t.fg(
        "muted",
        `  (${this.selectedIndex + 1}/${this.filteredModels.length})`,
      );
      this.listContainer.addChild(new Text(scrollInfo, 0, 0));
    }

    // Show "no results" or model name detail
    if (this.filteredModels.length === 0) {
      this.listContainer.addChild(new Text(t.fg("muted", "  No matching models"), 0, 0));
    } else {
      const selected = this.filteredModels[this.selectedIndex];
      if (selected) {
        this.listContainer.addChild(new Spacer(1));
        this.listContainer.addChild(
          new Text(t.fg("muted", `  Model Name: ${selected.model.name}`), 0, 0),
        );
      }
    }
  }

  handleInput(keyData: string): void {
    const kb = getKeybindings();

    // Tab — toggle scope
    if (
      kb.matches(keyData, "tui.input.tab") &&
      this.scopedModels.length !== this.allModels.length
    ) {
      const nextScope: ModelScope = this.scope === "all" ? "scoped" : "all";
      this.setScope(nextScope);
      if (this.scopeHintText) {
        this.scopeHintText.setText(this.getScopeHintText());
      }
      this.tui.requestRender();
      return;
    }

    // Up arrow — wrap to bottom when at top
    if (kb.matches(keyData, "tui.select.up")) {
      if (this.filteredModels.length === 0) return;
      this.selectedIndex =
        this.selectedIndex === 0 ? this.filteredModels.length - 1 : this.selectedIndex - 1;
      this.updateList();
      this.tui.requestRender();
      return;
    }

    // Down arrow — wrap to top when at bottom
    if (kb.matches(keyData, "tui.select.down")) {
      if (this.filteredModels.length === 0) return;
      this.selectedIndex =
        this.selectedIndex === this.filteredModels.length - 1 ? 0 : this.selectedIndex + 1;
      this.updateList();
      this.tui.requestRender();
      return;
    }

    // Enter — confirm selection
    if (kb.matches(keyData, "tui.select.confirm")) {
      const selected = this.filteredModels[this.selectedIndex];
      if (selected) {
        this.handleSelect(selected);
      }
      return;
    }

    // Escape or Ctrl+C — cancel
    if (kb.matches(keyData, "tui.select.cancel")) {
      this.onCancel();
      return;
    }

    // Pass everything else to search input
    this.searchInput.handleInput(keyData);
    this.filterModels(this.searchInput.getValue());
    this.tui.requestRender();
  }

  private handleSelect(entry: ModelSelectorEntry): void {
    this.onSelect(entry);
  }
}

/**
 * Open the interactive model selector overlay.
 *
 * @param ctx - Overlay context
 * @param allModels - All available model entries
 * @param scopedModels - Scoped model entries (subset shown in "scoped" view)
 * @param currentModel - Currently active model (shown with ✓)
 * @param initialSearch - Optional initial search query
 * @returns Selected model entry, or undefined if cancelled
 */
export async function openModelSelector(
  ctx: OverlayContext,
  allModels: ModelSelectorEntry[],
  scopedModels: ModelSelectorEntry[],
  currentModel: Model<string> | undefined,
  initialSearch?: string,
): Promise<ModelSelectResult | undefined> {
  return new Promise<ModelSelectResult | undefined>((resolve) => {
    const selector = new ModelSelectorComponent(
      ctx.tui,
      allModels,
      scopedModels,
      currentModel,
      (entry) => {
        replacementHandle?.hide();
        resolve(entry);
      },
      () => {
        replacementHandle?.hide();
        resolve(undefined);
      },
      initialSearch,
    );

    // ModelSelectorComponent already implements Focusable + handleInput
    const replacementHandle = ctx.showReplacement(selector as any);
  });
}
