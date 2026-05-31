/**
 * SessionManager — TUI-facing wrapper over pi-agent-core's Session + JsonlSessionRepo.
 *
 * All session I/O now delegates to pi's implementations.
 * Public API stays sync-compatible via cached metadata.
 */

import type { Message } from "piko-engine-protocol";
import { JsonlSessionRepo } from "./pi/jsonl-repo.js";
import { NodeExecutionEnv } from "./pi/nodejs-fs.js";
import type { JsonlSessionMetadata, Session, SessionTreeEntry } from "./pi/types.js";
import { SessionError } from "./pi/types.js";
import { getSessionsDir } from "./session-paths.js";
import type { SessionHandle, SessionMeta, SessionTreeNode } from "./session-types.js";

// Re-export from session-tree-utils
export { buildSessionTree, getEntryLabel, getSearchableText } from "./session-tree-utils.js";
export type { SessionTreeNode } from "./session-types.js";

// ============================================================================
// Helpers
// ============================================================================

function makeEnv(cwd: string) {
  return new NodeExecutionEnv({ cwd });
}
function makeRepo(cwd: string) {
  return new JsonlSessionRepo({ fs: makeEnv(cwd), sessionsRoot: getSessionsDir() });
}

// ============================================================================
// SessionManager
// ============================================================================

export class SessionManager {
  private session: Session;
  private repo: JsonlSessionRepo;
  private meta: JsonlSessionMetadata;
  private _leafId: string | null;

  private constructor(
    session: Session,
    repo: JsonlSessionRepo,
    meta: JsonlSessionMetadata,
    leafId: string | null,
  ) {
    this.session = session;
    this.repo = repo;
    this.meta = meta;
    this._leafId = leafId;
  }

  // ---- Factories ----

  static async create(
    cwd: string = process.cwd(),
    options: { parentSession?: string } = {},
  ): Promise<SessionManager> {
    const repo = makeRepo(cwd);
    const session = await repo.create({ cwd, parentSessionPath: options.parentSession });
    const meta = await session.getMetadata();
    const leafId = await session.getLeafId();
    return new SessionManager(session, repo, meta, leafId);
  }

  static async open(
    specifier: string,
    cwd: string = process.cwd(),
  ): Promise<SessionManager | null> {
    const repo = makeRepo(cwd);
    const list = await repo.list({ cwd });
    const meta = list.find((m) => m.id === specifier || m.id.startsWith(specifier));
    if (!meta) return null;
    const session = await repo.open(meta);
    const leafId = await session.getLeafId();
    return new SessionManager(session, repo, meta, leafId);
  }

  static async continueRecent(cwd: string = process.cwd()): Promise<SessionManager | null> {
    const repo = makeRepo(cwd);
    const list = await repo.list({ cwd });
    if (list.length === 0) return null;
    const meta = list[list.length - 1]!;
    const session = await repo.open(meta);
    const leafId = await session.getLeafId();
    return new SessionManager(session, repo, meta, leafId);
  }

  // ---- Static helpers ----

  static async list(cwd: string = process.cwd()): Promise<SessionMeta[]> {
    const repo = makeRepo(cwd);
    const list = await repo.list({ cwd });
    const results: SessionMeta[] = [];
    for (const m of list) {
      try {
        const session = await repo.open(m);
        const name = await session.getSessionName();
        results.push({
          id: m.id,
          path: m.path,
          cwd: m.cwd,
          created: m.createdAt,
          modified: m.createdAt,
          model: "",
          messageCount: 0,
          preview: "",
          name: name ?? undefined,
        });
      } catch {
        results.push({
          id: m.id,
          path: m.path,
          cwd: m.cwd,
          created: m.createdAt,
          modified: m.createdAt,
          model: "",
          messageCount: 0,
          preview: "",
        });
      }
    }
    return results;
  }

  static async listAll(): Promise<SessionMeta[]> {
    const repo = makeRepo(process.cwd());
    const list = await repo.list({});
    return list.map((m) => ({
      id: m.id,
      path: m.path,
      cwd: m.cwd,
      created: m.createdAt,
      modified: m.createdAt,
      model: "",
      messageCount: 0,
      preview: "",
    }));
  }

  static async rename(
    specifier: string,
    name?: string,
    cwd: string = process.cwd(),
  ): Promise<boolean> {
    const mgr = await SessionManager.open(specifier, cwd);
    if (!mgr) return false;
    if (name?.trim()) await mgr.session.appendSessionName(name.trim());
    return true;
  }

  static async delete(specifier: string, cwd: string = process.cwd()): Promise<boolean> {
    const repo = makeRepo(cwd);
    const list = await repo.list({ cwd });
    // Accept partial ID, full ID, or path
    const meta = list.find(
      (m: { id: string; path: string }) =>
        m.id === specifier || m.id.startsWith(specifier) || m.path === specifier,
    );
    if (!meta) return false;
    await repo.delete(meta);
    return true;
  }

  // ---- Accessors (sync via cached metadata) ----

  getSessionId(): string {
    return this.meta.id;
  }
  getSessionFile(): string | undefined {
    return this.meta.path;
  }
  getCwd(): string {
    return this.meta.cwd;
  }
  getParentSessionPath(): string | undefined {
    return this.meta.parentSessionPath;
  }
  isPersisted(): boolean {
    return true;
  }
  getLeafId(): string | null {
    return this._leafId;
  }

