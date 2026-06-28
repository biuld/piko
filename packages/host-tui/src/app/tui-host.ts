// ============================================================================
// TuiHostFacade — thin-client host API consumed by TUI renderer
//
// This is the ONLY interface the TUI uses to talk to the host backend.
// All session operations, turn execution, model config, and auth go through
// the wire to hostd. Methods NOT listed here belong to SessionHostPort
// (actions/session-actions.ts) or are handled by HostdActionAdapter directly.
//
// DO NOT ADD METHODS WITHOUT A CORRESPONDING hostd Command/Event pair.
// ============================================================================

import type { TuiHostConfig as HostConfig } from "./host-config.js";

export interface TuiHostFacade {
  // ---- Read-only identity ----
  readonly cwd: string;
  readonly sessionId: string;
  readonly sessionFile: string;
  readonly version: string;
  debugTracePath?: string;

  // ---- Model config (TUI ↔ hostd config_set) ----
  getConfig(): HostConfig;
  setConfig(config: HostConfig): void;
  getThinkingLevel(): string | undefined;
  setThinkingLevel(level: string): void;

  // ---- Lifecycle ----
  /** Restore host state (model, thinking, tools) from session log on startup. */
  restoreFromSession(): Promise<void>;

  // ---- Session metadata ----
  getSessionName(): Promise<string | null>;
  setSessionName(name?: string): Promise<void>;
  listSessions(): Promise<unknown[]>;
}

export type TuiHostConfig = HostConfig;
export type { FlatTreeEntry as TuiFlatTreeEntry } from "../shared/index.js";
