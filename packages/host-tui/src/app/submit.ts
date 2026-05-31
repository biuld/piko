import { computeCumulativeUsage, processFileArguments } from "piko-host-runtime";
import type { TuiContext } from "./context.js";
import { getTheme } from "../theme.js";

export function createSubmitOps(ctx: TuiContext) {
  const theme = getTheme();

  function runStreamWithUI(stream: ReturnType<typeof ctx.host.streamPrompt>, _displayText: string): void {
    let hasAssistant = false;
    const toolCallIds: Map<string, string> = new Map();
    const toolCallNames: Map<string, string> = new Map();

    void (async () => {
      for await (const event of stream) {
        if (event.type === "message_delta") {
          if (!hasAssistant) {
            ctx.chatView.addMessage("assistant", (event as { delta: string }).delta);
            hasAssistant = true;
          } else {
            ctx.chatView.updateLastAssistant((event as { delta: string }).delta);
          }
          ctx.chatView.rebuildChat();
          ctx.tui.requestRender();
        } else if (event.type === "thinking_delta") {
          ctx.statusLine.set("progress", theme.fg("muted", "Thinking..."));
          ctx.tui.requestRender();
        } else if (event.type === "tool_call_start") {
          ctx.statusLine.set("progress", theme.fg("toolPendingBg", `Running ${event.name}...`));
          const tid = ctx.chatView.startToolCall(event.name, event.args, ctx.host.cwd);
          toolCallIds.set(event.id, tid);
          toolCallNames.set(event.id, event.name);
          ctx.chatView.rebuildChat();
          ctx.tui.requestRender();
          ctx.extensionHost.dispatchEvent({ type: "tool_call_start", name: event.name, args: event.args as Record<string, unknown> });
        } else if (event.type === "tool_call_end") {
          const toolName = toolCallNames.get(event.id) ?? "tool";
          ctx.statusLine.set("progress", event.isError ? theme.fg("error", `${toolName} failed`) : theme.fg("success", `${toolName} completed`));
          const tid = toolCallIds.get(event.id);
          if (tid) ctx.chatView.endToolCall(tid, event.result, event.isError);
          ctx.chatView.rebuildChat();
          ctx.tui.requestRender();
          ctx.extensionHost.dispatchEvent({ type: "tool_call_end", name: toolName, result: event.result, isError: event.isError });
        }
      }

      const result = await stream.result();
      ctx.spinner.stop();
      ctx.abortController = null;
      ctx.transcript = result.messages;
      const usage = computeCumulativeUsage(result.messages);
      ctx.cumulativeInput += usage.input;
      ctx.cumulativeOutput += usage.output;
      ctx.cumulativeCacheRead += usage.cacheRead;
      ctx.cumulativeCacheWrite += usage.cacheWrite;
      ctx.cumulativeCost += usage.cost;
      ctx.chatView.rebuildFromTranscript(ctx.transcript,
        result.status === "max_steps" ? "Stopped after reaching max steps"
        : result.status === "aborted" ? "Interrupted"
        : result.status === "error" ? "Run failed"
        : undefined);
      ctx.updateHeader();
      ctx.updateFooter();
      ctx.statusLine.set("progress", undefined);
      ctx.running = false;
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
      ctx.extensionHost.dispatchEvent({ type: "turn_end", status: result.status, steps: ctx.transcript.length });
    })().catch((error: unknown) => {
      ctx.spinner.stop();
      ctx.abortController = null;
      ctx.running = false;
      const message = error instanceof Error ? error.message : String(error);
      ctx.chatView.addMessage("system", message);
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
    });
  }

  function submitUserMessage(text: string): void {
    const trimmed = text.trim();
    if (!trimmed) return;
    const { expanded: expandedText } = processFileArguments(trimmed, ctx.host.cwd);

    ctx.running = true;
    ctx.abortController = new AbortController();
    ctx.spinner.start();
    if (ctx.workingIndicatorConfig) ctx.spinner.setIndicator(ctx.workingIndicatorConfig);
    ctx.statusLine.set("progress", "");
    ctx.chatView.addMessage("user", expandedText);
    ctx.chatView.rebuildChat();
    ctx.tui.requestRender();
    ctx.extensionHost.dispatchEvent({ type: "message", role: "user", content: expandedText });

    const stream = ctx.host.streamPrompt(expandedText, {}, ctx.abortController.signal);
    runStreamWithUI(stream, expandedText);
  }

  return { runStreamWithUI, submitUserMessage };
}
