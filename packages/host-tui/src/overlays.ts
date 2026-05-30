import { getKeybindings, type TUI } from "@earendil-works/pi-tui";
import type { PikoHost } from "piko-host-runtime";
import { buildSessionTree, TreeSelectorComponent } from "./components/tree-selector.js";
import { PromptOverlay } from "./prompt-overlay.js";
import { SelectorOverlay } from "./selector-overlay.js";
import { createThreadedSessionSelectItems } from "./session-tree.js";

export interface OverlayContext {
  tui: TUI;
  host: PikoHost;
  msg: (role: string, text: string) => void;
  render: () => void;
  resync: (msg?: string) => Promise<void>;
  doResume: () => Promise<void>;
  doFork: (entryId: string) => Promise<void>;
  setEditorText: (text: string) => void;
  getActiveOverlay(): { hide(): void } | null;
  setActiveOverlay(o: { hide(): void } | null): void;
}

function closeOverlay(ctx: OverlayContext): void {
  ctx.getActiveOverlay()?.hide();
  ctx.setActiveOverlay(null);
}

export async function openResumeSelector(ctx: OverlayContext): Promise<void> {
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

  const updateOverlayState = (overlay: SelectorOverlay): void => {
    overlay.setTitle(scope === "current" ? "Resume Session (Current)" : "Resume Session (All)");
    overlay.setItems(createThreadedSessionSelectItems(sessions));
    overlay.setFooterLines([
      "Enter resume  Tab scope  Ctrl+N named-only  Ctrl+R rename  Ctrl+D delete  Esc cancel",
      `Scope: ${scope === "current" ? "current" : "all"}  Name: ${namedOnly ? "named" : "all"}`,
    ]);
  };

  const overlay = new SelectorOverlay(
    "",
    createThreadedSessionSelectItems(sessions),
    "",
    (item) => {
      void ctx.host.switchSession(item.value).then((resolved) => {
        closeOverlay(ctx);
        if (!resolved) {
          ctx.msg("system", `Session ${item.label} not found`);
          ctx.render();
          return;
        }
        void ctx.doResume();
      });
    },
    () => closeOverlay(ctx),
    (data) => {
      const kb = getKeybindings();
      const toggleNamedFilterKey = "app.session.toggleNamedFilter" as Parameters<
        typeof kb.matches
      >[1];
      if (
        !kb.matches(data, "tui.input.tab") &&
        !kb.matches(data, toggleNamedFilterKey) &&
        data !== "\u0012" &&
        data !== "\u0004"
      )
        return false;

      void (async () => {
        if (data === "\u0012") {
          const selected = createThreadedSessionSelectItems(sessions).find(
            (item) => item.value === overlay.getSelectedValue(),
          );
          if (!selected) return;
          const currentName = sessions.find((s) => s.path === selected.value)?.name ?? "";
          const prompt = new PromptOverlay(
            "Rename Session",
            currentName,
            "Enter save  Esc cancel",
            (value) => {
              void ctx.host.renameSession(selected.value, value).then(async () => {
                sessions = await loadSessions();
                updateOverlayState(overlay);
                ctx.tui.hideOverlay();
                ctx.render();
              });
            },
            () => {
              ctx.tui.hideOverlay();
              ctx.render();
            },
          );
          ctx.tui.showOverlay(prompt, { anchor: "center", width: "70%", maxHeight: "30%" });
          return;
        }
        if (data === "\u0004") {
          const selectedValue = overlay.getSelectedValue();
          if (!selectedValue) return;
          if (selectedValue === ctx.host.sessionFile) {
            ctx.msg("system", "Cannot delete the current active session");
            ctx.render();
            return;
          }
          await ctx.host.deleteSession(selectedValue);
          sessions = await loadSessions();
          updateOverlayState(overlay);
          ctx.render();
          return;
        }
        if (kb.matches(data, "tui.input.tab")) {
          scope = scope === "current" ? "all" : "current";
        } else {
          namedOnly = !namedOnly;
        }
        sessions = await loadSessions();
        updateOverlayState(overlay);
        ctx.render();
      })();
      return true;
    },
  );
  updateOverlayState(overlay);
  ctx.setActiveOverlay(
    ctx.tui.showOverlay(overlay, { anchor: "center", width: "80%", maxHeight: "60%" }),
  );
}

export async function openTreeSelector(ctx: OverlayContext): Promise<void> {
  const treeEntries = await ctx.host.getTreeEntries();
  if (treeEntries.length === 0) {
    ctx.msg("system", "Current session has no saved entries yet");
    ctx.render();
    return;
  }

  const tree = buildSessionTree(treeEntries);
  const component = new TreeSelectorComponent(
    tree,
    ctx.host.getLeafId(),
    process.stdout.rows ?? 40,
    (entryId) => {
      void ctx.host
        .branchToEntry(entryId)
        .then(async () => {
          closeOverlay(ctx);
          await ctx.resync(`Switched branch to ${ctx.host.getLeafId()}`);
        })
        .catch((error: unknown) => {
          closeOverlay(ctx);
          ctx.msg("system", error instanceof Error ? error.message : String(error));
          ctx.render();
        });
    },
    () => closeOverlay(ctx),
  );
  ctx.setActiveOverlay(
    ctx.tui.showOverlay(component, { anchor: "center", width: "80%", maxHeight: "70%" }),
  );
}

export async function openForkSelector(ctx: OverlayContext): Promise<void> {
  if (!ctx.host.isSessionPersisted()) {
    ctx.msg("system", "Fork requires a saved session");
    ctx.render();
    return;
  }

  const branch = await ctx.host.getBranchEntries();
  const items = branch
    .filter(
      (entry): entry is Extract<(typeof branch)[number], { type: "message" }> =>
        entry.type === "message",
    )
    .filter((entry) => entry.message.role === "user")
    .map((entry) => ({
      value: entry.id,
      label: entry.id,
      description:
        typeof entry.message.content === "string"
          ? entry.message.content.slice(0, 120)
          : entry.message.content
              .filter((block) => block.type === "text")
              .map((block) => block.text)
              .join(" ")
              .slice(0, 120),
    }))
    .reverse();

  if (items.length === 0) {
    ctx.msg("system", "Current branch has no user messages to fork from");
    ctx.render();
    return;
  }

  const overlay = new SelectorOverlay(
    "Fork From User Message",
    items,
    "Enter fork  Esc cancel  ↑↓ select",
    (item) => {
      void ctx
        .doFork(item.value)
        .then(() => closeOverlay(ctx))
        .catch((error: unknown) => {
          closeOverlay(ctx);
          ctx.msg("system", error instanceof Error ? error.message : String(error));
          ctx.render();
        });
    },
    () => closeOverlay(ctx),
  );
  ctx.setActiveOverlay(
    ctx.tui.showOverlay(overlay, { anchor: "center", width: "80%", maxHeight: "60%" }),
  );
}
