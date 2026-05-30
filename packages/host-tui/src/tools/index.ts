import { bashDef } from "./bash.js";
import { editDef, findDef, grepDef, lsDef, writeDef } from "./defs.js";
import { readDef } from "./read.js";
import type { ToolDef } from "./types.js";

const ALL_TOOLS: Record<string, ToolDef> = {
  bash: bashDef,
  read: readDef,
  write: writeDef,
  edit: editDef,
  grep: grepDef,
  find: findDef,
  ls: lsDef,
};

/** Get a built-in tool definition by name */
export function getToolDef(name: string): ToolDef | undefined {
  return ALL_TOOLS[name];
}

export { ToolBlock } from "./tool-block.js";
export type { ToolDef, ToolRenderContext, ToolRenderResultOptions } from "./types.js";
