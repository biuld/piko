/**
 * Thinking level selector overlay.
 */

import { Container, type SelectItem, SelectList, Spacer, Text } from "@earendil-works/pi-tui";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getSelectListTheme, getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

const LEVELS: Array<{ value: string; label: string; description: string }> = [
  { value: "off", label: "off", description: "No thinking" },
  { value: "minimal", label: "minimal", description: "Minimal reasoning" },
  { value: "low", label: "low", description: "Low reasoning" },
  { value: "medium", label: "medium", description: "Medium reasoning" },
  { value: "high", label: "high", description: "High reasoning" },
  { value: "xhigh", label: "xhigh", description: "Maximum reasoning" },
];

export async function openThinkingSelector(
  ctx: OverlayContext,
  currentLevel: string,
): Promise<string | undefined> {
  const t = getTheme();
  const borderColor = (s: string) => t.fg("border", s);

  const items: SelectItem[] = LEVELS.map((l) => ({
    value: l.value,
    label: l.value === currentLevel ? t.fg("accent", `• ${l.label}`) : `  ${l.label}`,
    description: l.description,
  }));

  return new Promise<string | undefined>((resolve) => {
    const selectList = new SelectList(items, items.length, getSelectListTheme());

    let replacementHandle: { hide(): void } | undefined;
    selectList.onSelect = (item) => {
      replacementHandle?.hide();
      resolve(item.value);
    };
    selectList.onCancel = () => {
      replacementHandle?.hide();
      resolve(undefined);
    };

    const container = new Container();
    container.addChild(new DynamicBorder(borderColor));
    container.addChild(new Text(t.fg("accent", t.bold(" Thinking Level")), 1, 0));
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

    replacementHandle = ctx.showReplacement(component);
  });
}
