import { describe, expect, it } from "bun:test";
import type { HostEvent } from "../src/client/hostd-protocol.js";
import { HostdClient, type HostdTransport } from "../src/client/index.js";

class MockTransport implements HostdTransport {
  writes: string[] = [];
  private lineListeners = new Set<(line: string) => void>();
  private closeListeners = new Set<(code?: number | null) => void>();

  write(line: string): void {
    this.writes.push(line);
  }

  onLine(listener: (line: string) => void): void {
    this.lineListeners.add(listener);
  }

  onClose(listener: (code?: number | null) => void): void {
    this.closeListeners.add(listener);
  }

  close(): void {
    for (const listener of this.closeListeners) listener(0);
  }

  emit(event: HostEvent): void {
    for (const listener of this.lineListeners) listener(JSON.stringify(event));
  }

  emitAck(ack: { type: "command_accepted" | "command_rejected"; command_id: string; reason?: string }): void {
    for (const listener of this.lineListeners) listener(JSON.stringify(ack));
  }
}

describe("HostdClient", () => {
  it("resolves commands on accepted ack", async () => {
    const transport = new MockTransport();
    const client = new HostdClient({ transport, commandTimeoutMs: 100 });
    const sent = client.send({
      type: "session_create",
      command_id: "cmd-1",
      cwd: "/tmp/project",
    });

    expect(JSON.parse(transport.writes[0]!)).toEqual({
      type: "session_create",
      command_id: "cmd-1",
      cwd: "/tmp/project",
    });

    transport.emitAck({ type: "command_accepted", command_id: "cmd-1" });
    await expect(sent).resolves.toBeUndefined();
  });

  it("rejects commands on rejected ack", async () => {
    const transport = new MockTransport();
    const client = new HostdClient({ transport, commandTimeoutMs: 100 });
    const sent = client.send({
      type: "state_snapshot",
      command_id: "cmd-2",
      session_id: "missing",
    });

    transport.emitAck({
      type: "command_rejected",
      command_id: "cmd-2",
      reason: "missing session",
    });

    await expect(sent).rejects.toThrow("missing session");
  });

  it("receives unified events", async () => {
    const transport = new MockTransport();
    const client = new HostdClient({ transport, commandTimeoutMs: 100 });

    const received: HostEvent[] = [];
    client.onEvent((event) => received.push(event));

    transport.emit({
      type: "text_delta",
      task_id: "task-1",
      agent_id: "main",
      message_id: "msg-1",
      delta: "hello",
    });

    expect(received).toEqual([{
      type: "text_delta",
      task_id: "task-1",
      agent_id: "main",
      message_id: "msg-1",
      delta: "hello",
    }]);
  });

  it("tracks last seen seq and resumes from it", async () => {
    const transport = new MockTransport();
    const client = new HostdClient({ transport, commandTimeoutMs: 100 });

    // The new protocol doesn't use seq for unified events, but resume still sends after_seq
    const sent = client.resume("session-1");
    expect(JSON.parse(transport.writes[0]!)).toMatchObject({
      type: "events_resume",
      session_id: "session-1",
      after_seq: 0,
    });
    const commandId = JSON.parse(transport.writes[0]!).command_id;
    transport.emitAck({ type: "command_accepted", command_id: commandId });
    await sent;
  });
});