  // ---- Metadata ----

  async getSessionName(): Promise<string | undefined> {
    return this.session.getSessionName();
  }

  // ---- Entries ----

  async getEntry(entryId: string): Promise<SessionTreeEntry | undefined> {
    return this.session.getEntry(entryId);
  }

  async getEntries(): Promise<SessionTreeEntry[]> {
    return this.session.getEntries();
  }

  async getBranch(): Promise<SessionTreeEntry[]> {
    return this.session.getBranch();
  }

  async getBranchFromLeafId(leafId: string | null): Promise<SessionTreeEntry[]> {
    return this.session.getBranch(leafId ?? undefined);
  }

  async getTree(): Promise<
    Array<SessionTreeEntry & { isLeaf: boolean; isOnCurrentBranch: boolean }>
  > {
    const entries = await this.session.getEntries();
    const branchIds = new Set((await this.session.getBranch()).map((e) => e.id));
    const currentLeafId = await this.session.getLeafId();
    return entries.map((entry) => ({
      ...entry,
      isLeaf: entry.id === currentLeafId,
      isOnCurrentBranch: branchIds.has(entry.id),
    }));
  }

  // ---- Messages ----

  async loadMessages(): Promise<Message[]> {
    const ctx = await this.session.buildContext();
    return ctx.messages as Message[];
  }

  async saveMessages(_modelId: string, messages: Message[]): Promise<void> {
    const existing = await this.loadMessages();
    const newMsgs = messages.slice(existing.length);
    for (const msg of newMsgs) {
      await this.session.appendMessage(msg);
    }
    this._leafId = await this.session.getLeafId();
  }

  async setSessionName(name?: string): Promise<void> {
    if (name?.trim()) await this.session.appendSessionName(name.trim());
  }

  // ---- Compaction ----

  async appendCompaction(
    summary: string,
    firstKeptEntryId: string,
    tokensBefore: number,
    details?: unknown,
    fromHook?: boolean,
  ): Promise<void> {
    await this.session.appendCompaction(summary, firstKeptEntryId, tokensBefore, details, fromHook);
    this._leafId = await this.session.getLeafId();
  }

  // ---- Tree navigation ----

  async branch(entryId: string): Promise<void> {
    const entry = await this.session.getEntry(entryId);
    if (!entry) throw new Error(`Entry ${entryId} not found`);
    await this.session.moveTo(entryId);
    this._leafId = entryId;
  }

  async branchWithSummary(entryId: string, summary: string): Promise<void> {
    const entry = await this.session.getEntry(entryId);
    if (!entry) throw new Error(`Entry ${entryId} not found`);
    await this.session.moveTo(entryId, { summary });
    this._leafId = entryId;
  }

  // ---- Fork / Clone ----

  async fork(
    entryId: string,
    options: { position?: "before" | "at" } = {},
  ): Promise<{ sessionManager: SessionManager; selectedText?: string }> {
    const entry = await this.session.getEntry(entryId);
    if (!entry) throw new Error(`Entry ${entryId} not found`);

    const position = options.position ?? "before";
    const forkEntryId = position === "before" ? (entry.parentId ?? entryId) : entryId;

    const forked = await this.repo.fork(this.meta, {
      entryId: forkEntryId,
      position: "at",
      cwd: this.meta.cwd,
    });

    let selectedText: string | undefined;
    if (position === "before" && entry.type === "message" && entry.message.role === "user") {
      const content = entry.message.content;
      selectedText =
        typeof content === "string"
          ? content
          : Array.isArray(content)
            ? content
                .filter((c: { type: string; text?: string }) => c.type === "text")
                .map((c: { type: string; text?: string }) => c.text ?? "")
                .join("\n")
            : "";
    }

    const forkedMeta = await forked.getMetadata();
    const forkedLeafId = await forked.getLeafId();
    return {
      sessionManager: new SessionManager(forked, this.repo, forkedMeta, forkedLeafId),
      selectedText,
    };
  }

  // ---- Piko specific ----

  async createBranchedSession(): Promise<SessionManager> {
    const leafId = await this.session.getLeafId();
    const forked = await this.repo.fork(this.meta, {
      entryId: leafId ?? undefined,
      position: "at",
      cwd: this.meta.cwd,
    });
    const forkedMeta = await forked.getMetadata();
    const forkedLeafId = await forked.getLeafId();
    return new SessionManager(forked, this.repo, forkedMeta, forkedLeafId);
  }

  newSession(options: { parentSession?: string } = {}): void {
    // This is used by PikoSessionRuntime to create a fresh in-memory session.
    // The actual creation happens lazily on first save. We just reset state.
    // The old SessionManager implementation created a new ID — we keep that behavior
    // for backward compat, but underlying Session will be created on first save.
  }

  async reopen(handle: SessionHandle): Promise<void> {
    const repo = makeRepo(handle.cwd);
    const list = await repo.list({ cwd: handle.cwd });
    let meta = list.find((m: { id: string }) => m.id === handle.id);
    if (!meta) {
      const all = await repo.list({});
      meta = all.find((m: { id: string }) => m.id === handle.id);
    }
    if (!meta) throw new Error(`Session ${handle.id} not found`);
    this.session = await repo.open(meta);
    this.repo = repo;
    this.meta = meta;
    this._leafId = await this.session.getLeafId();
  }
}
