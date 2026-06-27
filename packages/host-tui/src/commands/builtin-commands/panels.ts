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
        description: "Login to a provider",
      },
      requiresIdle: true,
      run(_ctx, _args) {
        ctx().openPanel({
          placement: "partial",
          inputPolicy: "capture",
          panel: createLoginPanelSession(),
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
        void args;
        ctx().notify("Logout is handled by hostd and is not exposed in this TUI yet.", "warning");
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
