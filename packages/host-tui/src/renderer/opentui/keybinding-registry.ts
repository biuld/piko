// ============================================================================
// Keybinding Registry — centralized command/key mappings
// Replaces scattered keyboard conditionals across components.
// ============================================================================

// ============================================================================
// Focus regions
// ============================================================================

export type FocusRegion = "editor" | "chat" | "overlay" | "confirm";

// ============================================================================
// Command IDs
// ============================================================================

export type CommandId =
  | "submit"
  | "abort"
  | "quit"
  | "openModel"
  | "openThinking"
  | "openResume"
  | "openSettings"
  | "openLogin"
  | "openHelp"
  | "closeOverlay"
  | "selectNext"
  | "selectPrevious"
  | "toggleExpanded"
  | "scrollUp"
  | "scrollDown"
  | "newline"
  | "clearEditor";

// ============================================================================
// Key combination
// ============================================================================

export interface KeyCombo {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  meta?: boolean;
}

// ============================================================================
// Slash command
// ============================================================================

export interface SlashCommand {
  /** Command name with leading slash, e.g. "/model" */
  name: string;
  /** Aliases, e.g. ["/m"] */
  aliases?: string[];
  /** Description for help overlay */
  description: string;
  /** Which focus regions this command is available in */
  regions: FocusRegion[];
  /** The command to dispatch */
  command: CommandId;
  /** Whether the command requires the stream to be idle */
  requiresIdle?: boolean;
}

// ============================================================================
// Keybinding entry
// ============================================================================

export interface KeybindingEntry {
  /** The command this keybinding triggers */
  command: CommandId;
  /** Human-readable hint for bottom bar / overlay footer */
  hint: string;
  /** Which focus regions this keybinding is active in */
  regions: FocusRegion[];
  /** The key combination */
  keys: KeyCombo;
  /** Whether the command requires the stream to be idle */
  requiresIdle?: boolean;
}

// ============================================================================
// Registry
// ============================================================================

class KeybindingRegistry {
  private bindings: KeybindingEntry[] = [];
  private slashCommands: SlashCommand[] = [];

  register(binding: KeybindingEntry): void {
    // Replace existing binding for same command+region
    const idx = this.bindings.findIndex(
      (b) => b.command === binding.command && arraysEqual(b.regions, binding.regions),
    );
    if (idx >= 0) {
      this.bindings[idx] = binding;
    } else {
      this.bindings.push(binding);
    }
  }

  registerSlash(cmd: SlashCommand): void {
    const idx = this.slashCommands.findIndex((s) => s.name === cmd.name);
    if (idx >= 0) {
      this.slashCommands[idx] = cmd;
    } else {
      this.slashCommands.push(cmd);
    }
  }

  /** Find a matching keybinding for a key event */
  findBinding(
    keyName: string,
    ctrl: boolean,
    shift: boolean,
    alt: boolean,
    meta: boolean,
    region: FocusRegion,
    isIdle: boolean,
  ): KeybindingEntry | undefined {
    return this.bindings.find((b) => {
      if (!b.regions.includes(region)) return false;
      if (b.keys.key !== keyName) return false;
      if ((b.keys.ctrl ?? false) !== ctrl) return false;
      if ((b.keys.shift ?? false) !== shift) return false;
      if ((b.keys.alt ?? false) !== alt) return false;
      if ((b.keys.meta ?? false) !== meta) return false;
      if (b.requiresIdle && !isIdle) return false;
      return true;
    });
  }

  /** Find a slash command by text */
  findSlash(text: string): SlashCommand | undefined {
    const t = text.trim().toLowerCase();
    return this.slashCommands.find(
      (s) => s.name.toLowerCase() === t || s.aliases?.some((a) => a.toLowerCase() === t),
    );
  }

  /** Get hints for a focus region */
  getHints(region: FocusRegion, width: number): string[] {
    const regionBindings = this.bindings.filter((b) => b.regions.includes(region));
    // Sort by command priority for consistent display
    const hints = regionBindings.map((b) => b.hint);
    return packHints(hints, width);
  }

