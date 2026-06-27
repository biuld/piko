import type {
  Model,
  ModelProviderConfig,
  ModelSummary,
  ProviderInfo,
} from "../shared/orchd/protocol/index.js";

export interface TuiResolvedModel {
  model: Model<string> | ModelSummary;
  providerConfig: ModelProviderConfig;
}

export interface TuiModelCatalog {
  resolve(modelId?: string, providerName?: string): TuiResolvedModel | null;
  listProviders?(): ProviderInfo[];
  listModels?(): ModelSummary[];
  listScopedModels?(): { provider: string; model: ModelSummary }[];
}
