import { mkdirp } from "../utils/bun-fs.js";
import { basenamePath, joinPath, resolvePath } from "../utils/bun-path.js";
import { SessionManager } from "./session-manager.js";
import { getSessionDir } from "./session-paths.js";

// ---- Types ----

export type SessionReplaceReason = "new" | "resume" | "fork" | "clone" | "import";

export interface ReplaceSessionEvent {
  reason: SessionReplaceReason;
  previousSessionId: string;
  nextSessionId: string;
}

export interface CreateSessionRuntimeOptions {
  cwd?: string;
  session?: string;
}

/**
 * Diagnostic collected during runtime creation.
 */
export interface SessionRuntimeDiagnostic {
  kind: "info" | "warning" | "error";
  message: string;
  detail?: unknown;
}

export class SessionImportFileNotFoundError extends Error {
  readonly filePath: string;

  constructor(filePath: string) {
    super(`File not found: ${filePath}`);
    this.name = "SessionImportFileNotFoundError";
    this.filePath = filePath;
  }
}

// ---- Runtime ----

export class PikoSessionRuntime {
  private sessionManager: SessionManager;
  private _diagnostics: SessionRuntimeDiagnostic[];

  // Lifecycle hooks
  private onSessionReplaced?: (event: ReplaceSessionEvent) => Promise<void> | void;
  private beforeInvalidate?: () => void;
  private afterRebind?: () => Promise<void> | void;

  private constructor(
    sessionManager: SessionManager,
    diagnostics: SessionRuntimeDiagnostic[] = [],
  ) {
    this.sessionManager = sessionManager;
    this._diagnostics = diagnostics;
  }

  // ---- Static factories ----

  /** Wrap an existing SessionManager for test / migration use. */
  static fromSessionManager(sessionManager: SessionManager): PikoSessionRuntime {
    return new PikoSessionRuntime(sessionManager);
  }

  static async create(options: CreateSessionRuntimeOptions = {}): Promise<PikoSessionRuntime> {
    const cwd = options.cwd ?? process.cwd();
    const diagnostics: SessionRuntimeDiagnostic[] = [];

    let sessionManager: SessionManager | null = null;
    if (options.session !== undefined) {
      if (options.session === "") {
        sessionManager = await SessionManager.continueRecent(cwd);
      } else {
        sessionManager = await SessionManager.open(options.session, cwd);
      }
    }

    if (!sessionManager) {
      sessionManager = await SessionManager.create(cwd);
      diagnostics.push({
        kind: "info",
        message: "Created new session",
      });
    } else {
      const overview = await sessionManager.loadPersistenceOverview();
      diagnostics.push({
        kind: "info",
        message: `Loaded existing session with ${overview.mainMessageCount} messages and ${overview.subagentCount} subagents`,
        detail: overview,
      });
    }

    return new PikoSessionRuntime(sessionManager, diagnostics);
  }

  // ---- Accessors ----

  getSessionManager(): SessionManager {
    return this.sessionManager;
  }

  getCwd(): string {
    return this.sessionManager.getCwd();
  }

  get diagnostics(): readonly SessionRuntimeDiagnostic[] {
    return this._diagnostics;
  }

  // ---- Lifecycle hooks ----

  setOnSessionReplaced(handler?: (event: ReplaceSessionEvent) => Promise<void> | void): void {
    this.onSessionReplaced = handler;
  }

  /**
   * Synchronous hook called after teardown but before the new session is applied.
   * Suitable for detaching TUI components before the old session context goes stale.
   */
  setOnBeforeInvalidate(handler?: () => void): void {
    this.beforeInvalidate = handler;
  }

  /**
   * Called after the new session is applied and onSessionReplaced fires.
   * Suitable for reattaching UI to the new session state.
   */
  setOnAfterRebind(handler?: () => Promise<void> | void): void {
    this.afterRebind = handler;
  }

  // ---- Session lifecycle ----

  async switchSession(specifier: string): Promise<SessionManager | null> {
    const nextSessionManager = await SessionManager.open(specifier, this.getCwd());
    if (!nextSessionManager) return null;

    await this.replaceSession("resume", nextSessionManager);
    return nextSessionManager;
  }

  async newSession(options: { parentSession?: string } = {}): Promise<SessionManager> {
    const nextSessionManager = await SessionManager.create(this.getCwd(), options);
    await this.replaceSession("new", nextSessionManager);
    return nextSessionManager;
  }

  async cloneSession(): Promise<SessionManager> {
    const nextSessionManager = await this.sessionManager.createBranchedSession();
    await this.replaceSession("clone", nextSessionManager);
    return nextSessionManager;
  }

  async forkSession(
    entryId: string,
    options?: Parameters<SessionManager["fork"]>[1],
  ): Promise<Awaited<ReturnType<SessionManager["fork"]>>> {
    const result = await this.sessionManager.fork(entryId, options);
    await this.replaceSession("fork", result.sessionManager);
    return result;
  }

  /**
   * Import a session JSONL file and switch to it.
   *
   * The file is copied into the local session directory before switching.
   *
   * @throws {SessionImportFileNotFoundError} When the input path does not exist.
   */
  async importFromJsonl(inputPath: string): Promise<SessionManager> {
    const resolvedPath = resolvePath(inputPath);
    if (!(await Bun.file(resolvedPath).exists())) {
      throw new SessionImportFileNotFoundError(resolvedPath);
    }

    const sessionDir = getSessionDir(this.getCwd());
    await mkdirp(sessionDir);

    const destinationPath = joinPath(sessionDir, basenamePath(resolvedPath));
    if (resolvePath(destinationPath) !== resolvedPath) {
      await Bun.write(destinationPath, Bun.file(resolvedPath));
    }

    const sessionManager = await SessionManager.open(destinationPath, this.getCwd());
    if (!sessionManager) {
      throw new Error(`Failed to open imported session: ${destinationPath}`);
    }

    await this.replaceSession("import", sessionManager);
    return sessionManager;
  }

  /**
   * Tear down the current session. After calling dispose() the runtime
   * should not be used again.
   */
  async dispose(): Promise<void> {
    this.beforeInvalidate?.();
    // Reset hooks to prevent stale references
    this.onSessionReplaced = undefined;
    this.beforeInvalidate = undefined;
    this.afterRebind = undefined;
  }

  // ---- Internal ----

  private async replaceSession(
    reason: SessionReplaceReason,
    nextSessionManager: SessionManager,
  ): Promise<void> {
    const previousSessionId = this.sessionManager.getSessionId();

    // 1. Invalidate old session
    this.beforeInvalidate?.();

    // 2. Swap
    this.sessionManager = nextSessionManager;

    // 3. Notify observers
    await this.onSessionReplaced?.({
      reason,
      previousSessionId,
      nextSessionId: nextSessionManager.getSessionId(),
    });

    // 4. Rebind
    await this.afterRebind?.();
  }
}
