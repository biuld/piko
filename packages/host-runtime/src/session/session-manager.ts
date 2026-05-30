import type { Message } from "piko-engine-protocol";
import {
  appendSessionInfo,
  appendSessionMessages,
  deleteSession,
  writeSessionSnapshot,
} from "./session-io.js";
import {
  findMostRecentSession,
  listAllSessions,
  listSessions,
  readSessionEntries,
  resolveSession,
} from "./session-meta.js";
import { getSessionDir } from "./session-paths.js";
import type {
  SessionEntry,
  SessionHandle,
  SessionHeader,
  SessionInfoEntry,
  SessionMessageEntry,
  SessionMeta,
} from "./session-types.js";

function createSessionId(): string {
  return crypto.randomUUID();
}

function createEntryId(index: number): string {
  return index.toString(16).padStart(8, "0").slice(-8);
}

export class SessionManager {
  private cwd: string;
  private sessionId: string;
  private sessionPath?: string;
  private leafId: string | null;
  private parentSessionPath?: string;

  private constructor(
    cwd: string,
    sessionId: string,
    sessionPath?: string,
    leafId: string | null = null,
    parentSessionPath?: string,
  ) {
    this.cwd = cwd;
    this.sessionId = sessionId;
    this.sessionPath = sessionPath;
    this.leafId = leafId;
    this.parentSessionPath = parentSessionPath;
  }

  static async create(
    cwd: string = process.cwd(),
    options: { parentSession?: string } = {},
  ): Promise<SessionManager> {
    return new SessionManager(cwd, createSessionId(), undefined, null, options.parentSession);
  }

  static async open(
    specifier: string,
    cwd: string = process.cwd(),
  ): Promise<SessionManager | null> {
    const handle = await resolveSession(specifier, cwd);
    if (!handle) return null;
    const entries = await readSessionEntries(handle.path);
    const header = entries.find((entry): entry is SessionHeader => entry.type === "session");
    const sessionEntries = entries.filter(
      (entry): entry is SessionEntry => entry.type !== "session",
    );
    return new SessionManager(
      handle.cwd,
      handle.id,
      handle.path,
      sessionEntries[sessionEntries.length - 1]?.id ?? null,
      header?.parentSession,
    );
  }

  static async continueRecent(cwd: string = process.cwd()): Promise<SessionManager | null> {
    const handle = await findMostRecentSession(cwd);
    if (!handle) return null;
    const entries = await readSessionEntries(handle.path);
    const header = entries.find((entry): entry is SessionHeader => entry.type === "session");
    const sessionEntries = entries.filter(
      (entry): entry is SessionEntry => entry.type !== "session",
    );
    return new SessionManager(
      handle.cwd,
      handle.id,
      handle.path,
      sessionEntries[sessionEntries.length - 1]?.id ?? null,
      header?.parentSession,
    );
  }

  static async list(cwd: string = process.cwd()): Promise<SessionMeta[]> {
    return listSessions(cwd);
  }

  static async listAll(): Promise<SessionMeta[]> {
    return listAllSessions();
  }

  static async rename(
    specifier: string,
    name?: string,
    cwd: string = process.cwd(),
  ): Promise<boolean> {
    const manager = await SessionManager.open(specifier, cwd);
    if (!manager) return false;
    await manager.setSessionName(name);
    return true;
  }

  static async delete(specifier: string, cwd: string = process.cwd()): Promise<boolean> {
    const handle = await resolveSession(specifier, cwd);
    if (!handle) return false;
    await deleteSession(handle.path);
    return true;
  }

  getSessionId(): string {
    return this.sessionId;
  }

  getSessionFile(): string | undefined {
    return this.sessionPath;
  }

  getSessionDir(): string {
    return getSessionDir(this.cwd);
  }

  getCwd(): string {
    return this.cwd;
  }

  getParentSessionPath(): string | undefined {
    return this.parentSessionPath;
  }

  async getSessionName(): Promise<string | undefined> {
    const entries = await this.getEntries();
    const sessionInfoEntries = entries.filter(
      (entry): entry is SessionInfoEntry => entry.type === "session_info",
    );
    return sessionInfoEntries[sessionInfoEntries.length - 1]?.name;
  }

  getLeafId(): string | null {
    return this.leafId;
  }

  isPersisted(): boolean {
    return this.sessionPath !== undefined;
  }

  async getHeader(): Promise<SessionHeader | null> {
    if (!this.sessionPath) return null;
    const entries = await readSessionEntries(this.sessionPath);
    return entries.find((entry): entry is SessionHeader => entry.type === "session") ?? null;
  }

  async getEntry(entryId: string): Promise<SessionEntry | null> {
    const resolvedEntryId = await this.resolveEntryId(entryId);
    if (!resolvedEntryId) return null;
    const entries = await this.getEntries();
    return entries.find((entry) => entry.id === resolvedEntryId) ?? null;
  }

  async loadMessages(): Promise<Message[]> {
    const branch = await this.getBranch();
    return branch
      .filter((entry): entry is SessionMessageEntry => entry.type === "message")
      .map((entry) => entry.message);
  }

  async getEntries(): Promise<SessionEntry[]> {
    if (!this.sessionPath) return [];
    const entries = await readSessionEntries(this.sessionPath);
    return entries.filter((entry): entry is SessionEntry => entry.type !== "session");
  }

