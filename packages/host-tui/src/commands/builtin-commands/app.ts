import type { CommandDefinition } from "../types.js";
import type { BuiltinCommandContext } from "./types.js";

export function createAppCommands(ctx: BuiltinCommandContext): CommandDefinition[] {
  return [
    {
      id: "piko.config.reload",
      slash: {
        name: "/reload",
        description: "Reload configuration",
      },
      run(_ctx) {
        const sm = ctx().host.getSettingsManager();
        if (sm) {
          sm.reload();
          ctx().notify("Configuration reloaded", "success");
        } else {
          ctx().notify("No settings manager available", "warning");
        }
      },
    },
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
    {
      id: "piko.stream.abort",
      run(_ctx) {
        ctx().abort();
      },
    },
    {
      id: "piko.app.shutdown",
      run(_ctx) {
        ctx().shutdown();
      },
    },
    {
      id: "piko.app.interrupt",
      keybindings: ["app.interrupt"],
      run(_ctx) {
        ctx().abort();
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
  ];
}
