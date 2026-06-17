import type { JsonlSessionMetadata } from "piko-session";
import { JsonlSessionRepo, type MessageEntry, type Session } from "piko-session";
import { NodeExecutionEnv } from "./nodejs-fs.js";
import { getSessionsDir } from "./session-paths.js";
import type { SessionMeta } from "./session-types.js";

export function makeSessionEnv(cwd: string) {
  return new NodeExecutionEnv({ cwd });
}

export function makeSessionRepo(cwd: string) {
  return new JsonlSessionRepo({ fs: makeSessionEnv(cwd), sessionsRoot: getSessionsDir() });
}

export async function extractSessionInfo(session: Session<JsonlSessionMetadata>): Promise<{
  name?: string;
  messageCount: number;
  preview: string;
  modified: string;
}> {
  const entries = await session.getStorage().getEntries();
  let messageCount = 0;
  let firstUserMessage = "";
  let name: string | undefined;
  let lastTimestamp = "";

  for (const entry of entries) {
    lastTimestamp = entry.timestamp;

    if (entry.type === "message") {
      messageCount++;
      const msg = (entry as MessageEntry).message;
      if (!firstUserMessage && "role" in msg && msg.role === "user") {
        const content = (msg as { content: unknown }).content;
        if (typeof content === "string") {
          firstUserMessage = content;
        } else if (Array.isArray(content)) {
          for (const part of content) {
            if (typeof part === "object" && part !== null && "text" in part) {
              const text = (part as { text: unknown }).text;
              if (typeof text === "string") {
                firstUserMessage = text;
                break;
              }
            }
          }
        }
      }
    } else if (entry.type === "session_info") {
      const infoName = (entry as { name?: string }).name?.trim();
      if (infoName) name = infoName;
    }
  }

  return {
    name,
    messageCount,
    preview: firstUserMessage || "",
    modified: lastTimestamp || "",
  };
}

export async function listSessionMetas(options: {
  cwd?: string;
  all?: boolean;
}): Promise<SessionMeta[]> {
  const repo = makeSessionRepo(options.cwd ?? process.cwd());
  const list = await repo.list(options.all ? {} : { cwd: options.cwd ?? process.cwd() });
  const results: SessionMeta[] = [];

  for (const meta of list) {
    try {
      const session = await repo.open(meta);
      const info = await extractSessionInfo(session);
      results.push({
        id: meta.id,
        path: meta.path,
        cwd: meta.cwd,
        created: meta.createdAt,
        modified: info.modified || meta.createdAt,
        model: "",
        messageCount: info.messageCount,
        preview: info.preview,
        name: info.name,
      });
    } catch {
      results.push({
        id: meta.id,
        path: meta.path,
        cwd: meta.cwd,
        created: meta.createdAt,
        modified: meta.createdAt,
        model: "",
        messageCount: 0,
        preview: "",
      });
    }
  }

  return results;
}
