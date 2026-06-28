import {
  createModelPickerPanelSession,
  createThinkingPanelSession,
} from "../../panels/panel-factories.js";
import type { ProviderInfo } from "../../shared/types.js";
import type { CommandDefinition } from "../types.js";
import type { BuiltinCommandContext } from "./types.js";

type FlatModelEntry = ProviderInfo["models"][number] & { provider: string };

export function createModelCommands(ctx: BuiltinCommandContext): CommandDefinition[] {
  return [
    {
      id: "piko.model.select",
      slash: {
        name: "/model",
        aliases: ["/m"],
        description: "Select a model",
      },
      keybindings: ["app.model.select"],
      requiresIdle: true,
      run(_ctx) {
        ctx().openPanel({
          placement: "partial",
          panel: createModelPickerPanelSession(),
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
  ];
}

function cycleModel(ctx: BuiltinCommandContext, direction: 1 | -1): void {
  const state = ctx().getState();
  const catalog = (state.model.modelCatalog || []) as ProviderInfo[];
  const allModels: FlatModelEntry[] = catalog
    .filter((p) => p.hasAuth)
    .flatMap((p) => p.models.map((m) => ({ ...m, provider: p.provider })));

  if (allModels.length <= 1) {
    ctx().notify("Only one model available", "info");
    return;
  }
  const current = state.model.current;
  const idx = allModels.findIndex(
    (m: any) => m.id === current.id && m.provider === current.provider,
  );
  const next = allModels[(idx + direction + allModels.length) % allModels.length];
  ctx().switchModel(next.id, next.provider);
}
