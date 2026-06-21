import {
  createModelPickerPanelSession,
  createThinkingPanelSession,
} from "../../panels/panel-factories.js";
import type { CommandDefinition } from "../types.js";
import type { BuiltinCommandContext } from "./types.js";

export function createModelCommands(ctx: BuiltinCommandContext): CommandDefinition[] {
  return [
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
        if (args) {
          const parts = args.includes("/") ? args.split("/") : [undefined, args];
          const provider = parts[0];
          const modelId = parts[1] ?? parts[0];
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
        const panel = createModelPickerPanelSession();
        if (args) panel.state.filterText = args;
        ctx().openPanel({
          placement: "partial",
          panel,
        });
      },
    },
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
        ctx().openPanel({
          placement: "partial",
          panel: createThinkingPanelSession(),
        });
      },
    },
    {
      id: "piko.model.cycleForward",
      keybindings: ["app.model.cycleForward"],
      run(_ctx) {
        cycleModel(ctx, 1);
      },
    },
    {
      id: "piko.model.cycleBackward",
      keybindings: ["app.model.cycleBackward"],
      run(_ctx) {
        cycleModel(ctx, -1);
      },
    },
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

function cycleModel(ctx: BuiltinCommandContext, direction: 1 | -1): void {
  const state = ctx().getState();
  const models = ctx().modelRegistry?.listScopedModels() || [];
  if (models.length <= 1) {
    ctx().notify("Only one model available", "info");
    return;
  }
  const current = state.model.current;
  const idx = models.findIndex((m: any) => m.id === current.id && m.provider === current.provider);
  const next = models[(idx + direction + models.length) % models.length];
  ctx().switchModel(next.id, next.provider);
}
