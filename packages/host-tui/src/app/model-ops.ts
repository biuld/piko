import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import {
  createDefaultSettings,
  createHostConfig,
  findModel,
  listAvailableModels,
  type ResolvedModel,
} from "piko-host-runtime";
import type { TuiContext } from "./context.js";

export function createModelOps(ctx: TuiContext) {
  function getModelList(): Array<{ model: Model<string>; providerConfig: EngineProviderConfig }> {
    if (ctx.options.modelRegistry) {
      const registryModels = ctx.options.modelRegistry.listScopedModels();
      return registryModels.map((m) => {
        const resolved = ctx.options.modelRegistry!.resolve(m.id, m.provider);
        return { model: m, providerConfig: resolved?.providerConfig ?? ctx.currentProviderConfig };
      });
    }
    const allProviders = listAvailableModels();
    return allProviders.flatMap((p) =>
      p.models.map((m) => {
        const found = findModel(m.id, p.provider);
        return {
          model: { provider: p.provider, id: m.id, name: m.name } as Model<string>,
          providerConfig: found?.providerConfig ?? ctx.currentProviderConfig,
        };
      }),
    );
  }

  function getModelIds(): string[] {
    if (ctx.options.modelRegistry) {
      return ctx.options.modelRegistry.listScopedModels().map((m) => `${m.provider}/${m.id}`);
    }
    return listAvailableModels().flatMap((p) => p.models.map((m) => `${p.provider}/${m.id}`));
  }

  function resolveModel(id: string, prov: string): ResolvedModel | null {
    if (ctx.options.modelRegistry) return ctx.options.modelRegistry.resolve(id, prov);
    const found = findModel(id, prov);
    return found ? { model: found.model, providerConfig: found.providerConfig } : null;
  }

  function applyModelChange(found: ResolvedModel): void {
    ctx.currentModel = found.model;
    ctx.currentProviderConfig = found.providerConfig;
    const settings = createDefaultSettings({
      maxSteps: 10, parallelTools: false,
      allowToolCalls: ctx.options.noTools ? false : true,
      allowApprovals: true,
    });
    ctx.host.setConfig(createHostConfig(found.model, found.providerConfig, settings));
    ctx.host.setThinkingLevel(ctx.currentThinkingLevel);
  }

  async function cycleModelForward() {
    const currentId = `${ctx.currentModel.provider}/${ctx.currentModel.id}`;
    const modelIds = getModelIds();
    const currentIdx = modelIds.indexOf(currentId);
    if (currentIdx === -1 || modelIds.length === 0) return;
    const nextIdx = (currentIdx + 1) % modelIds.length;
    const [prov, id] = modelIds[nextIdx].split("/");
    const found = resolveModel(id, prov);
    if (found) {
      applyModelChange(found);
      ctx.chatView.addMessage("system", `Switched to ${found.model.provider}/${found.model.id}`);
      ctx.updateHeader();
      ctx.updateFooter();
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
    }
  }

  async function cycleModelBackward() {
    const currentId = `${ctx.currentModel.provider}/${ctx.currentModel.id}`;
    const modelIds = getModelIds();
    const currentIdx = modelIds.indexOf(currentId);
    if (currentIdx === -1 || modelIds.length === 0) return;
    const prevIdx = (currentIdx - 1 + modelIds.length) % modelIds.length;
    const [prov, id] = modelIds[prevIdx].split("/");
    const found = resolveModel(id, prov);
    if (found) {
      applyModelChange(found);
      ctx.chatView.addMessage("system", `Switched to ${found.model.provider}/${found.model.id}`);
      ctx.updateHeader();
      ctx.updateFooter();
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
    }
  }

  return { getModelList, getModelIds, resolveModel, applyModelChange, cycleModelForward, cycleModelBackward };
}
