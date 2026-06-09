import type {
  EngineInput,
  EngineStepResult,
  EngineToolSet,
  Message,
  StatelessEngine,
} from "piko-engine-protocol";

/**
 * Engine bridge: wraps a stateless engine step with orchestrator events.
 *
 * The orchestrator calls this to run one step for an agent. It:
 * 1. Resolves toolSets for the agent
 * 2. Builds the engine input
 * 3. Streams engine events wrapped in orchestrator envelopes
 * 4. Returns the step result
 */
export interface EngineBridgeOptions {
  engine: StatelessEngine;
  toolSets: EngineToolSet[];
  /** Optional handler for host/orchestrator tools. */
  externalToolHandler?: (name: string, args: Record<string, unknown>) => Promise<unknown>;
}

export type EngineBridge = ReturnType<typeof createEngineBridge>;

/**
 * Create an engine bridge that wraps StatelessEngine calls.
 */
export function createEngineBridge(options: EngineBridgeOptions) {
  const { engine, toolSets } = options;

  return {
    /**
     * Run a single engine step.
     * Returns the step result; caller (orchestrator) is responsible for
     * wrapping orchestrator events around the engine events.
     */
    async runStep(input: {
      runId: string;
      stepId: string;
      transcript: Message[];
      systemPrompt: string;
      model: import("piko-engine-protocol").Model<string>;
      provider: import("piko-engine-protocol").EngineProviderConfig;
      settings: import("piko-engine-protocol").EngineRunSettings;
      signal?: AbortSignal;
    }): Promise<EngineStepResult> {
      const engineInput: EngineInput = {
        runId: input.runId,
        stepId: input.stepId,
        transcript: input.transcript,
        systemPrompt: input.systemPrompt,
        model: input.model,
        provider: input.provider,
        toolSets,
        settings: input.settings,
      };

      // Use engine with native engine path, but with our toolSets
      // The engine will project tools from toolSets
      const stream = engine.executeStep(engineInput, input.signal);

      // Consume the stream (events will be re-emitted by the orchestrator)
      for await (const _event of stream) {
        // Events are consumed; they'll be re-emitted by the orchestrator
      }

      return stream.result();
    },

    /** Build an externalToolHandler that handles host/orchestrator tools. */
    createToolHandler(
      handlers: Partial<Record<string, (args: Record<string, unknown>) => Promise<unknown>>>,
    ): (name: string, args: Record<string, unknown>) => Promise<unknown> {
      return async (name: string, args: Record<string, unknown>) => {
        const handler = handlers[name];
        if (!handler) {
          throw new Error(`No handler registered for external tool: ${name}`);
        }
        return handler(args);
      };
    },
  };
}
