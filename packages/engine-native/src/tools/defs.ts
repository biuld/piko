import type { NativeToolRegistry } from "piko-engine-native";
import type { ToolDef } from "piko-protocol";
import { applyPatchTool } from "./apply-patch/index.js";
import { shellTool } from "./shell.js";

/** Built-in native tool definitions — the canonical ToolDef for engine-native tools. */
export const nativeToolDefs: ToolDef[] = [
  {
    name: "shell",
    description: "Execute a shell command in the workspace.",
    inputSchema: {
      type: "object",
      properties: {
        command: { type: "string", description: "Shell command to execute" },
        timeout: { type: "number", description: "Timeout in seconds" },
        cwd: { type: "string", description: "Working directory" },
        login: { type: "boolean", description: "Use login shell" },
      },
      required: ["command"],
    },
    executor: { kind: "native", target: "shell" },
    executionMode: "sequential",
    exposure: "direct",
    capabilities: ["execute_process", "read_workspace", "write_workspace"],
    approval: "always",
  },
  {
    name: "apply_patch",
    description: "Apply a structured patch to files.",
    inputSchema: {
      type: "object",
      properties: {
        patch: { type: "string", description: "Patch content" },
      },
      required: ["patch"],
    },
    executor: { kind: "native", target: "apply_patch" },
    executionMode: "sequential",
    exposure: "direct",
    capabilities: ["write_workspace"],
    approval: "always",
  },
];

export interface BuiltinToolSet {
  definitions: ToolDef[];
  registry: NativeToolRegistry;
}

/** Default built-in tool set for native engine execution. */
export function createBuiltinCodingToolSet(cwd: string = process.cwd()): BuiltinToolSet {
  return {
    definitions: nativeToolDefs,
    registry: {
      shell: (args) => shellTool(cwd, args),
      apply_patch: (args) => applyPatchTool(cwd, args),
    },
  };
}
