import { EventStream } from "piko-engine-protocol";
import type {
  StatelessEngine,
  EngineInput,
  EngineEvent,
  EngineStepResult,
  EngineApprovalResolution,
  EngineCapabilities,
  EngineEventEnvelope,
} from "piko-engine-protocol";
import type { RemoteTransport } from "./protocol.js";
import { REMOTE_METHODS } from "./protocol.js";

export interface CreateRemoteEngineOptions {
  transport: RemoteTransport;
}

export function createRemoteEngine(
  options: CreateRemoteEngineOptions,
): StatelessEngine {
  const { transport } = options;

  const capabilities: EngineCapabilities = {
    supportsApprovals: true,
    supportsTools: true,
    supportsSandbox: true,
    supportsMCP: true,
    maxSteps: 100,
  };

  return {
    capabilities,

    executeStep(
      input: EngineInput,
      signal?: AbortSignal,
    ): EventStream<EngineEvent, EngineStepResult> {
      const stream = new EventStream<EngineEvent, EngineStepResult>();

      const unsub = transport.onNotification(
        (method: string, params: unknown) => {
          if (signal?.aborted) return;
          if (method !== REMOTE_METHODS.EVENT) return;
          const envelope = params as EngineEventEnvelope;
          stream.push(envelope.event);
        },
      );

      transport
        .send(REMOTE_METHODS.EXECUTE_STEP, input)
        .then((result) => {
          unsub();
          stream.end(result as EngineStepResult);
        })
        .catch((err) => {
          unsub();
          const errorMsg = err instanceof Error ? err.message : String(err);
          stream.push({ type: "error", message: errorMsg });
          stream.end({
            status: "error",
            appendedMessages: [],
            stopReason: "error",
          });
        });

      return stream;
    },

    async resolveApproval(
      request: EngineApprovalResolution,
      signal?: AbortSignal,
    ): Promise<EngineStepResult> {
      return transport.send(
        REMOTE_METHODS.RESOLVE_APPROVAL,
        request,
      ) as Promise<EngineStepResult>;
    },

    async shutdown(): Promise<void> {
      await transport.send(REMOTE_METHODS.SHUTDOWN, {});
      await transport.close();
    },
  };
}
