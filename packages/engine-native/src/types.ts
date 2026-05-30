import type { Message } from "piko-engine-protocol";

// ---- Tool executor registry ----

export type NativeToolExecutor = (
  args: Record<string, unknown>,
) => Promise<unknown>;

export interface NativeToolRegistry {
  [name: string]: NativeToolExecutor;
}

// ---- Engine state machine states ----

export type EngineState =
  | "idle"
  | "requesting_assistant"
  | "executing_tools"
  | "awaiting_approval"
  | "completed"
  | "error";

// ---- Internal step context ----

export interface StepContext {
  appendedMessages: Message[];
  state: EngineState;
  errorMessage?: string;
}

export function createInitialStepContext(): StepContext {
  return {
    appendedMessages: [],
    state: "idle",
  };
}
