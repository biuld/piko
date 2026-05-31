import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import {
  createDefaultSettings,
  createHostConfig,
  findModel,
  listAvailableModels,
  type ResolvedModel,
} from "piko-host-runtime";
import type { BaseApp } from "./base.js";

export interface ModelDeps extends BaseApp {
  updateHeader(): void;
  updateFooter(): void;
}

export function doGetModelList(
  app: ModelDeps,
): Array<{ model: Model<string>; providerConfig: EngineProviderConfig }> {
  if (app.opts.modelRegistry) {
    return app.opts.modelRegistry.listScopedModels().map((m) => ({
      model: m,
      providerConfig:
        app.opts.modelRegistry!.resolve(m.id, m.provider)?.providerConfig ??
        app.currentProviderConfig,
    }));
  }
  return listAvailableModels().flatMap((p) =>
    p.models.map((m) => {
      const found = findModel(m.id, p.provider);
      return {
        model: { provider: p.provider, id: m.id, name: m.name } as Model<string>,
        providerConfig: found?.providerConfig ?? app.currentProviderConfig,
      };
    }),
  );
}

export function doGetModelIds(app: ModelDeps): string[] {
  if (app.opts.modelRegistry)
    return app.opts.modelRegistry.listScopedModels().map((m) => `${m.provider}/${m.id}`);
  return listAvailableModels().flatMap((p) => p.models.map((m) => `${p.provider}/${m.id}`));
}

export function doResolveModel(app: ModelDeps, id: string, prov: string): ResolvedModel | null {
  if (app.opts.modelRegistry) return app.opts.modelRegistry.resolve(id, prov);
  const f = findModel(id, prov);
  return f ? { model: f.model, providerConfig: f.providerConfig } : null;
}

export function doApplyModelChange(app: ModelDeps, found: ResolvedModel): void {
  app.currentModel = found.model;
  app.currentProviderConfig = found.providerConfig;
  app.host.setConfig(
    createHostConfig(
      found.model,
      found.providerConfig,
      createDefaultSettings({
        maxSteps: 10,
        parallelTools: false,
        allowToolCalls: !app.opts.noTools,
        allowApprovals: true,
      }),
    ),
  );
  app.host.setThinkingLevel(app.currentThinkingLevel);
}

export async function doCycleModel(app: ModelDeps, forward: boolean): Promise<void> {
  const ids = doGetModelIds(app);
  const currentId = `${app.currentModel.provider}/${app.currentModel.id}`;
  const idx = ids.indexOf(currentId);
  if (idx === -1 || ids.length === 0) return;
  const nextId = ids[(idx + (forward ? 1 : -1) + ids.length) % ids.length];
  const [prov, id] = nextId.split("/");
  const found = doResolveModel(app, id, prov);
  if (!found) return;
  doApplyModelChange(app, found);
  app.chatView.addMessage("system", `Switched to ${found.model.provider}/${found.model.id}`);
  app.updateHeader();
  app.updateFooter();
  app.chatView.rebuildChat();
  app.tui.requestRender();
}
