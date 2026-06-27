/**
 * Auth System — API key and OAuth credential storage.
 *
 * Backends:
 * - File: ~/.piko/auth.json
 * - InMemory: for tests / --api-key
 *
 * Priority for resolving API keys:
 * 1. Runtime override (CLI --api-key)
 * 2. Auth storage (auth.json)
 * 3. Environment variable
 */

import { getEnvApiKey } from "@earendil-works/pi-ai";
import { getPikoDir } from "../session/index.js";
import { joinPath } from "../utils/bun-path.js";
import type { OAuthCredentials, OAuthLoginCallbacks } from "./oauth-types.js";

// ============================================================================
// Types
// ============================================================================

export type ApiKeyCredential = {
  type: "api_key";
  key: string;
};

export type OAuthCredential = {
  type: "oauth";
} & OAuthCredentials;

export type AuthCredential = ApiKeyCredential | OAuthCredential;

export type AuthStorageData = Record<string, AuthCredential>;

export type AuthStatus = {
  configured: boolean;
  source?: "stored" | "runtime" | "environment";
  label?: string;
};

// ============================================================================
// File backend
// ============================================================================

export class FileAuthStorage {
  private authPath: string;

  constructor(authPath: string = joinPath(getPikoDir(), "auth.json")) {
    this.authPath = authPath;
  }

  private async ensureFile(): Promise<void> {
    if (!(await Bun.file(this.authPath).exists())) {
      await Bun.write(this.authPath, "{}", { createPath: true, mode: 0o600 });
    }
  }

  async read(): Promise<AuthStorageData> {
    try {
      await this.ensureFile();
      const content = await Bun.file(this.authPath).text();
      return JSON.parse(content) as AuthStorageData;
    } catch {
      return {};
    }
  }

  async write(data: AuthStorageData): Promise<void> {
    await Bun.write(this.authPath, JSON.stringify(data, null, 2), {
      createPath: true,
      mode: 0o600,
    });
  }
}

// ============================================================================
// In-memory backend
// ============================================================================

export class InMemoryAuthStorage {
  private data: AuthStorageData;

  constructor(data: AuthStorageData = {}) {
    this.data = data;
  }

  async read(): Promise<AuthStorageData> {
    return { ...this.data };
  }

  async write(data: AuthStorageData): Promise<void> {
    this.data = data;
  }
}

// ============================================================================
// Auth storage
// ============================================================================

export class AuthStorage {
  private data: AuthStorageData = {};
  private runtimeOverrides = new Map<string, string>();
  private backend: FileAuthStorage | InMemoryAuthStorage;
  private pendingPersist: Promise<void> = Promise.resolve();

  constructor(backend: FileAuthStorage | InMemoryAuthStorage) {
    this.backend = backend;
  }

  /** Create an AuthStorage backed by ~/.piko/auth.json. */
  static async create(authPath?: string): Promise<AuthStorage> {
    const storage = new AuthStorage(new FileAuthStorage(authPath));
    await storage.reload();
    return storage;
  }

  /** Create an in-memory AuthStorage for tests / --api-key. */
  static inMemory(data: AuthStorageData = {}): AuthStorage {
    const storage = new AuthStorage(new InMemoryAuthStorage(data));
    storage.data = { ...data };
    return storage;
  }

  // ---- Runtime API key ----

  setRuntimeApiKey(provider: string, apiKey: string): void {
    this.runtimeOverrides.set(provider, apiKey);
  }

  removeRuntimeApiKey(provider: string): void {
    this.runtimeOverrides.delete(provider);
  }

  // ---- Persistence ----

  async reload(): Promise<void> {
    this.data = await this.backend.read();
  }

  private save(): void {
    const snapshot = { ...this.data };
    this.pendingPersist = this.pendingPersist
      .catch(() => {})
      .then(() => this.backend.write(snapshot));
  }

  async flush(): Promise<void> {
    await this.pendingPersist;
  }

  // ---- CRUD ----

  get(provider: string): AuthCredential | undefined {
    return this.data[provider];
  }

  set(provider: string, credential: AuthCredential): void {
    this.data[provider] = credential;
    this.save();
  }

  remove(provider: string): void {
    delete this.data[provider];
    this.save();
  }

  list(): string[] {
    return Object.keys(this.data);
  }

  has(provider: string): boolean {
    return provider in this.data;
  }

  hasAuth(provider: string): boolean {
    if (this.runtimeOverrides.has(provider)) return true;
    if (this.data[provider]) return true;
    if (getEnvApiKey(provider)) return true;
    return false;
  }

  getAuthStatus(provider: string): AuthStatus {
    if (this.data[provider]) {
      return { configured: true, source: "stored" };
    }
    if (this.runtimeOverrides.has(provider)) {
      return { configured: false, source: "runtime", label: "--api-key" };
    }
    const envKey = getEnvApiKey(provider);
    if (envKey) {
      return { configured: false, source: "environment" };
    }
    return { configured: false };
  }

  getAll(): AuthStorageData {
    return { ...this.data };
  }

  // ---- API key resolution ----

  /**
   * Resolve API key for a provider.
   * Priority: runtime override > stored API key > stored OAuth > env variable.
   */
  getApiKey(provider: string): string | undefined {
    // Runtime override
    const runtimeKey = this.runtimeOverrides.get(provider);
    if (runtimeKey) return runtimeKey;

    // Stored API key
    const cred = this.data[provider];
    if (cred?.type === "api_key") return cred.key;

    // Stored OAuth credential
    if (cred?.type === "oauth") {
      // For hostd proxy architecture, TUI shouldn't be generating API keys from OAuth creds
      // as hostd does it. But if needed for local rendering, just return access.
      return cred.access;
    }

    // Environment variable
    return getEnvApiKey(provider) ?? undefined;
  }

  // ---- OAuth login ----
}
