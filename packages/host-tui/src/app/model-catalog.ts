import type {
  Model,
  ModelCatalogEntry,
  ModelProviderConfig,
  ProviderInfo,
} from "../shared/orchd/protocol/index.js";

export interface TuiResolvedModel {
  model: Model<string> | ModelCatalogEntry;
  providerConfig: ModelProviderConfig;
}

export interface TuiModelCatalog {
  resolve(modelId?: string, providerName?: string): TuiResolvedModel | null;
  listProviders?(): ProviderInfo[];
  listModels?(): ModelCatalogEntry[];
  listScopedModels?(): ModelCatalogEntry[];
}
