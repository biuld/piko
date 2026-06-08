import type { Message, TranscriptDelta } from "piko-engine-protocol";

/** Build transcript deltas from the messages appended in a step. */
export function buildTranscriptDelta(messages: Message[]): TranscriptDelta[] {
  const deltas: TranscriptDelta[] = [];
  for (const msg of messages) {
    if (msg.role === "assistant") {
      deltas.push({ kind: "assistant_message", message: msg });
    } else if (msg.role === "toolResult") {
      deltas.push({
        kind: "tool_result",
        message: msg,
        toolCallId: msg.toolCallId,
      });
    }
  }
  return deltas;
}
