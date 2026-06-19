/**
 * SessionManager — TUI-facing wrapper over pi-agent-core's Session + JsonlSessionRepo.
 *
 * All session I/O now delegates to pi's implementations.
 * Public API stays sync-compatible via cached metadata.
 */

import type { Message } from "piko-orchestrator-protocol";
import {
  createSessionId,
  createTimestamp,
  type JsonlSessionMetadata,
  type JsonlSessionRepo,
  JsonlSessionStorage,
  Session,
  type SessionTreeEntry,
} from "piko-session";
import { mkdirp } from "../utils/bun-fs.js";
import { joinPath } from "../utils/bun-path.js";
import { BunExecutionEnv } from "./bun-execution-env.js";
import type { ExecutionEnv } from "./exec-env.js";
import { listSessionMetas, makeSessionEnv, makeSessionRepo } from "./session-repo.js";
import {
  type AgentPersistencePolicy,
  type AgentRuntimeEventRecord,
  type AgentSessionRecord,
  type AgentTaskRecord,
  attachedSessionsDirForSessionFile,
  PikoSessionSidecar,
  sanitizeAgentId,
} from "./session-sidecar.js";
import type { SessionHandle, SessionMeta, SessionPersistenceOverview } from "./session-types.js";

// Re-export from session-tree-utils
export { buildSessionTree, getEntryLabel, getSearchableText } from "./session-tree-utils/index.js";
export type { SessionTreeNode } from "./session-types.js";

const defaultSubagentPersistence: AgentPersistencePolicy = {
  kind: "session",
  transcript: "task_scoped",
  context: "empty",
};

// ============================================================================
// SessionManager
// ============================================================================

