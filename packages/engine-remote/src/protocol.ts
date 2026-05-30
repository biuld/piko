import type {
  EngineEventEnvelope,
  EngineInput,
  EngineStepResult,
  EngineApprovalResolution,
} from "piko-engine-protocol";

export interface RemoteTransport {
  send(method: string, params: unknown): Promise<unknown>;
  onNotification(handler: (method: string, params: unknown) => void): () => void;
  close(): Promise<void>;
}

export interface RemoteEngineOptions {
  transport: RemoteTransport;
}

/**
 * Maps the remote JSON-RPC protocol to the StatelessEngine interface.
 *
 * JSON-RPC methods:
 *   engine/execute_step  -> executeStep()
 *   engine/resolve_approval -> resolveApproval()
 *   engine/shutdown       -> shutdown()
 *
 * Notifications (server -> client):
 *   engine/event          -> event stream
 */
export const REMOTE_METHODS = {
  EXECUTE_STEP: "engine/execute_step",
  RESOLVE_APPROVAL: "engine/resolve_approval",
  SHUTDOWN: "engine/shutdown",
  EVENT: "engine/event",
} as const;
