// ============================================================================
// CommandRegistry — centralized command registration and dispatch
// ============================================================================

import type { KeybindingId } from "../keymap/types.js";
import type {
  AutocompleteItem,
  CommandAvailability,
  CommandAvailabilityState,
  CommandContext,
  CommandDefinition,
} from "./types.js";

export class CommandRegistry {
  private commands: Map<string, CommandDefinition> = new Map();
  private keybindingToCommand: Map<KeybindingId, string> = new Map();
  private slashToCommand: Map<string, string> = new Map();

  /**
   * Register a command. If a command with the same ID exists, it is replaced.
   */
  register(cmd: CommandDefinition): void {
    this.commands.set(cmd.id, cmd);

    // Index keybindings
    if (cmd.keybindings) {
      for (const kb of cmd.keybindings) {
        this.keybindingToCommand.set(kb, cmd.id);
      }
    }

    // Index slash commands
    if (cmd.slash) {
      this.slashToCommand.set(cmd.slash.name.toLowerCase(), cmd.id);
      if (cmd.slash.aliases) {
        for (const alias of cmd.slash.aliases) {
          this.slashToCommand.set(alias.toLowerCase(), cmd.id);
        }
      }
    }
  }

  /**
   * Register multiple commands.
   */
  registerAll(cmds: CommandDefinition[]): void {
    for (const cmd of cmds) {
      this.register(cmd);
    }
  }

  /**
   * Find a command by keybinding ID.
   */
  findByKeybinding(id: KeybindingId): CommandDefinition | undefined {
    const commandId = this.keybindingToCommand.get(id);
    if (!commandId) return undefined;
    return this.commands.get(commandId);
  }

  /**
   * Find a command by slash text (e.g., "/model").
   */
  findBySlash(text: string): CommandDefinition | undefined {
    const trimmed = text.trim().toLowerCase();
    const commandId = this.slashToCommand.get(trimmed);
    if (!commandId) return undefined;
    return this.commands.get(commandId);
  }

  /**
   * Get a command by ID.
   */
  get(id: string): CommandDefinition | undefined {
    return this.commands.get(id);
  }

  /**
   * Get all slash commands for autocomplete.
   */
  listSlashCommands(): Array<{
    name: string;
    aliases?: string[];
    description: string;
    argumentHint?: string;
    commandId: string;
  }> {
    const result: Array<{
      name: string;
      aliases?: string[];
      description: string;
      argumentHint?: string;
      commandId: string;
    }> = [];
    for (const cmd of this.commands.values()) {
      if (cmd.slash) {
        result.push({
          name: cmd.slash.name,
          aliases: cmd.slash.aliases,
          description: cmd.slash.description,
          argumentHint: cmd.slash.argumentHint,
          commandId: cmd.id,
        });
      }
    }
    return result;
  }

  /**
   * Get all commands for display.
   */
  listCommands(): Array<{ id: string; description: string }> {
    return [...this.commands.values()].map((cmd) => ({
      id: cmd.id,
      description: cmd.slash?.description ?? cmd.id,
    }));
  }

  /**
   * Get argument completions for a slash command.
   */
  async getArgumentCompletions(
    slashText: string,
    prefix: string,
  ): Promise<AutocompleteItem[] | null> {
    const cmd = this.findBySlash(slashText.split(" ")[0]);
    if (!cmd?.slash?.getArgumentCompletions) return null;
    return cmd.slash.getArgumentCompletions(prefix);
  }

  /**
   * Check if a command is available given current state.
   */
  checkAvailability(commandId: string, state: CommandAvailabilityState): CommandAvailability {
    const cmd = this.commands.get(commandId);
    if (!cmd) return { available: false, reason: `Unknown command: ${commandId}` };
    if (cmd.availability) return cmd.availability(state);
    if (cmd.requiresIdle && state.isStreamRunning) {
      return { available: false, reason: "Command unavailable while running" };
    }
    return { available: true };
  }

  /**
   * Execute a command by ID.
   */
  async execute(commandId: string, ctx: CommandContext, args?: string): Promise<void> {
    const cmd = this.commands.get(commandId);
    if (!cmd) {
      ctx.notify(`Unknown command: ${commandId}`, "error");
      return;
    }
    await cmd.run(ctx, args);
  }

  /**
   * Execute a slash command by text (e.g., "/model gpt-4").
   * Returns true if a command was found and executed.
   */
  async executeSlash(slashText: string, ctx: CommandContext): Promise<boolean> {
    const parts = slashText.trim().split(/\s+/);
    const cmdText = parts[0] ?? "";
    const args = parts.slice(1).join(" ");

    const cmd = this.findBySlash(cmdText);
    if (!cmd) return false;

    await cmd.run(ctx, args);
    return true;
  }
}
