import type {
  AssistantMessage,
  Message,
  EngineInput,
  EngineEvent,
  EngineTool,
} from "piko-engine-protocol";
import type { NativeToolRegistry } from "./types.js";
import { buildToolResultMessage } from "./transcript-builder.js";

export interface ToolExecutionResult {
  messages: Message[];
  approvalNeeded: boolean;
  approvalRequestId?: string;
  approvalKind?: string;
  approvalDetails?: unknown;
}

export async function executeToolCalls(
  assistantMessage: AssistantMessage,
  tools: EngineTool[],
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
): Promise<ToolExecutionResult> {
  const toolCalls = assistantMessage.content.filter(
    (c) => c.type === "toolCall",
  );

  if (toolCalls.length === 0) {
    return { messages: [], approvalNeeded: false };
  }

  const messages: Message[] = [];
  let approvalNeeded = false;
  let approvalRequestId: string | undefined;
  let approvalKind: string | undefined;
  let approvalDetails: unknown;

  for (const tc of toolCalls) {
    if (signal?.aborted) break;

    const toolDef = tools.find((t) => t.name === tc.name);
    if (!toolDef) {
      const errorMsg = buildToolResultMessage(
        tc.id,
        tc.name,
        `Tool not found: ${tc.name}`,
        true,
      );
      messages.push(errorMsg);
      emit({
        type: "tool_call_end",
        id: tc.id,
        result: `Tool not found: ${tc.name}`,
        isError: true,
      });
      continue;
    }

    // Check for approval requirement
    if (toolDef.metadata?.["requiresApproval"]) {
      approvalNeeded = true;
      approvalRequestId = tc.id;
      approvalKind = `tool:${tc.name}`;
      approvalDetails = {
        toolName: tc.name,
        toolCallId: tc.id,
        arguments: tc.arguments,
      };
      break; // Stop at first approval needed
    }

    emit({
      type: "tool_call_start",
      id: tc.id,
      name: tc.name,
      args: tc.arguments,
    });

    try {
      const executor = registry[toolDef.executor.target];
      if (!executor) {
        throw new Error(`No executor registered for target: ${toolDef.executor.target}`);
      }

      const result = await executor(tc.arguments);
      const msg = buildToolResultMessage(tc.id, tc.name, result, false);
      messages.push(msg);

      emit({
        type: "tool_call_end",
        id: tc.id,
        result,
        isError: false,
      });
    } catch (err) {
      const errorText = err instanceof Error ? err.message : String(err);
      const msg = buildToolResultMessage(tc.id, tc.name, errorText, true);
      messages.push(msg);

      emit({
        type: "tool_call_end",
        id: tc.id,
        result: errorText,
        isError: true,
      });
    }
  }

  return {
    messages,
    approvalNeeded,
    approvalRequestId,
    approvalKind,
    approvalDetails,
  };
}
