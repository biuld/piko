// ============================================================================
// Command types — command definitions, metadata, slash command dispatch
// ============================================================================

import type { KeybindingId } from "../keymap/types.js";

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
  openSurface: (request: SurfaceRequest) => string;
  /** Close a surface or all surfaces */
  closeSurface: (id?: string) => void;
  /** Notify the user */
  notify: (message: string, severity?: "info" | "success" | "warning" | "error") => void;
  /** Current state snapshot */
  getState: () => any;
  /** Execute another command by ID */
  executeCommand: (commandId: string, args?: string) => void;
  /** Shutdown (exit) the application */
  shutdown: () => void;
  /** Abort the current stream */
  abort: () => void;
}

// Surface request (cross-ref with surfaces module)
export interface SurfaceRequest {
  role: "autocomplete" | "selector" | "menu" | "form" | "confirm" | "status";
  preferredMount?: "replace-slot" | "insert-between" | "anchored" | "side-drawer" | "status-line";
  targetSlot?: "app" | "timeline" | "editor" | "status" | "bottom-bar";
  contentSize?: "small" | "medium" | "large";
  requiresSecretInput?: boolean;
  destructive?: boolean;
  parentId?: string;
  anchorId?: string;
  data?: unknown;
}
