import type { AgentTaskStatus } from "piko-orch-protocol";
import { mkdirp } from "../utils/bun-fs.js";
import { dirnamePath, joinPath, parsePath } from "../utils/bun-path.js";

export interface PikoSessionSidecarHeader {
  type: "piko_session_persistence";
  version: 1;
  rootSessionId: string;
  rootSessionPath: string;
  createdAt: string;
}

export type AgentTranscriptPolicy = "agent_reused" | "task_scoped";
export type AgentContextPolicy = "empty" | "agent_history" | "task_thread";

export type AgentPersistencePolicy =
  | { kind: "ephemeral" }
  | {
      kind: "session";
      transcript: AgentTranscriptPolicy;
      context: AgentContextPolicy;
    };

export interface AgentSessionRecord {
  schema: "piko.agent_session.v1";
  rootSessionId: string;
  agentId: string;
  agentSessionId: string;
  sessionPath: string;
  kind: "main" | "subagent";
  displayName?: string;
  role?: string;
  persistence: "session" | "ephemeral";
  createdAt: string;
}

export interface AgentTaskRecord {
  schema: "piko.agent_task.v1";
  rootSessionId: string;
  taskId: string;
  agentId: string;
  agentSessionId: string;
  parentTaskId?: string;
  sourceAgentId?: string;
  sourceTaskId?: string;
  promptEntryId?: string;
  anchorEntryId?: string;
  status: AgentTaskStatus;
  createdAt: string;
  completedAt?: string;
  summary?: string;
  error?: string;
}

export interface AgentRuntimeEventRecord {
  schema: "piko.agent_runtime_event.v1";
  rootSessionId: string;
  eventId: string;
  taskId: string;
  agentId: string;
  agentSessionId: string;
  anchorEntryId?: string;
  timestamp: string;
  event:
    | { type: "tool_started"; callId: string; name: string; args?: unknown }
    | { type: "tool_finished"; callId: string; name: string; result: unknown; isError: boolean }
    | { type: "approval_requested"; approvalId: string; toolName: string; toolArgs: unknown }
    | { type: "approval_resolved"; approvalId: string; decision: "accept" | "decline" };
}

export type PikoSessionSidecarRecord =
  | AgentSessionRecord
  | AgentTaskRecord
  | AgentRuntimeEventRecord;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function sidecarPathForSessionFile(sessionFile: string): string {
  const parsed = parsePath(sessionFile);
  return joinPath(parsed.dir, `${parsed.name}.piko.jsonl`);
}

export function attachedSessionsDirForSessionFile(sessionFile: string): string {
  const parsed = parsePath(sessionFile);
  return joinPath(parsed.dir, `${parsed.name}.piko`);
}

export function sanitizeAgentId(agentId: string): string {
  return agentId.replace(/[^a-zA-Z0-9._-]/g, "_") || "agent";
}

export class PikoSessionSidecar {
  readonly path: string;
  readonly header: PikoSessionSidecarHeader;

  private constructor(path: string, header: PikoSessionSidecarHeader) {
    this.path = path;
    this.header = header;
  }

  static async openOrCreate(options: {
    rootSessionId: string;
    rootSessionPath: string;
  }): Promise<PikoSessionSidecar> {
    const path = sidecarPathForSessionFile(options.rootSessionPath);
    const existing = await readHeader(path);
    if (existing) {
      if (existing.rootSessionId !== options.rootSessionId) {
        throw new Error(
          `Sidecar ${path} belongs to ${existing.rootSessionId}, not ${options.rootSessionId}`,
        );
      }
      return new PikoSessionSidecar(path, existing);
    }

    await mkdirp(dirnamePath(path));
    const header: PikoSessionSidecarHeader = {
      type: "piko_session_persistence",
      version: 1,
      rootSessionId: options.rootSessionId,
      rootSessionPath: options.rootSessionPath,
      createdAt: new Date().toISOString(),
    };
    await Bun.write(path, `${JSON.stringify(header)}\n`);
    return new PikoSessionSidecar(path, header);
  }

  static async openIfExists(options: {
    rootSessionId: string;
    rootSessionPath: string;
  }): Promise<PikoSessionSidecar | null> {
    const path = sidecarPathForSessionFile(options.rootSessionPath);
    const header = await readHeader(path);
    if (!header) return null;
    if (header.rootSessionId !== options.rootSessionId) return null;
    return new PikoSessionSidecar(path, header);
  }

  async append(record: PikoSessionSidecarRecord): Promise<void> {
    const existing = await Bun.file(this.path).text();
    await Bun.write(this.path, `${existing}${JSON.stringify(record)}\n`);
  }

  async records(): Promise<PikoSessionSidecarRecord[]> {
    return readRecords(this.path);
  }
}

async function readHeader(path: string): Promise<PikoSessionSidecarHeader | null> {
  let content: string;
  try {
    content = await Bun.file(path).text();
  } catch (error) {
    if (isNodeNotFound(error)) return null;
    throw error;
  }
  const first = content.split("\n").find((line) => line.trim());
  if (!first) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(first) as unknown;
  } catch {
    return null;
  }
  if (
    !isRecord(parsed) ||
    (parsed.type !== "piko_session_persistence" && parsed.type !== "piko_multi_agent_session")
  ) {
    return null;
  }
  if (
    parsed.version !== 1 ||
    typeof parsed.rootSessionId !== "string" ||
    typeof parsed.rootSessionPath !== "string" ||
    typeof parsed.createdAt !== "string"
  ) {
    return null;
  }
  return { ...parsed, type: "piko_session_persistence" } as PikoSessionSidecarHeader;
}

async function readRecords(path: string): Promise<PikoSessionSidecarRecord[]> {
  let content: string;
  try {
    content = await Bun.file(path).text();
  } catch (error) {
    if (isNodeNotFound(error)) return [];
    throw error;
  }
  const lines = content.split("\n").filter((line) => line.trim());
  const records: PikoSessionSidecarRecord[] = [];
  for (const line of lines.slice(1)) {
    try {
      const parsed = JSON.parse(line) as unknown;
      if (isSidecarRecord(parsed)) records.push(parsed);
    } catch {
      // Malformed sidecar records must not make the root session unreadable.
    }
  }
  return records;
}

function isSidecarRecord(value: unknown): value is PikoSessionSidecarRecord {
  return (
    isRecord(value) &&
    (value.schema === "piko.agent_session.v1" ||
      value.schema === "piko.agent_task.v1" ||
      value.schema === "piko.agent_runtime_event.v1")
  );
}

function isNodeNotFound(error: unknown): boolean {
  return error instanceof Error && "code" in error && error.code === "ENOENT";
}