export class SessionManager {
  private session: Session;
  private repo: JsonlSessionRepo;
  private meta: JsonlSessionMetadata;
  private _leafId: string | null;
  private _execEnv?: ExecutionEnv;

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
    const repo = makeSessionRepo(cwd);
    const session = await repo.create({ cwd, parentSessionPath: options.parentSession });
    const meta = await session.getMetadata();
    const leafId = await session.getLeafId();
    return new SessionManager(session, repo, meta, leafId);
  }

  static async open(
    specifier: string,
    cwd: string = process.cwd(),
  ): Promise<SessionManager | null> {
    const repo = makeSessionRepo(cwd);
    const list = await repo.list({ cwd });
    const meta = list.find((m) => m.id === specifier || m.id.startsWith(specifier));
    if (!meta) return null;
    const session = await repo.open(meta);
    const leafId = await session.getLeafId();
    return new SessionManager(session, repo, meta, leafId);
  }

  static async continueRecent(cwd: string = process.cwd()): Promise<SessionManager | null> {
    const repo = makeSessionRepo(cwd);
    const list = await repo.list({ cwd });
    if (list.length === 0) return null;
    const meta = list[0]!;
    const session = await repo.open(meta);
    const leafId = await session.getLeafId();
    return new SessionManager(session, repo, meta, leafId);
  }

  static async list(cwd: string = process.cwd()): Promise<SessionMeta[]> {
    return listSessionMetas({ cwd });
  }

  static async listAll(): Promise<SessionMeta[]> {
    return listSessionMetas({ all: true });
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
    const repo = makeSessionRepo(cwd);
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

  /** Execution environment for this session (lazy, cached). */
  getExecutionEnv(): ExecutionEnv {
    if (!this._execEnv) {
      this._execEnv = new BunExecutionEnv({ cwd: this.meta.cwd });
    }
    return this._execEnv;
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

  /**
   * Load the full branch entries including metadata (model_change,
   * thinking_level_change, etc.) that buildSessionContext filters out.
   * Used by the TUI to build a complete timeline.
   */
  async loadBranchEntries(): Promise<SessionTreeEntry[]> {
    return this.session.getBranch();
  }

  async saveMessages(_modelId: string, messages: Message[]): Promise<void> {
    const existing = await this.loadMessages();
    const newMsgs = messages.slice(existing.length);
    for (const msg of newMsgs) {
      await this.session.appendMessage(msg);
    }
    this._leafId = await this.session.getLeafId();
  }

  // ---- Multi-agent attached sessions ----

  private async getSidecar(options: { create: boolean }): Promise<PikoSessionSidecar | null> {
    const rootSessionPath = this.meta.path;
    if (options.create) {
      const sidecar = await PikoSessionSidecar.openOrCreate({
        rootSessionId: this.meta.id,
        rootSessionPath,
      });
      await this.ensureMainAgentRecord(sidecar);
      return sidecar;
    }
    return PikoSessionSidecar.openIfExists({
      rootSessionId: this.meta.id,
      rootSessionPath,
    });
  }

  private async ensureMainAgentRecord(sidecar: PikoSessionSidecar): Promise<void> {
    const records = await sidecar.records();
    const hasMain = records.some(
      (record) =>
        record.schema === "piko.agent_session.v1" &&
        record.agentId === "main" &&
        record.agentSessionId === this.meta.id,
    );
    if (hasMain) return;
    await sidecar.append({
      schema: "piko.agent_session.v1",
      rootSessionId: this.meta.id,
      agentId: "main",
      agentSessionId: this.meta.id,
      sessionPath: this.meta.path,
      kind: "main",
      displayName: "Main",
      persistence: "session",
      createdAt: this.meta.createdAt,
    } satisfies AgentSessionRecord);
  }

  async createAgentSession(
    agentId: string,
    options: {
      displayName?: string;
      role?: string;
      persistence?: AgentPersistencePolicy;
    } = {},
  ): Promise<SessionManager> {
    if (agentId === "main") return this;

    const persistence = options.persistence ?? defaultSubagentPersistence;
    if (persistence.kind === "ephemeral") {
      throw new Error(`Cannot create persistent session for ephemeral agent: ${agentId}`);
    }

    const sidecar = await this.getSidecar({ create: true });
    if (!sidecar) throw new Error("Failed to create session sidecar");

    const agentSessionId = createSessionId();
    const createdAt = createTimestamp();
    const agentDir = joinPath(
      attachedSessionsDirForSessionFile(this.meta.path),
      "agents",
      sanitizeAgentId(agentId),
    );
    await mkdirp(agentDir);
    const filePath = joinPath(
      agentDir,
      `${createdAt.replace(/[:.]/g, "-")}_${agentSessionId}.jsonl`,
    );

    const env = makeSessionEnv(this.meta.cwd);
    const storage = await JsonlSessionStorage.create(env, filePath, {
      cwd: this.meta.cwd,
      sessionId: agentSessionId,
      parentSessionPath: this.meta.path,
    });
    const session = new Session(storage);
    const meta = await session.getMetadata();
    const leafId = await session.getLeafId();

    await sidecar.append({
      schema: "piko.agent_session.v1",
      rootSessionId: this.meta.id,
      agentId,
      agentSessionId,
      sessionPath: meta.path,
      kind: "subagent",
      displayName: options.displayName,
      role: options.role,
      persistence: "session",
      createdAt,
    } satisfies AgentSessionRecord);

    return new SessionManager(session, this.repo, meta, leafId);
  }

  async openAgentSession(agentSessionId: string): Promise<SessionManager | null> {
    if (agentSessionId === this.meta.id) return this;
    const sidecar = await this.getSidecar({ create: false });
    if (!sidecar) return null;
    const records = await sidecar.records();
    const record = records
      .filter(
        (r): r is AgentSessionRecord =>
          r.schema === "piko.agent_session.v1" && r.agentSessionId === agentSessionId,
      )
      .at(-1);
    if (!record) return null;
    const session = await this.repo.open({
      id: record.agentSessionId,
      createdAt: record.createdAt,
      cwd: this.meta.cwd,
      path: record.sessionPath,
      parentSessionPath: this.meta.path,
    });
    const meta = await session.getMetadata();
    const leafId = await session.getLeafId();
    return new SessionManager(session, this.repo, meta, leafId);
  }

  async appendAgentTask(
    record: Omit<AgentTaskRecord, "schema" | "rootSessionId" | "createdAt"> & {
      createdAt?: string;
    },
  ): Promise<void> {
    const sidecar = await this.getSidecar({ create: true });
    if (!sidecar) throw new Error("Failed to create session sidecar");
    await sidecar.append({
      schema: "piko.agent_task.v1",
      rootSessionId: this.meta.id,
      createdAt: record.createdAt ?? new Date().toISOString(),
      ...record,
    });
  }

  async updateAgentTaskStatus(
    taskId: string,
    status: AgentTaskRecord["status"],
    details: { summary?: string; error?: string; completedAt?: string } = {},
  ): Promise<void> {
    const sidecar = await this.getSidecar({ create: false });
    if (!sidecar) throw new Error(`No session sidecar for task ${taskId}`);
    const task = (await this.loadTaskTree()).find((record) => record.taskId === taskId);
    if (!task) throw new Error(`Agent task not found: ${taskId}`);
    await sidecar.append({
      ...task,
      status,
      summary: details.summary ?? task.summary,
      error: details.error ?? task.error,
      completedAt:
        details.completedAt ??
        (status === "completed" || status === "failed" || status === "cancelled"
          ? new Date().toISOString()
          : task.completedAt),
    } satisfies AgentTaskRecord);
  }

  async appendAgentRuntimeEvent(
    record: Omit<AgentRuntimeEventRecord, "schema" | "rootSessionId" | "eventId" | "timestamp"> & {
      eventId?: string;
      timestamp?: string;
    },
  ): Promise<void> {
    const sidecar = await this.getSidecar({ create: true });
    if (!sidecar) throw new Error("Failed to create session sidecar");
    await sidecar.append({
      schema: "piko.agent_runtime_event.v1",
      rootSessionId: this.meta.id,
      eventId: record.eventId ?? createSessionId(),
      timestamp: record.timestamp ?? new Date().toISOString(),
      ...record,
    });
  }

  async loadAgentSessions(): Promise<AgentSessionRecord[]> {
    const sidecar = await this.getSidecar({ create: false });
    if (!sidecar) return [];
    const records = await sidecar.records();
    return records.filter(
      (record): record is AgentSessionRecord => record.schema === "piko.agent_session.v1",
    );
  }

  async loadTaskTree(): Promise<AgentTaskRecord[]> {
    const sidecar = await this.getSidecar({ create: false });
    if (!sidecar) return [];
    const latest = new Map<string, AgentTaskRecord>();
    for (const record of await sidecar.records()) {
      if (record.schema === "piko.agent_task.v1") {
        latest.set(record.taskId, record);
      }
    }
    return [...latest.values()];
  }

  async loadRuntimeEvents(taskId: string): Promise<AgentRuntimeEventRecord[]> {
    const sidecar = await this.getSidecar({ create: false });
    if (!sidecar) return [];
    const records = await sidecar.records();
    return records.filter(
      (record): record is AgentRuntimeEventRecord =>
        record.schema === "piko.agent_runtime_event.v1" && record.taskId === taskId,
    );
  }

  async loadTaskTranscript(taskId: string): Promise<Message[]> {
    const task = (await this.loadTaskTree()).find((record) => record.taskId === taskId);
    if (!task) return [];
    const session = await this.openAgentSession(task.agentSessionId);
    return session ? session.loadMessages() : [];
  }

  async loadPersistenceOverview(): Promise<SessionPersistenceOverview> {
    const [messages, agentSessions, tasks] = await Promise.all([
      this.loadMessages(),
      this.loadAgentSessions(),
      this.loadTaskTree(),
    ]);
    const subagentIds = new Set(
      agentSessions.filter((record) => record.kind === "subagent").map((record) => record.agentId),
    );
    return {
      rootSessionId: this.meta.id,
      rootSessionPath: this.meta.path,
      mainMessageCount: messages.length,
      hasSidecar: agentSessions.length > 0 || tasks.length > 0,
      agentSessions,
      tasks,
      subagentCount: subagentIds.size,
      taskCount: tasks.length,
    };
  }

  async setSessionName(name?: string): Promise<void> {
    if (name?.trim()) await this.session.appendSessionName(name.trim());
  }

  // ---- Runtime state persistence (model / thinking changes) ----

  async appendModelChange(provider: string, modelId: string): Promise<string> {
    return this.session.appendModelChange(provider, modelId);
  }

  async appendThinkingLevelChange(level: string): Promise<string> {
    return this.session.appendThinkingLevelChange(level);
  }

  async appendActiveToolsChange(toolNames: string[]): Promise<string> {
    return this.session.appendActiveToolsChange(toolNames);
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

  async navigateToEntry(entryId: string): Promise<{ editorText?: string }> {
    const entry = await this.session.getEntry(entryId);
    if (!entry) throw new Error(`Entry ${entryId} not found`);

    let newLeafId: string | null = entryId;
    let editorText: string | undefined;

    if (entry.type === "message" && entry.message.role === "user") {
      newLeafId = entry.parentId;
      const content = entry.message.content;
      editorText =
        typeof content === "string"
          ? content
          : Array.isArray(content)
            ? content
                .filter((part): part is { type: "text"; text: string } => part.type === "text")
                .map((part) => part.text)
                .join("\n")
            : "";
    }

    await this.session.moveTo(newLeafId);
    this._leafId = newLeafId;
    return { editorText };
  }

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

  newSession(_options: { parentSession?: string } = {}): void {
    // This is used by PikoSessionRuntime to create a fresh in-memory session.
    // The actual creation happens lazily on first save. We just reset state.
  }

  async reopen(handle: SessionHandle): Promise<void> {
    const repo = makeSessionRepo(handle.cwd);
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
