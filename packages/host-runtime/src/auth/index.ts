// OAuth types

export { loginAnthropic, refreshAnthropicToken } from "./anthropic.js";
export type { AntigravityCredentials } from "./antigravity.js";
export { loginAntigravity, refreshAntigravityToken } from "./antigravity.js";

// Device code flow (RFC 8628)
export { pollOAuthDeviceCodeFlow } from "./device-code.js";
export { loginGitHubCopilot, refreshGitHubCopilotToken } from "./github-copilot.js";
// Provider implementations
// Provider registry
export {
  anthropicOAuthProvider,
  antigravityOAuthProvider,
  getOAuthApiKey,
  getOAuthProvider,
  getOAuthProviders,
  githubCopilotOAuthProvider,
  openaiCodexOAuthProvider,
  refreshOAuthToken,
  registerOAuthProvider,
  resetOAuthProviders,
  unregisterOAuthProvider,
} from "./oauth-providers.js";
export type {
  OAuthAuthInfo,
  OAuthCredentials,
  OAuthDeviceCodeInfo,
  OAuthLoginCallbacks,
  OAuthPrompt,
  OAuthProviderId,
  OAuthProviderInterface,
  OAuthSelectOption,
  OAuthSelectPrompt,
} from "./oauth-types.js";
export {
  loginOpenAICodex,
  loginOpenAICodexDeviceCode,
  refreshOpenAICodexToken,
} from "./openai-codex/index.js";

// Auth storage
export type {
  ApiKeyCredential,
  AuthCredential,
  AuthStatus,
  AuthStorageData,
  OAuthCredential,
} from "./storage.js";
export {
  AuthStorage,
  FileAuthStorage,
  InMemoryAuthStorage,
} from "./storage.js";
