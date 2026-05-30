import {
  Container,
  getKeybindings,
  Input,
  type SelectItem,
  SelectList,
  Spacer,
  Text,
} from "@earendil-works/pi-tui";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint, rawKeyHint } from "../components/key-hints.js";
import { createThreadedSessionSelectItems } from "../session-tree.js";
import { getSelectListTheme, getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";
import { openRenamePrompt } from "./rename-prompt.js";

function closeOverlay(ctx: OverlayContext): void {
  ctx.getActiveOverlay()?.hide();
  ctx.setActiveOverlay(null);
}

export async function openResumeSelector(ctx: OverlayContext): Promise<void> {
  const t = getTheme();
  const borderColor = (s: string) => t.fg("border", s);
  let scope: "current" | "all" = "current";
  let namedOnly = false;

  async function loadSessions() {
    return ctx.host.listSessions({ scope, namedOnly });
  }

  let sessions = await loadSessions();
  if (sessions.length === 0) {
    const allSessions = await ctx.host.listSessions({ scope: "all" });
    if (allSessions.length === 0 && !namedOnly) {
      ctx.msg("system", "No saved sessions. /resume <id> to load");
      ctx.render();
      return;
    }
    scope = "all";
    sessions = await loadSessions();
    if (sessions.length === 0) {
      ctx.msg(
        "system",
        namedOnly ? "No named sessions found" : "No saved sessions. /resume <id> to load",
      );
      ctx.render();
      return;
    }
  }

  let items = createThreadedSessionSelectItems(sessions);
  const searchInput = new Input();
  let selectList = new SelectList(items, Math.min(items.length, 12), getSelectListTheme());

  function rebuild(title: string, footerLines: string[]) {
    overlayComp.clear();
    overlayComp.addChild(new DynamicBorder(borderColor));
    overlayComp.addChild(new Text(t.fg("accent", t.bold(` ${title}`)), 1, 0));
    overlayComp.addChild(new Spacer(1));
    overlayComp.addChild(searchInput);
    overlayComp.addChild(new Spacer(1));
    overlayComp.addChild(selectList);
    overlayComp.addChild(new Spacer(1));
    for (const line of footerLines) {
      overlayComp.addChild(new Text(line, 1, 0));
    }
    overlayComp.addChild(new DynamicBorder(borderColor));
  }

  function applyFilter() {
    const query = searchInput.getValue().trim().toLowerCase();
    const filtered =
      query.length === 0
        ? items
        : items.filter((item) =>
            [item.label, item.description, item.value]
              .filter(Boolean)
              .join(" ")
              .toLowerCase()
              .includes(query),
          );
    selectList = new SelectList(filtered, Math.min(filtered.length, 12), getSelectListTheme());
    selectList.onSelect = onSelect;
    selectList.onCancel = onCancel;
    rebuild(scope === "current" ? "Resume Session (Current)" : "Resume Session (All)", [
      `${keyHint("tui.input.submit", "resume")}  ${rawKeyHint("Tab", "scope")}  ${rawKeyHint("Ctrl+N", "named")}  ${rawKeyHint("Ctrl+R", "rename")}  ${rawKeyHint("Ctrl+D", "delete")}  ${keyHint("tui.select.cancel", "cancel")}`,
      t.fg(
        "dim",
        `Scope: ${scope === "current" ? "current" : "all"}  Name: ${namedOnly ? "named" : "all"}`,
      ),
    ]);
  }

  function onSelect(item: SelectItem) {
    void ctx.host.switchSession(item.value).then((resolved) => {
      closeOverlay(ctx);
      if (!resolved) {
        ctx.msg("system", `Session ${item.label} not found`);
        ctx.render();
        return;
      }
      void ctx.doResume();
    });
  }

  function onCancel() {
    closeOverlay(ctx);
  }

  selectList.onSelect = onSelect;
  selectList.onCancel = onCancel;

  const overlayComp = new Container();
  rebuild(scope === "current" ? "Resume Session (Current)" : "Resume Session (All)", [
    `${keyHint("tui.input.submit", "resume")}  ${rawKeyHint("Tab", "scope")}  ${rawKeyHint("Ctrl+N", "named")}  ${rawKeyHint("Ctrl+R", "rename")}  ${rawKeyHint("Ctrl+D", "delete")}  ${keyHint("tui.select.cancel", "cancel")}`,
    t.fg(
      "dim",
      `Scope: ${scope === "current" ? "current" : "all"}  Name: ${namedOnly ? "named" : "all"}`,
    ),
  ]);

  const component = makeFocusable(overlayComp, searchInput);
  Object.assign(component, {
    handleInput(data: string) {
      const kb = getKeybindings();

      if (data === "\u0012") {
        const selected = selectList.getSelectedItem();
        if (!selected) return;
        const currentName = sessions.find((s) => s.path === selected.value)?.name ?? "";
        openRenamePrompt(ctx, selected.value, currentName).then(async (newName) => {
          if (newName !== undefined) {
            sessions = await loadSessions();
            items = createThreadedSessionSelectItems(sessions);
            applyFilter();
            ctx.tui.hideOverlay();
            ctx.render();
          }
        });
        return;
      }
      if (data === "\u0004") {
        const selectedValue = selectList.getSelectedItem()?.value;
        if (!selectedValue) return;
        if (selectedValue === ctx.host.sessionFile) {
          ctx.msg("system", "Cannot delete the current active session");
          ctx.render();
          return;
        }
        void ctx.host.deleteSession(selectedValue).then(async () => {
          sessions = await loadSessions();
          items = createThreadedSessionSelectItems(sessions);
          applyFilter();
          ctx.render();
        });
        return;
      }
      if (kb.matches(data, "tui.input.tab")) {
        scope = scope === "current" ? "all" : "current";
        void loadSessions().then((s) => {
          sessions = s;
          items = createThreadedSessionSelectItems(sessions);
          applyFilter();
          ctx.render();
        });
        return;
      }
      const toggleNamedKey = "app.session.toggleNamedFilter" as Parameters<typeof kb.matches>[1];
      if (kb.matches(data, toggleNamedKey)) {
        namedOnly = !namedOnly;
        void loadSessions().then((s) => {
          sessions = s;
          items = createThreadedSessionSelectItems(sessions);
          applyFilter();
          ctx.render();
        });
        return;
      }

      if (
        kb.matches(data, "tui.select.up") ||
        kb.matches(data, "tui.select.down") ||
        kb.matches(data, "tui.select.confirm") ||
        kb.matches(data, "tui.select.cancel")
      ) {
        selectList.handleInput(data);
        return;
      }

      searchInput.handleInput(data);
      applyFilter();
      ctx.render();
    },
  });

  ctx.setActiveOverlay(
    ctx.tui.showOverlay(component, { anchor: "center", width: "80%", maxHeight: "60%" }),
  );
}