  async getBranch(): Promise<SessionEntry[]> {
    return this.getBranchFromLeafId(this.leafId);
  }

  async getTree(): Promise<Array<SessionEntry & { isLeaf: boolean; isOnCurrentBranch: boolean }>> {
    const entries = await this.getEntries();
    const branchEntryIds = new Set((await this.getBranch()).map((entry) => entry.id));
    return entries.map((entry) => ({
      ...entry,
      isLeaf: entry.id === this.leafId,
      isOnCurrentBranch: branchEntryIds.has(entry.id),
    }));
  }

  async branch(branchFromId: string): Promise<void> {
    const resolvedEntryId = await this.resolveEntryId(branchFromId);
    if (!resolvedEntryId) {
      throw new Error(`Entry ${branchFromId} not found`);
    }
    this.leafId = resolvedEntryId;
  }

  resetLeaf(): void {
    this.leafId = null;
  }

  newSession(options: { parentSession?: string } = {}): void {
    this.sessionId = createSessionId();
    this.sessionPath = undefined;
    this.leafId = null;
    this.parentSessionPath = options.parentSession;
  }

  async saveMessages(modelId: string, messages: Message[]): Promise<void> {
    const existingMessages = await this.loadMessages();
    const newMessages = messages.slice(existingMessages.length);
    if (newMessages.length === 0 && this.sessionPath) return;
    const result = await appendSessionMessages(
      this.sessionId,
      modelId,
      newMessages,
      this.cwd,
      this.sessionPath,
      this.leafId,
      this.parentSessionPath,
    );
    this.sessionPath = result.path;
    this.leafId = result.lastEntryId;
  }

  async setSessionName(name?: string): Promise<void> {
    const normalized = name?.trim();
    const result = await appendSessionInfo(
      this.sessionId,
      this.cwd,
      normalized ? normalized : undefined,
      this.sessionPath,
      this.leafId,
      this.parentSessionPath,
    );
    this.sessionPath = result.path;
    this.leafId = result.lastEntryId;
  }

  async createBranchedSession(entryId: string | null = this.leafId): Promise<SessionManager> {
    const branchEntries = await this.getBranchFromLeafId(entryId);
    const rebasedEntries = this.rebaseBranchEntries(branchEntries);
    const sessionId = createSessionId();
    const sessionPath = await writeSessionSnapshot(sessionId, rebasedEntries, this.cwd, {
      parentSession: this.sessionPath,
    });
    return new SessionManager(
      this.cwd,
      sessionId,
      sessionPath,
      rebasedEntries[rebasedEntries.length - 1]?.id ?? null,
      this.sessionPath,
    );
  }

  async fork(
    entryId: string,
    options: { position?: "before" | "at" } = {},
  ): Promise<{ sessionManager: SessionManager; selectedText?: string }> {
    const entry = await this.getEntry(entryId);
    if (!entry) {
      throw new Error(`Entry ${entryId} not found`);
    }

    const position = options.position ?? "before";
    if (position === "before") {
      if (entry.type !== "message" || entry.message.role !== "user") {
        throw new Error("Fork before requires a user message entry");
      }
      const selectedText =
        typeof entry.message.content === "string"
          ? entry.message.content
          : entry.message.content
              .filter((block) => block.type === "text")
              .map((block) => block.text)
              .join("\n");
      return {
        sessionManager: await this.createBranchedSession(entry.parentId),
        selectedText,
      };
    }

    return {
      sessionManager: await this.createBranchedSession(entry.id),
    };
  }

  async reopen(handle: SessionHandle): Promise<void> {
    this.cwd = handle.cwd;
    this.sessionId = handle.id;
    this.sessionPath = handle.path;
    const header = await this.getHeader();
    const entries = await this.getEntries();
    this.leafId = entries[entries.length - 1]?.id ?? null;
    this.parentSessionPath = header?.parentSession;
  }

  private async getBranchFromLeafId(leafId: string | null): Promise<SessionEntry[]> {
    const entries = await this.getEntries();
    if (leafId === null) return [];
    const byId = new Map(entries.map((entry) => [entry.id, entry]));
    const branch: SessionEntry[] = [];
    let current = leafId ? byId.get(leafId) : undefined;
    while (current) {
      branch.unshift(current);
      current = current.parentId ? byId.get(current.parentId) : undefined;
    }
    return branch;
  }

  private rebaseBranchEntries(entries: SessionEntry[]): SessionEntry[] {
    let parentId: string | null = null;
    return entries.map((entry, index) => {
      const id = createEntryId(index);
      const rebasedParentId = parentId;
      parentId = id;
      if (entry.type === "message") {
        return {
          ...entry,
          id,
          parentId: rebasedParentId,
        };
      }
      return {
        ...entry,
        id,
        parentId: rebasedParentId,
      };
    });
  }

  private async resolveEntryId(entryId: string): Promise<string | null> {
    const entries = await this.getEntries();
    const exact = entries.find((entry) => entry.id === entryId);
    if (exact) return exact.id;

    const partialMatches = entries.filter((entry) => entry.id.includes(entryId));
    if (partialMatches.length === 1) {
      return partialMatches[0]!.id;
    }
    if (partialMatches.length > 1) {
      throw new Error(`Entry ${entryId} is ambiguous`);
    }
    return null;
  }
}
