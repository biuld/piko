// ============================================================================
// Built-in commands — pi-compatible slash commands, piko-specific commands
// ============================================================================

import {
  createChangelogPanelSession,
  createForkSessionPanelSession,
  createHelpPanelSession,
  createHotkeysPanelSession,
  createImportSessionPanelSession,
  createLoginPanelSession,
  createModelPickerPanelSession,
  createNotificationsPanelSession,
  createRenameSessionPanelSession,
  createResumePanelSession,
  createSettingsPanelSession,
  createThinkingPanelSession,
  createTreePanelSession,
} from "../panels/panel-factories.js";
import type { CommandDefinition } from "./types.js";

/**
 * Create all built-in commands and return them as an array.
 * The caller wires in runtime dependencies (openSurface, notify, etc.)
 * via a factory so commands stay renderer-independent.
 */
export function createBuiltinCommands(
  deps: () => {
    openPanel: (request: any) => string;
    closeSurface: (id?: string) => void;
    notify: (message: string, severity?: string) => void;
    getState: () => any;
    executeCommand: (commandId: string, args?: string) => void;
    shutdown: () => void;
    abort: () => void;
    host: any;
    dispatch: (event: any) => void;
    switchModel: (modelId: string, provider: string) => boolean;
    modelRegistry?: any;
  },
): CommandDefinition[] {
  const ctx = () => deps();

  return [
    // ---- /model ----
    {
      id: "piko.model.select",
      slash: {
        name: "/model",
        aliases: ["/m"],
        description: "Select a model",
        argumentHint: "[provider/]model",
      },
      keybindings: ["app.model.select"],
      requiresIdle: true,
      run(_ctx, args) {
        // If args are provided, try direct model switch
        if (args) {
          const parts = args.includes("/") ? args.split("/") : [undefined, args];
          const provider = parts[0];
          const modelId = parts[1] ?? parts[0];
          const _state = ctx().getState();
          const _registry = ctx().host?.getSettingsManager?.()?.getAuthStorage?.()
            ? undefined
            : undefined; // we actually have host
          // wait, ctx() gives us ActionService which has modelRegistry but ctx() returns an interface:
          // { switchModel, openPanel, notify, getState, host, ... }
          // Does it expose modelRegistry?
          const registryModels = ctx().modelRegistry?.listScopedModels() || [];
          const match = registryModels.find((m: any) => {
            if (provider && m.provider !== provider) return false;
            return m.id === modelId || m.id.startsWith(modelId);
          });
          if (match) {
            ctx().switchModel(match.id, match.provider);
            return;
          }
        }
        // No args or no match — open selector
        const panel = createModelPickerPanelSession();
        if (args) panel.state.filterText = args;
        ctx().openPanel({
          placement: "partial",
          panel,
        });
      },
    },

    // ---- /thinking ----
    {
      id: "piko.thinking.select",
      slash: {
        name: "/thinking",
        aliases: ["/think"],
        description: "Change thinking level",
        argumentHint: "[off|minimal|low|medium|high|xhigh]",
      },
      keybindings: ["app.thinking.toggle"],
      requiresIdle: true,
      run(_ctx, _args) {
        // args is handled by the component internally or we can pass it
        ctx().openPanel({
          placement: "partial",
          panel: createThinkingPanelSession(),
        });
      },
    },

    // ---- /resume ----
    {
      id: "piko.session.resume",
      slash: {
        name: "/resume",
        aliases: ["/r"],
        description: "Resume a previous session",
        argumentHint: "[query]",
      },
      keybindings: ["app.session.resume"],
      requiresIdle: true,
      run(_ctx, args) {
        const panel = createResumePanelSession();
        if (args) panel.state.filterText = args;
        ctx().openPanel({
          placement: "full",
          panel,
        });
      },
    },

    // ---- /settings ----
    {
      id: "piko.settings.open",
      slash: {
        name: "/settings",
        aliases: ["/set"],
        description: "Open settings",
      },
      requiresIdle: true,
      run(_ctx) {
        ctx().openPanel({
          placement: "partial",
          panel: createSettingsPanelSession(),
        });
      },
    },

    // ---- /login ----
    {
      id: "piko.auth.login",
      slash: {
        name: "/login",
        description: "Login to provider",
        argumentHint: "[provider]",
      },
      requiresIdle: true,
      run(_ctx, args) {
        ctx().openPanel({
          placement: "partial",
          inputPolicy: "capture",
          panel: createLoginPanelSession(args),
        });
      },
    },

    // ---- /logout ----
    {
      id: "piko.auth.logout",
      slash: {
        name: "/logout",
        description: "Logout from provider",
        argumentHint: "[provider]",
      },
      requiresIdle: true,
      async run(_ctx, args) {
        const host = ctx().host;
        try {
          const auth = host.getSettingsManager?.()?.getAuthStorage?.();
          if (auth?.clear) {
            if (args) {
              await auth.clear(args);
              ctx().notify(`Logged out from ${args}`, "success");
            } else {
              await auth.clear();
              ctx().notify("Logged out from all providers", "success");
            }
          } else {
            ctx().notify("No auth storage available", "warning");
          }
        } catch (e: any) {
          ctx().notify(`Logout failed: ${e.message}`, "error");
        }
      },
    },

    // ---- /new ----
    {
      id: "piko.session.new",
      slash: {
        name: "/new",
        description: "Start a new session",
      },
      requiresIdle: true,
      async run(_ctx) {
        const host = ctx().host;
        try {
          await host.newSession();
          ctx().notify("New session started", "success");
        } catch (e: any) {
          ctx().notify(`Failed to start new session: ${e.message}`, "error");
        }
      },
    },

    // ---- /compact ----
    {
      id: "piko.session.compact",
      slash: {
        name: "/compact",
        description: "Compact the current session",
      },
      requiresIdle: true,
      async run(_ctx) {
        const host = ctx().host;
        try {
          const result = await host.compact();
          if (result.compacted) {
            ctx().notify(
              `Compacted: ${result.messagesBefore ?? "?"} → ${result.messagesAfter ?? "?"} messages`,
              "success",
            );
          } else {
            ctx().notify(result.reason ?? "Compaction not needed", "info");
          }
        } catch (e: any) {
          ctx().notify(`Compaction failed: ${e.message}`, "error");
        }
      },
    },

    // ---- /fork ----
    {
      id: "piko.session.fork",
      slash: {
        name: "/fork",
        description: "Fork session at a message",
      },
      requiresIdle: true,
      run(_ctx) {
        ctx().openPanel({
          placement: "full",
          panel: createForkSessionPanelSession(),
        });
      },
    },

    // ---- /clone ----
    {
      id: "piko.session.clone",
      slash: {
        name: "/clone",
        description: "Clone current session",
      },
      requiresIdle: true,
      async run(_ctx) {
        const host = ctx().host;
        try {
          await host.cloneSession();
          ctx().notify("Session cloned", "success");
        } catch (e: any) {
          ctx().notify(`Clone failed: ${e.message}`, "error");
        }
      },
    },

    // ---- /tree ----
    {
      id: "piko.session.tree",
      slash: {
        name: "/tree",
        description: "Show session tree",
      },
      keybindings: ["app.session.tree"],
      requiresIdle: true,
      run(_ctx) {
        ctx().openPanel({
          placement: "full",
          panel: createTreePanelSession(),
        });
      },
    },

    // ---- /name ----
    {
      id: "piko.session.rename",
      slash: {
        name: "/name",
        description: "Rename current session",
        argumentHint: "[name]",
      },
      requiresIdle: true,
      async run(_ctx, args) {
        if (!args) {
          ctx().openPanel({
            placement: "partial",
            inputPolicy: "capture",
            panel: createRenameSessionPanelSession(),
          });
          return;
        }
        const host = ctx().host;
        try {
          await host.setSessionName(args);
          ctx().notify(`Session renamed to "${args}"`, "success");
        } catch (e: any) {
          ctx().notify(`Rename failed: ${e.message}`, "error");
        }
      },
    },

    // ---- /notifications ----
    {
      id: "piko.notifications.show",
      slash: {
        name: "/notifications",
        aliases: ["/noti"],
        description: "Show notification history",
      },
      run(_ctx) {
        ctx().openPanel({
          placement: "full",
          panel: createNotificationsPanelSession(),
        });
      },
    },

    // ---- /hotkeys ----
    {
      id: "piko.help.hotkeys",
      slash: {
        name: "/hotkeys",
        description: "Show keybindings",
      },
      run(_ctx) {
        ctx().openPanel({
          placement: "partial",
          panel: createHotkeysPanelSession(),
        });
      },
    },

    // ---- /changelog ----
    {
      id: "piko.help.changelog",
      slash: {
        name: "/changelog",
        description: "Show changelog",
      },
      run(_ctx) {
        ctx().openPanel({
          placement: "partial",
          panel: createChangelogPanelSession(),
        });
      },
    },

    // ---- /export ----
    {
      id: "piko.session.export",
      slash: {
        name: "/export",
        description: "Show session file path",
      },
      requiresIdle: true,
      run(_ctx) {
        const host = ctx().host;
        const file = host.sessionFile;
        if (file) {
          ctx().notify(`Session file: ${file}`, "info");
        } else {
          ctx().notify("Session not yet saved to file", "warning");
        }
      },
    },

    // ---- /import ----
    {
      id: "piko.session.import",
      slash: {
        name: "/import",
        description: "Import session from JSONL file",
        argumentHint: "<path>",
      },
      requiresIdle: true,
      async run(_ctx, args) {
        if (!args) {
          ctx().openPanel({
            placement: "partial",
            inputPolicy: "capture",
            panel: createImportSessionPanelSession(),
          });
          return;
        }
        const host = ctx().host;
        try {
          await host.importSession(args);
          ctx().notify(`Imported session from ${args}`, "success");
        } catch (e: any) {
          ctx().notify(`Import failed: ${e.message}`, "error");
        }
      },
    },

    // ---- /share ----
    {
      id: "piko.session.share",
      slash: {
        name: "/share",
        description: "Share session",
      },
      requiresIdle: true,
      run(_ctx) {
        ctx().notify("Share not yet implemented", "warning");
      },
    },

    // ---- /copy ----
    {
      id: "piko.session.copy",
      slash: {
        name: "/copy",
        description: "Copy session content",
      },
      run(_ctx) {
        ctx().notify("Copy not yet implemented", "warning");
      },
    },

    // ---- /reload ----
    {
      id: "piko.config.reload",
      slash: {
        name: "/reload",
        description: "Reload configuration",
      },
      run(_ctx) {
        const host = ctx().host;
        const sm = host.getSettingsManager?.();
        if (sm?.reload) {
          sm.reload();
          ctx().notify("Configuration reloaded", "success");
        } else {
          ctx().notify("No settings manager available", "warning");
        }
      },
    },

    // ---- /quit / /exit ----
    {
      id: "piko.app.quit",
      slash: {
        name: "/quit",
        aliases: ["/exit", "/q"],
        description: "Exit piko",
      },
      keybindings: ["app.exit"],
      run(_ctx) {
        ctx().shutdown();
      },
    },

    // ---- Interrupt (Esc during stream) ----
    {
      id: "piko.stream.abort",
      run(_ctx) {
        ctx().abort();
      },
    },

    // ---- Shutdown (internal) ----
    {
      id: "piko.app.shutdown",
      run(_ctx) {
        ctx().shutdown();
      },
    },

    // ---- /help ----
    {
      id: "piko.help.show",
      slash: {
        name: "/help",
        aliases: ["/h", "/?"],
        description: "Show help",
      },
      run(_ctx) {
        ctx().openPanel({
          placement: "partial",
          panel: createHelpPanelSession(),
        });
      },
    },

    // ---- App-level commands (no slash) ----
    {
      id: "piko.app.interrupt",
      keybindings: ["app.interrupt"],
      run(_ctx) {
        ctx().abort();
      },
    },

    {
      id: "piko.model.cycleForward",
      keybindings: ["app.model.cycleForward"],
      run(_ctx) {
        const state = ctx().getState();
        const models = ctx().modelRegistry?.listScopedModels() || [];
        if (models.length <= 1) {
          ctx().notify("Only one model available", "info");
          return;
        }
        const current = state.model.current;
        const idx = models.findIndex(
          (m: any) => m.id === current.id && m.provider === current.provider,
        );
        const next = models[(idx + 1) % models.length];
        ctx().switchModel(next.id, next.provider);
      },
    },

    {
      id: "piko.model.cycleBackward",
      keybindings: ["app.model.cycleBackward"],
      run(_ctx) {
        const state = ctx().getState();
        const models = ctx().modelRegistry?.listScopedModels() || [];
        if (models.length <= 1) {
          ctx().notify("Only one model available", "info");
          return;
        }
        const current = state.model.current;
        const idx = models.findIndex(
          (m: any) => m.id === current.id && m.provider === current.provider,
        );
        const prev = models[(idx - 1 + models.length) % models.length];
        ctx().switchModel(prev.id, prev.provider);
      },
    },

    {
      id: "piko.tools.expand",
      keybindings: ["app.tools.expand"],
      run(_ctx) {
        const state = ctx().getState();
        const collapsed = state.timeline.collapsedToolCallIds;
        ctx().dispatch({ type: "timeline_toggle_all_tools" });
        ctx().notify(collapsed.size > 0 ? "All tools expanded" : "All tools collapsed", "info");
      },
    },

    // ---- /scoped-models ----
    {
      id: "piko.stub.scoped-models",
      slash: {
        name: "/scoped-models",
        description: "Select scoped model",
      },
      requiresIdle: true,
      run(_ctx: any) {
        ctx().openPanel({
          placement: "partial",
          panel: createModelPickerPanelSession(),
        });
      },
    },
  ];
}
