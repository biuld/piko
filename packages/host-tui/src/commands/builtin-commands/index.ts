import type { CommandDefinition } from "../types.js";
import { createAppCommands } from "./app.js";
import { createModelCommands } from "./model.js";
import { createPanelCommands } from "./panels.js";
import { createSessionCommands } from "./session.js";
import type { BuiltinCommandDeps } from "./types.js";

/**
 * Create all built-in commands and return them as an array.
 * The caller wires runtime dependencies via a factory so commands stay renderer-independent.
 */
export function createBuiltinCommands(deps: () => BuiltinCommandDeps): CommandDefinition[] {
  const ctx = () => deps();
  return [
    ...createModelCommands(ctx),
    ...createSessionCommands(ctx),
    ...createPanelCommands(ctx),
    ...createAppCommands(ctx),
  ];
}

export type { BuiltinCommandDeps } from "./types.js";
