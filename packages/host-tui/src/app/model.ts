import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import {
  createDefaultSettings, createHostConfig, findModel, listAvailableModels,
  type ResolvedModel,
} from "piko-host-runtime";
import type { AppConstructor, BaseApp } from "./base.js";

export function ModelMixin<TBase extends AppConstructor<BaseApp>>(Base: TBase) {
  return class extends Base {
    getModelList(this: any): Array<{ model: Model<string>; providerConfig: EngineProviderConfig }> {
      if (this.opts.modelRegistry) {
        return this.opts.modelRegistry.listScopedModels().map((m: any) => ({
          model: m, providerConfig: this.opts.modelRegistry!.resolve(m.id, m.provider)?.providerConfig ?? this.currentProviderConfig,
        }));
      }
      return listAvailableModels().flatMap((p) => p.models.map((m) => {
        const found = findModel(m.id, p.provider);
        return { model: { provider: p.provider, id: m.id, name: m.name } as Model<string>, providerConfig: found?.providerConfig ?? this.currentProviderConfig };
      }));
    }

    getModelIds(this: any): string[] {
      if (this.opts.modelRegistry) return this.opts.modelRegistry.listScopedModels().map((m: any) => `${m.provider}/${m.id}`);
      return listAvailableModels().flatMap((p) => p.models.map((m) => `${p.provider}/${m.id}`));
    }

    resolveModel(this: any, id: string, prov: string): ResolvedModel | null {
      if (this.opts.modelRegistry) return this.opts.modelRegistry.resolve(id, prov);
      const f = findModel(id, prov);
      return f ? { model: f.model, providerConfig: f.providerConfig } : null;
    }

    applyModelChange(this: any, found: ResolvedModel): void {
      this.currentModel = found.model;
      this.currentProviderConfig = found.providerConfig;
      this.host.setConfig(createHostConfig(found.model, found.providerConfig, createDefaultSettings({
        maxSteps: 10, parallelTools: false, allowToolCalls: !this.opts.noTools, allowApprovals: true,
      })));
      this.host.setThinkingLevel(this.currentThinkingLevel);
    }

    async cycleModel(this: any, forward: boolean): Promise<void> {
      const ids = this.getModelIds();
      const currentId = `${this.currentModel.provider}/${this.currentModel.id}`;
      const idx = ids.indexOf(currentId);
      if (idx === -1 || ids.length === 0) return;
      const nextId = ids[(idx + (forward ? 1 : -1) + ids.length) % ids.length];
      const [prov, id] = nextId.split("/");
      const found = this.resolveModel(id, prov);
      if (!found) return;
      this.applyModelChange(found);
      this.chatView.addMessage("system", `Switched to ${found.model.provider}/${found.model.id}`);
      this.updateHeader(); this.updateFooter();
      this.chatView.rebuildChat(); this.tui.requestRender();
    }
  };
}
