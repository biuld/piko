import { SessionManager } from "./session-manager.js";

export type SessionReplaceReason = "new" | "resume" | "fork" | "clone";

export interface ReplaceSessionEvent {
  reason: SessionReplaceReason;
  previousSessionId: string;
  nextSessionId: string;
}

export interface CreateSessionRuntimeOptions {
  cwd?: string;
  session?: string;
}

export class PikoSessionRuntime {
  private sessionManager: SessionManager;
  private onSessionReplaced?: (event: ReplaceSessionEvent) => Promise<void> | void;

  private constructor(sessionManager: SessionManager) {
    this.sessionManager = sessionManager;
  }

  static async create(options: CreateSessionRuntimeOptions = {}): Promise<PikoSessionRuntime> {
    const cwd = options.cwd ?? process.cwd();
    const sessionManager = options.session
      ? await SessionManager.open(options.session, cwd)
      : await SessionManager.continueRecent(cwd);
    return new PikoSessionRuntime(sessionManager ?? await SessionManager.create(cwd));
  }

  getSessionManager(): SessionManager {
    return this.sessionManager;
  }

  getCwd(): string {
    return this.sessionManager.getCwd();
  }

  setOnSessionReplaced(handler?: (event: ReplaceSessionEvent) => Promise<void> | void): void {
    this.onSessionReplaced = handler;
  }

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

  private async replaceSession(reason: SessionReplaceReason, nextSessionManager: SessionManager): Promise<void> {
    const previousSessionId = this.sessionManager.getSessionId();
    this.sessionManager = nextSessionManager;
    await this.onSessionReplaced?.({
      reason,
      previousSessionId,
      nextSessionId: nextSessionManager.getSessionId(),
    });
  }
}
