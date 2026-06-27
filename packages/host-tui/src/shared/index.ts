// ============================================================================
// @piko/shared — minimal types and utilities needed by host-tui
// (stripped from ex-host-runtime)
// ============================================================================

export type {
  ApiKeyCredential,
  AuthCredential,
  AuthStatus,
  AuthStorageData,
  OAuthAuthInfo,
  OAuthCredential,
  OAuthDeviceCodeInfo,
  OAuthLoginCallbacks,
  OAuthPrompt,
  OAuthProviderId,
  OAuthProviderInterface,
  OAuthSelectOption,
  OAuthSelectPrompt,
} from "./auth/index.js";
export {
  AuthStorage,
  FileAuthStorage,
  InMemoryAuthStorage,
} from "./auth/index.js";

export { installDebugTraceFromEnv } from "./debug/file-trace.js";

/** A context file loaded from the project (AGENTS.md, CLAUDE.md, etc.) */
export interface ContextFile {
  path: string;
  content: string;
}

export { PikoHost } from "./host-stub.js";
export type {
  HostConfig,
  ProviderDefinition,
  ProviderInfo,
  ResolvedModel,
} from "./models/index.js";
export {
  createAntigravityModels,
  createDefaultSettings,
  createHostConfig,
  findModel,
  listAvailableModels,
  ModelRegistry,
  registerProvider,
} from "./models/index.js";
export * from "./orchd/protocol/index.js";
export type {
  FlatTreeEntry,
  FlattenedTreeItem,
  GutterInfo,
  SessionHandle,
  SessionMeta,
  SessionTreeEntry,
  SessionTreeNode,
  TextSegment,
  TreeNavigationResult,
} from "./session/index.js";
export {
  buildSessionTree,
  encodeCwd,
  ensurePikoDir,
  flattenSessionTree,
  getAgentDir,
  getEntryLabel,
  getEntrySegments,
  getPikoDir,
  getSearchableText,
  getSessionDir,
  getSessionsDir,
  recalculateVisibleFlatTree,
  renderFlatTree,
} from "./session/index.js";
export type {
  BranchSummarySettings,
  CompactionSettings as SettingsCompactionSettings,
  McpServerConfig,
  RetrySettings,
  Settings,
  TransportSetting,
} from "./settings/index.js";
export { SettingsManager } from "./settings/index.js";
export type {
  CumulativeUsage,
  FileArgument,
  ImageAttachment,
  ImageDimensions,
  ImageResizeOptions,
  TimingEntry,
} from "./utils/index.js";
export {
  applyHttpSettings,
  basenamePath,
  computeCumulativeUsage,
  configureHttpDispatcher,
  createImageAttachment,
  dirnamePath,
  estimateImageTokens,
  extnamePath,
  getContextPercent,
  getGitBranch,
  getImageDimensions,
  getImageFormatFromPath,
  getTimings,
  isAbsolutePath,
  isImage,
  joinPath,
  parsePath,
  pathSeparator,
  processFileArguments,
  resetTimings,
  resolvePath,
  shouldResize,
  Timings,
} from "./utils/index.js";
