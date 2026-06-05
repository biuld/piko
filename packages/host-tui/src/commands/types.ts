// ============================================================================
// Command types — command definitions, metadata, slash command dispatch
// ============================================================================

import type { KeybindingId } from "../keymap/types.js";
import type { PanelSurfaceRequest } from "../surfaces/types.js";

export interface CommandDefinition {
  /** Unique command identifier */
  id: string;
  /** Slash command metadata (if this command has a slash alias) */
  slash?: {
    name: string; // e.g. "/model"
    aliases?: string[]; // e.g. ["/m"]
    description: string;
    argumentHint?: string;
    getArgumentCompletions?: (prefix: string) => Promise<AutocompleteItem[] | null>;
  };
  /** Keybindings that trigger this command */
  keybindings?: KeybindingId[];
  /** Whether the command requires the stream to be idle */
  requiresIdle?: boolean;
  /** Whether the command is currently available */
  availability?: (state: CommandAvailabilityState) => CommandAvailability;
  /** Execute the command */
  run: (ctx: CommandContext, args?: string) => void | Promise<void>;
}

export interface AutocompleteItem {
  value: string;
  label: string;
  description?: string;
}

export interface CommandAvailabilityState {
  isStreamRunning: boolean;
  hasSession: boolean;
}

export type CommandAvailability = { available: true } | { available: false; reason: string };

export interface CommandContext {
  /** Open a surface by ID */

  /** Open a panel surface, returns the new surface ID */
  openPanel: (request: PanelSurfaceRequest) => string;
  /** Close a surface or all surfaces */
  closeSurface: (id?: string) => void;
  /** Notify the user */
  notify: (message: string, severity?: "info" | "success" | "warning" | "error") => void;
  /** Current state snapshot */
  getState: () => any;
  /** Execute another command by ID */
  executeCommand: (commandId: string, args?: string) => void;
  /** Dispatch a store event (for commands that need to modify state directly) */
  dispatch: (event: any) => void;
  /** Shutdown (exit) the application */
  shutdown: () => void;
  /** Abort the current stream */
  abort: () => void;
  /** Access to host runtime for session operations */
  host: any;
  /** Switch model through the ActionService (with ModelRegistry resolution) */
  switchModel: (modelId: string, provider: string) => boolean;
}
