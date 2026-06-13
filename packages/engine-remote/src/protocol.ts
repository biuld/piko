export interface RemoteTransport {
  send(method: string, params: unknown): Promise<unknown>;
  onNotification(handler: (method: string, params: unknown) => void): () => void;
  close(): Promise<void>;
}

export interface RemoteEngineOptions {
  transport: RemoteTransport;
}

/**
 * JSON-RPC methods:
 *   engine/execute_step  -> executeStep()
 *   engine/shutdown       -> shutdown()
 *
 * Notifications (server -> client):
 *   engine/event          -> event stream
 */
export const REMOTE_METHODS = {
  EXECUTE_STEP: "engine/execute_step",
  SHUTDOWN: "engine/shutdown",
  EVENT: "engine/event",
} as const;
