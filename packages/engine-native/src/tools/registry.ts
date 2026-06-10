import type { NativeToolRegistry } from "piko-engine-native";
import type { EngineTool } from "piko-engine-protocol";
import { builtinToolSet } from "piko-engine-protocol";
import { applyPatchTool } from "./apply-patch/index.js";
import { shellTool } from "./shell.js";

export interface BuiltinToolSet {
  definitions: EngineTool[];
  registry: NativeToolRegistry;
}

/** Default built-in tool set: definitions from protocol, implementations from engine-native. */
export function createBuiltinCodingToolSet(cwd: string = process.cwd()): BuiltinToolSet {
  return {
    definitions: builtinToolSet.tools,
    registry: {
      shell: (args) => shellTool(cwd, args),
      apply_patch: (args) => applyPatchTool(cwd, args),
    },
  };
}
