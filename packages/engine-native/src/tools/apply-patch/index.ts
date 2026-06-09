import { applyOperations } from "./applier.js";
import { PatchParseError, parsePatch } from "./parser.js";

export interface ApplyPatchResult {
  applied: boolean;
  filesAdded: string[];
  filesUpdated: string[];
  filesDeleted: string[];
  filesMoved: Array<{ from: string; to: string }>;
  hunksApplied: number;
  errors?: string[];
}

/**
 * Main apply_patch tool entry point.
 */
export async function applyPatchTool(
  cwd: string,
  args: Record<string, unknown>,
): Promise<ApplyPatchResult> {
  const patch = typeof args.patch === "string" ? args.patch : undefined;
  if (!patch) throw new Error("apply_patch requires a string 'patch' argument");

  if (!patch.trim().startsWith("*** Begin Patch")) {
    throw new Error("Patch must start with '*** Begin Patch'");
  }

  let operations: ReturnType<typeof parsePatch>;
  try {
    operations = parsePatch(patch);
  } catch (err) {
    if (err instanceof PatchParseError) {
      throw new Error(`Patch parse error: ${err.message}`);
    }
    throw err;
  }

  const result = await applyOperations(cwd, operations.operations);

  return {
    applied: result.errors.length === 0,
    filesAdded: result.filesAdded,
    filesUpdated: result.filesUpdated,
    filesDeleted: result.filesDeleted,
    filesMoved: [], // Not yet supported
    hunksApplied: result.hunksApplied,
    ...(result.errors.length > 0 ? { errors: result.errors } : {}),
  };
}
