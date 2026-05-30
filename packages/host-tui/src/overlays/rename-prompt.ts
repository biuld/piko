import { Container, Input, Spacer, Text } from "@earendil-works/pi-tui";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

export async function openRenamePrompt(
  ctx: OverlayContext,
  sessionPath: string,
  currentName: string,
): Promise<string | undefined> {
  const t = getTheme();
  const borderColor = (s: string) => t.fg("border", s);

  return new Promise<string | undefined>((resolve) => {
    const input = new Input();
    input.setValue(currentName);
    input.onSubmit = (value) => {
      void ctx.host.renameSession(sessionPath, value).then(() => {
        resolve(value);
      });
    };
    input.onEscape = () => {
      resolve(undefined);
    };

    const container = new Container();
    container.addChild(new DynamicBorder(borderColor));
    container.addChild(new Text(t.fg("accent", t.bold(" Rename Session")), 1, 0));
    container.addChild(new Spacer(1));
    container.addChild(input);
    container.addChild(new Spacer(1));
    container.addChild(
      new Text(
        `${keyHint("tui.input.submit", "save")}  ${keyHint("tui.select.cancel", "cancel")}`,
        1,
        0,
      ),
    );
    container.addChild(new DynamicBorder(borderColor));

    const component = makeFocusable(container, input);
    Object.assign(component, {
      handleInput(data: string) {
        input.handleInput(data);
      },
    });

    ctx.tui.showOverlay(component, { anchor: "center", width: "70%", maxHeight: "30%" });
  });
}
