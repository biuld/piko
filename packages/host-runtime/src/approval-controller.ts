// ---- Approval handlers — factories for ToolApprovalHandler (used by Orchestrator) ----

import type { ToolApprovalHandler } from "./host/types.js";

/** Auto-accept approval handler for non-interactive mode. */
export function createAutoAcceptHandler(): ToolApprovalHandler {
  return async (_request) => "accept";
}

/** Approval handler that always declines. */
export function createAutoDeclineHandler(): ToolApprovalHandler {
  return async (_request) => "decline";
}