  /** List all slash commands for help overlay */
  listSlashCommands(): SlashCommand[] {
    return [...this.slashCommands];
  }

  /** List all keybindings for help overlay */
  listBindings(): KeybindingEntry[] {
    return [...this.bindings];
  }
}

// ============================================================================
// Default bindings
// ============================================================================

export function createDefaultRegistry(): KeybindingRegistry {
  const reg = new KeybindingRegistry();

  // ---- Global ----
  reg.register({
    command: "abort",
    hint: "^C abort",
    regions: ["editor", "chat", "overlay"],
    keys: { key: "c", ctrl: true },
  });
  reg.register({
    command: "quit",
    hint: "^D quit",
    regions: ["editor", "chat"],
    keys: { key: "d", ctrl: true },
    requiresIdle: true,
  });

  // ---- Editor ----
  reg.register({
    command: "submit",
    hint: "Enter submit",
    regions: ["editor"],
    keys: { key: "enter" },
  });
  reg.register({
    command: "newline",
    hint: "Shift+Enter newline",
    regions: ["editor"],
    keys: { key: "enter", shift: true },
  });
  reg.register({
    command: "openModel",
    hint: "^P model",
    regions: ["editor"],
    keys: { key: "p", ctrl: true },
  });
  reg.register({
    command: "openThinking",
    hint: "^T thinking",
    regions: ["editor"],
    keys: { key: "t", ctrl: true },
  });
  reg.register({
    command: "openResume",
    hint: "^R resume",
    regions: ["editor"],
    keys: { key: "r", ctrl: true },
    requiresIdle: true,
  });

  // ---- Overlay ----
  reg.register({
    command: "closeOverlay",
    hint: "Esc close",
    regions: ["overlay"],
    keys: { key: "escape" },
  });
  reg.register({
    command: "selectNext",
    hint: "↓ next",
    regions: ["overlay"],
    keys: { key: "down" },
  });
  reg.register({
    command: "selectPrevious",
    hint: "↑ prev",
    regions: ["overlay"],
    keys: { key: "up" },
  });

  // ---- Slash commands ----
  reg.registerSlash({
    name: "/model",
    aliases: ["/m"],
    description: "Open model selector",
    regions: ["editor"],
    command: "openModel",
    requiresIdle: true,
  });
  reg.registerSlash({
    name: "/thinking",
    description: "Open thinking level selector",
    regions: ["editor"],
    command: "openThinking",
    requiresIdle: true,
  });
  reg.registerSlash({
    name: "/resume",
    description: "Open resume session selector",
    regions: ["editor"],
    command: "openResume",
    requiresIdle: true,
  });
  reg.registerSlash({
    name: "/settings",
    description: "Open settings",
    regions: ["editor"],
    command: "openSettings",
    requiresIdle: true,
  });
  reg.registerSlash({
    name: "/login",
    description: "Login to provider",
    regions: ["editor"],
    command: "openLogin",
    requiresIdle: true,
  });
  reg.registerSlash({
    name: "/help",
    aliases: ["/h", "/?"],
    description: "Show help",
    regions: ["editor"],
    command: "openHelp",
  });
  reg.registerSlash({
    name: "/exit",
    aliases: ["/quit", "/q"],
    description: "Exit piko",
    regions: ["editor"],
    command: "quit",
  });

  return reg;
}

// ============================================================================
// Helpers
// ============================================================================

function arraysEqual(a: unknown[], b: unknown[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((v, i) => v === b[i]);
}

/**
 * Pack hint strings into the available width.
 * Drops rightmost hints until they fit.
 */
function packHints(hints: string[], width: number): string[] {
  if (hints.length === 0) return [];

  const sep = "  ";
  const result = [...hints];
  let totalLen = result.reduce((sum, h) => sum + h.length, 0) + (result.length - 1) * sep.length;

  while (totalLen > width && result.length > 1) {
    const removed = result.pop()!;
    totalLen -= removed.length + sep.length;
  }

  return result;
}
