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

export async function openModelSelector(
  ctx: OverlayContext,
  models: Array<{
    model: Model<string>;
    providerConfig: import("piko-engine-protocol").EngineProviderConfig;
  }>,
): Promise<ModelSelectResult | undefined> {
  const t = getTheme();
  const borderColor = (s: string) => t.fg("border", s);

  const items: SelectItem[] = models.map((m) => ({
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
    container.addChild(new Text(t.fg("accent", t.bold(" Select Model")), 1, 0));
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
