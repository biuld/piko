import type { Model } from "@earendil-works/pi-ai";
import { Container, type SelectItem, SelectList, Spacer, Text } from "@earendil-works/pi-tui";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getSelectListTheme, getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

export interface ModelSelectResult {
  model: Model<string>;
  providerConfig: import("piko-engine-protocol").EngineProviderConfig;
}

type ModelSelectorEntry = {
  model: Model<string>;
  providerConfig: import("piko-engine-protocol").EngineProviderConfig;
};

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

export async function openModelSelector(
  ctx: OverlayContext,
  models: ModelSelectorEntry[],
  initialSearch?: string,
): Promise<ModelSelectResult | undefined> {
  const t = getTheme();
  const borderColor = (s: string) => t.fg("border", s);
  const filteredModels = filterModelSelectorEntries(models, initialSearch);
  const visibleModels = filteredModels.length > 0 ? filteredModels : models;
  const trimmedSearch = initialSearch?.trim();

  const items: SelectItem[] = visibleModels.map((m) => ({
    value: `${m.model.provider}/${m.model.id}`,
    label: `${m.model.provider}/${m.model.id}`,
    description: m.model.name,
  }));

  return new Promise<ModelSelectResult | undefined>((resolve) => {
    const selectList = new SelectList(items, Math.min(items.length, 12), getSelectListTheme());
    selectList.onSelect = (item) => {
      overlayHandle?.hide();
      const found = models.find((m) => `${m.model.provider}/${m.model.id}` === item.value);
      resolve(found);
    };
    selectList.onCancel = () => {
      overlayHandle?.hide();
      resolve(undefined);
    };

    const container = new Container();
    container.addChild(new DynamicBorder(borderColor));
    const title =
      trimmedSearch && filteredModels.length > 0
        ? ` Select Model: ${trimmedSearch}`
        : trimmedSearch
          ? ` Select Model: no matches for ${trimmedSearch}`
          : " Select Model";
    container.addChild(new Text(t.fg("accent", t.bold(title)), 1, 0));
    container.addChild(new Spacer(1));
    container.addChild(selectList);
    container.addChild(new Spacer(1));
    container.addChild(
      new Text(
        `${keyHint("tui.select.confirm", "select")}  ${keyHint("tui.select.cancel", "cancel")}  ${keyHint("tui.select.up", "")}${keyHint("tui.select.down", "navigate")}`,
        1,
        0,
      ),
    );
    container.addChild(new DynamicBorder(borderColor));

    const component = makeFocusable(container);
    Object.assign(component, {
      handleInput(data: string) {
        selectList.handleInput(data);
        ctx.tui.requestRender();
      },
    });

    const overlayHandle = ctx.tui.showOverlay(component, {
      anchor: "center",
      width: "60%",
      maxHeight: "60%",
    });
  });
}
