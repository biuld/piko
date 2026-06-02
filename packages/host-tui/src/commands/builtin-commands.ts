// ============================================================================
// Built-in commands — pi-compatible slash commands, piko-specific commands
// ============================================================================

import type { CommandDefinition } from "./types.js";

/**
 * Create all built-in commands and return them as an array.
 * The caller wires in runtime dependencies (openSurface, notify, etc.)
 * via a factory so commands stay renderer-independent.
 */
export function createBuiltinCommands(
  deps: () => {
    openSurface: (request: any) => string;
    closeSurface: (id?: string) => void;
    notify: (message: string, severity?: string) => void;
    getState: () => any;
    executeCommand: (commandId: string, args?: string) => void;
    shutdown: () => void;
    abort: () => void;
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
        argumentHint: "[query]",
      },
      keybindings: ["app.model.select"],
      requiresIdle: true,
      run(_ctx, args) {
        ctx().openSurface({
          role: "selector",
          preferredMount: "insert-between",
          targetSlot: "editor",
          contentSize: "medium",
          data: { type: "model", filter: args },
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
      run(_ctx, args) {
        ctx().openSurface({
          role: "selector",
          preferredMount: "insert-between",
          contentSize: "small",
          data: { type: "thinking", level: args },
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
        ctx().openSurface({
          role: "selector",
          preferredMount: "side-drawer",
          contentSize: "large",
          data: { type: "resume", filter: args },
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
        ctx().openSurface({
          role: "menu",
          preferredMount: "insert-between",
          contentSize: "medium",
          data: { type: "settings" },
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
      run(_ctx, _args) {
        ctx().openSurface({
          role: "form",
          preferredMount: "insert-between",
          contentSize: "small",
          requiresSecretInput: true,
          data: { type: "login" },
        });
      },
    },

    // ---- /logout ----
    {
      id: "piko.auth.logout",
      slash: {
        name: "/logout",
        description: "Logout from provider",
      },
      requiresIdle: true,
      run(_ctx, _args) {
        ctx().notify("Logout not yet implemented", "warning");
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
      run(_ctx) {
        ctx().notify("New session not yet implemented", "warning");
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
      run(_ctx) {
        ctx().notify("Compact not yet implemented", "warning");
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
        ctx().notify("Fork not yet implemented", "warning");
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
      run(_ctx) {
        ctx().notify("Clone not yet implemented", "warning");
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
        ctx().openSurface({
          role: "menu",
          preferredMount: "side-drawer",
          contentSize: "large",
          data: { type: "tree" },
        });
      },
    },

    // ---- /name ----
    {
      id: "piko.session.rename",
      slash: {
        name: "/name",
        description: "Rename current session",
      },
      requiresIdle: true,
      run(_ctx) {
        ctx().openSurface({
          role: "form",
          preferredMount: "insert-between",
          contentSize: "small",
          data: { type: "rename" },
        });
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
        ctx().openSurface({
          role: "menu",
          preferredMount: "side-drawer",
          contentSize: "large",
          data: { type: "notifications" },
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
        ctx().openSurface({
          role: "menu",
          preferredMount: "insert-between",
          contentSize: "medium",
          data: { type: "hotkeys" },
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
        ctx().notify("Changelog not yet implemented", "warning");
      },
    },

    // ---- /export ----
    {
      id: "piko.session.export",
      slash: {
        name: "/export",
        description: "Export session",
      },
      requiresIdle: true,
      run(_ctx) {
        ctx().notify("Export not yet implemented", "warning");
      },
    },

    // ---- /import ----
    {
      id: "piko.session.import",
      slash: {
        name: "/import",
        description: "Import a session",
      },
      requiresIdle: true,
      run(_ctx) {
        ctx().notify("Import not yet implemented", "warning");
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
        ctx().notify("Reload not yet implemented", "warning");
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
        ctx().openSurface({
          role: "menu",
          preferredMount: "insert-between",
          contentSize: "medium",
          data: { type: "help" },
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
        ctx().notify("Model cycling not yet implemented", "warning");
      },
    },

    {
      id: "piko.model.cycleBackward",
      keybindings: ["app.model.cycleBackward"],
      run(_ctx) {
        ctx().notify("Model cycling not yet implemented", "warning");
      },
    },

    {
      id: "piko.tools.expand",
      keybindings: ["app.tools.expand"],
      run(_ctx) {
        ctx().notify("Tool expansion toggle not yet implemented", "warning");
      },
    },

    // ---- Stub implementations for pi parity ----
    ...["scoped-models", "session"].map((name) => ({
      id: `piko.stub.${name}`,
      slash: {
        name: `/${name}`,
        description: `[Not implemented] ${name}`,
      },
      run(_ctx: any) {
        ctx().notify(`Command /${name} is not yet implemented`, "warning");
      },
    })),
  ];
}
