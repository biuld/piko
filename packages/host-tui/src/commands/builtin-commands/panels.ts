import {
  createChangelogPanelSession,
  createHelpPanelSession,
  createHotkeysPanelSession,
  createLoginPanelSession,
  createNotificationsPanelSession,
  createSettingsPanelSession,
} from "../../panels/panel-factories.js";
import type { CommandDefinition } from "../types.js";
import type { BuiltinCommandContext } from "./types.js";

export function createPanelCommands(ctx: BuiltinCommandContext): CommandDefinition[] {
  return [
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
    {
      id: "piko.auth.login",
      slash: {
        name: "/login",
        description: "Login to provider",
        argumentHint: "[provider]",
      },
      requiresIdle: true,
      run(_ctx, args) {
        const provider = args?.trim() || undefined;
        ctx().openPanel({
          placement: "partial",
          inputPolicy: "capture",
          panel: createLoginPanelSession(provider),
        });
      },
    },
    {
      id: "piko.auth.logout",
      slash: {
        name: "/logout",
        description: "Logout from provider",
        argumentHint: "[provider]",
      },
      requiresIdle: true,
      async run(_ctx, args) {
        try {
          const provider = args?.trim();
          if (!provider) {
            ctx().notify("Usage: /logout <provider> (e.g. /logout openai)", "warning");
            return;
          }
          const registry = ctx().modelRegistry;
          if (registry) {
            const authStorage = registry.getAuthStorage();
            if (authStorage.has(provider)) {
              authStorage.remove(provider);
              ctx().notify(`Successfully logged out of ${provider}`, "success");
            } else {
              ctx().notify(`No active login session found for ${provider}`, "warning");
            }
          } else {
            ctx().notify("Auth storage not available", "error");
          }
        } catch (e: any) {
          ctx().notify(`Logout failed: ${e.message}`, "error");
        }
      },
    },
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
  ];
}
