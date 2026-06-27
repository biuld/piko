// ============================================================================
// HostdClient — TUI ↔ hostd JSON-lines client
//
// Sends HostCommand requests, receives HostEvent + CommandAck.
// ============================================================================

import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import type { CommandAck, HostCommand, SessionId, HostEvent } from "./hostd-protocol.js";

export interface HostdTransport {
  write(line: string): void | Promise<void>;
  onLine(listener: (line: string) => void): void;
  onClose(listener: (code?: number | null) => void): void;
  close(): void | Promise<void>;
}

export interface HostdClientOptions {
  command?: string;
  args?: string[];
  transport?: HostdTransport;
  commandTimeoutMs?: number;
}

type PendingCommand = {
  resolve: () => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};

export class HostdClient {
  private readonly transport: HostdTransport;
  private readonly commandTimeoutMs: number;
  private pending = new Map<string, PendingCommand>();
  private listeners = new Set<(event: HostEvent) => void>();
  private lastSeqBySession = new Map<SessionId, number>();

  constructor(options: HostdClientOptions = {}) {
    this.transport =
      options.transport ?? spawnHostdTransport(options.command ?? "hostd", options.args ?? []);
    this.commandTimeoutMs = options.commandTimeoutMs ?? 10_000;
    this.transport.onLine((line) => this.handleLine(line));
    this.transport.onClose((code) => this.rejectAll(new Error(`hostd exited with code ${code}`)));
  }

  onEvent(listener: (event: HostEvent) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  getLastSeenSeq(sessionId: SessionId): number {
    return this.lastSeqBySession.get(sessionId) ?? 0;
  }

  async send(command: HostCommand): Promise<void> {
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(command.command_id);
        reject(new Error(`hostd command timed out: ${command.type}`));
      }, this.commandTimeoutMs);
      this.pending.set(command.command_id, { resolve, reject, timer });
      void this.transport.write(`${JSON.stringify(command)}\n`);
    });
  }

  resume(sessionId: SessionId): Promise<void> {
    return this.send({
      type: "events_resume",
      command_id: crypto.randomUUID(),
      session_id: sessionId,
      after_seq: this.getLastSeenSeq(sessionId),
    });
  }

  snapshot(sessionId: SessionId): Promise<void> {
    return this.send({
      type: "state_snapshot",
      command_id: crypto.randomUUID(),
      session_id: sessionId,
    });
  }

  async close(): Promise<void> {
    await this.transport.close();
    this.rejectAll(new Error("hostd client closed"));
  }

  private handleLine(line: string): void {
    if (!line.trim()) return;
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(line) as Record<string, unknown>;
    } catch {
      return;
    }

    // Command acks
    if (parsed.type === "command_accepted" || parsed.type === "command_rejected") {
      const ack = parsed as unknown as CommandAck;
      const pending = this.pending.get(ack.command_id);
      if (pending) {
        clearTimeout(pending.timer);
        this.pending.delete(ack.command_id);
        if (ack.type === "command_rejected") {
          pending.reject(new Error(ack.reason));
        } else {
          pending.resolve();
        }
      }
      return;
    }

    // HostEvent
    const event = parsed as unknown as HostEvent;
    for (const listener of this.listeners) listener(event);
  }

  private rejectAll(error: Error): void {
    for (const [commandId, pending] of this.pending) {
      clearTimeout(pending.timer);
      pending.reject(error);
      this.pending.delete(commandId);
    }
  }
}

export function spawnHostdTransport(command: string, args: string[]): HostdTransport {
  const child = spawn(command, args, { stdio: ["pipe", "pipe", "inherit"] });
  const lines = createInterface({ input: child.stdout });
  return {
    write(line) {
      child.stdin.write(line);
    },
    onLine(listener) {
      lines.on("line", listener);
    },
    onClose(listener) {
      child.on("close", listener);
    },
    close() {
      child.kill();
    },
  };
}
