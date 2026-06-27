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
      async run(_ctx) {
        ctx().notify("Configuration reload is handled by hostd and is not exposed yet.", "warning");
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
    {
      id: "piko.agent.toggleExpand",
      keybindings: ["app.agent.toggleExpand"],
      run(_ctx) {
        const state = ctx().getState();
        const wasExpanded = !!state.expandedAgentId;
        ctx().dispatch({ type: "agent_expansion_toggled" });
        ctx().notify(
          wasExpanded ? `${state.viewedAgentId} collapsed` : `${state.viewedAgentId} expanded`,
          "info",
        );
      },
    },
  ];
}
