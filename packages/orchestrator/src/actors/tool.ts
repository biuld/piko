// ---- ToolActor — pure per-call tool executor ----
//
// Responsibilities:
//  - Receive one pre-resolved execute message with route info
//  - Check approval via ApprovalGateway
//  - Call provider.execute()
//  - Emit lifecycle events (tool_started, tool_finished, approval_resolved)
//  - Return structured ToolExecResult
//
// Discovery is handled by ToolRegistry.discoverTools() (direct call, not an actor).
// Each ToolActor instance handles exactly one tool call, then can be stopped.

import type {
  ApprovalGateway,
  ToolCall,
  ToolDef,
  ToolExecResult,
  ToolExecutionContext,
  ToolProvider,
} from "piko-orchestrator-protocol";
import type { ActorHandler } from "../kernel/actor-system.js";
import type { OrchestratorEvent } from "./state.js";

// Re-export for convenience
export type { ToolCall, ToolExecResult, ToolExecutionContext, ToolProvider };

// ---- Route info (passed from discovery to execution) ----

export interface CatalogRoute {
  providerId: string;
  providerToolName: string;
  toolDef: ToolDef;
}

// ---- Messages ----

export type ToolMsg =
  | {
      type: "execute";
      call: ToolCall;
      context: ToolExecutionContext;
      /** Pre-resolved route from ToolRegistry.discoverTools(). */
      route: CatalogRoute;
    }
  | { type: "cancel"; callId: string; reason?: string }
  | { type: "task_finished"; agentId: string; taskId: string };

// ---- ToolActor state ----

interface ToolActorState {
  /** Shared provider registry — facade pushes to this, ToolActor reads it directly. */
  providers: Map<string, ToolProvider>;
  /** Approval gateway — set once by facade, shared reference. */
  approvalGateway?: ApprovalGateway;
  /** Per-instance active call tracking. */
  activeCalls: Map<string, { call: ToolCall; context: ToolExecutionContext }>;
}

// ---- ToolActor handler ----

export function toolActor(
  state: ToolActorState,
  deps: {
    emit: (event: OrchestratorEvent) => Promise<void>;
  },
): ActorHandler<ToolMsg> {
  return async (msg, ctx, meta) => {
    switch (msg.type) {
      case "execute": {
        const { call, context, route } = msg;
        state.activeCalls.set(call.id, { call, context });

        await deps.emit({
          type: "tool_started",
          agentId: context.agentId,
          taskId: context.taskId,
          callId: call.id,
          name: call.name,
          args: call.arguments,
        });

        const provider = state.providers.get(route.providerId);
        if (!provider) {
          const result: ToolExecResult = {
            ok: false,
            error: {
              code: "not_found",
              message: `No provider "${route.providerId}" for tool "${call.name}"`,
            },
          };
          await emitToolFinished(deps, context, call.id, result);
          state.activeCalls.delete(call.id);
          ctx.reply(meta, result);
          return;
        }

        // Map alias → provider tool name
        const providerCall: ToolCall =
          route.providerToolName !== call.name ? { ...call, name: route.providerToolName } : call;

        // ---- Approval check ----
        const effectiveApproval = route.toolDef.approval ?? "never";
        if (effectiveApproval !== "never" && state.approvalGateway) {
          const needsApproval =
            effectiveApproval === "always" || effectiveApproval === "on_request";

          if (needsApproval) {
            const decision = await state.approvalGateway.requestToolApproval({
              callId: call.id,
              agentId: context.agentId,
              taskId: context.taskId,
              toolName: call.name, // public name
              toolArgs: providerCall.arguments,
            });

            if (decision === "decline") {
              const result: ToolExecResult = {
                ok: false,
                error: { code: "declined", message: "User declined approval" },
              };
              await deps.emit({
                type: "approval_resolved",
                approvalId: call.id,
                decision: "decline",
              });
              await emitToolFinished(deps, context, call.id, result);
              state.activeCalls.delete(call.id);
              ctx.reply(meta, result);
              return;
            }

            await deps.emit({
              type: "approval_resolved",
              approvalId: call.id,
              decision: "accept",
            });
          }
        }

        // ---- Execute ----
        try {
          const result = await provider.execute(providerCall, context);

          await emitToolFinished(deps, context, call.id, result);
          state.activeCalls.delete(call.id);
          ctx.reply(meta, result);
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message : String(err);
          const errorResult: ToolExecResult = {
            ok: false,
            error: {
              code: "execution_error",
              message: errorMsg,
            },
          };

          await emitToolFinished(deps, context, call.id, errorResult);
          state.activeCalls.delete(call.id);
          ctx.reply(meta, errorResult);
        }
        return;
      }

      case "cancel": {
        state.activeCalls.delete(msg.callId);
        ctx.reply(meta, undefined);
        return;
      }

      case "task_finished": {
        for (const [callId, active] of state.activeCalls) {
          if (active.context.taskId === msg.taskId) {
            state.activeCalls.delete(callId);
          }
        }
        ctx.reply(meta, undefined);
        return;
      }
    }
  };
}

// ---- Helpers ----

async function emitToolFinished(
  deps: { emit: (event: OrchestratorEvent) => Promise<void> },
  context: ToolExecutionContext,
  callId: string,
  result: ToolExecResult,
): Promise<void> {
  await deps.emit({
    type: "tool_finished",
    agentId: context.agentId,
    taskId: context.taskId,
    callId,
    result,
  });
}

// ---- Factory (spawn-time injection) ----

export function createToolActor(initial: {
  emit: (event: OrchestratorEvent) => Promise<void>;
  providers: Map<string, ToolProvider>;
  approvalGateway?: ApprovalGateway;
}) {
  const state: ToolActorState = {
    providers: initial.providers,
    approvalGateway: initial.approvalGateway,
    activeCalls: new Map(),
  };

  return {
    handler: toolActor(state, { emit: initial.emit }),
    state,
  };
}
