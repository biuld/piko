import { Container, type SelectItem, SelectList, Spacer, Text } from "@earendil-works/pi-tui";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getSelectListTheme, getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

function closeOverlay(ctx: OverlayContext): void {
  ctx.getActiveOverlay()?.hide();
  ctx.setActiveOverlay(null);
}

export async function openForkSelector(ctx: OverlayContext): Promise<void> {
  const t = getTheme();
  const borderColor = (s: string) => t.fg("border", s);

  if (!ctx.host.isSessionPersisted()) {
    ctx.msg("system", "Fork requires a saved session");
    ctx.render();
    return;
  }

  const branch = await ctx.host.getBranchEntries();
  const items: SelectItem[] = branch
    .filter(
      (entry): entry is Extract<(typeof branch)[number], { type: "message" }> =>
        entry.type === "message",
    )
    .filter((entry) => entry.message.role === "user")
    .map((entry) => {
      const msg = (entry as any).message;
      const desc =
        entry.type === "message"
          ? typeof msg.content === "string"
            ? msg.content.slice(0, 120)
            : Array.isArray(msg.content)
              ? msg.content
                  .filter((b: any) => b.type === "text")
                  .map((b: any) => b.text ?? "")
                  .join(" ")
                  .slice(0, 120)
              : ""
          : "";
      return { value: entry.id, label: entry.id, description: desc };
    })
    .reverse();

  if (items.length === 0) {
    ctx.msg("system", "Current branch has no user messages to fork from");
    ctx.render();
    return;
  }

  const selectList = new SelectList(items, Math.min(items.length, 12), getSelectListTheme());

  selectList.onSelect = (item: SelectItem) => {
    void ctx
      .doFork(item.value)
      .then(() => closeOverlay(ctx))
      .catch((error: unknown) => {
        closeOverlay(ctx);
        ctx.msg("system", error instanceof Error ? error.message : String(error));
        ctx.render();
      });
  };
  selectList.onCancel = () => closeOverlay(ctx);

  const container = new Container();
  container.addChild(new DynamicBorder(borderColor));
  container.addChild(new Text(t.fg("accent", t.bold(" Fork From User Message")), 1, 0));
  container.addChild(new Spacer(1));
  container.addChild(selectList);
  container.addChild(new Spacer(1));
  container.addChild(
    new Text(
      `${keyHint("tui.select.confirm", "fork")}  ${keyHint("tui.select.cancel", "cancel")}  ${keyHint("tui.select.up", "")}${keyHint("tui.select.down", "select")}`,
      1,
      0,
    ),
  );
  container.addChild(new DynamicBorder(borderColor));

  const component = makeFocusable(container);
  Object.assign(component, {
    handleInput(data: string) {
      selectList.handleInput(data);
    },
  });

  ctx.setActiveOverlay(
    ctx.tui.showOverlay(component, { anchor: "center", width: "80%", maxHeight: "60%" }),
  );
}
