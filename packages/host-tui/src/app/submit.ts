import { computeCumulativeUsage, processFileArguments } from "piko-host-runtime";
import { getTheme } from "../theme.js";
import type { BaseApp } from "./base.js";

export interface SubmitDeps extends BaseApp {
  updateHeader(): void;
  updateFooter(): void;
}

export function doRunStream(app: SubmitDeps, stream: ReturnType<BaseApp["host"]["streamPrompt"]>): void {
  let hasAssistant = false;
  const tcIds = new Map<string, string>();
  const tcNames = new Map<string, string>();
  void (async () => {
    for await (const e of stream) {
      if (e.type === "message_delta") { (hasAssistant ? app.chatView.updateLastAssistant : (hasAssistant = true, app.chatView.addMessage))("assistant", (e as any).delta); app.chatView.rebuildChat(); app.tui.requestRender(); }
      else if (e.type === "thinking_delta") { app.statusLine.set("progress", getTheme().fg("muted", "Thinking...")); app.tui.requestRender(); }
      else if (e.type === "tool_call_start") {
        app.statusLine.set("progress", getTheme().fg("toolPendingBg", `Running ${e.name}...`));
        tcIds.set(e.id, app.chatView.startToolCall(e.name, e.args, app.host.cwd));
        tcNames.set(e.id, e.name);
        app.chatView.rebuildChat(); app.tui.requestRender();
        app.extensionHost.dispatchEvent({ type: "tool_call_start", name: e.name, args: e.args as Record<string, unknown> });
      } else if (e.type === "tool_call_end") {
        const n = tcNames.get(e.id) ?? "tool";
        app.statusLine.set("progress", getTheme().fg(e.isError ? "error" : "success", `${n} ${e.isError ? "failed" : "completed"}`));
        const tid = tcIds.get(e.id); if (tid) app.chatView.endToolCall(tid, e.result, e.isError);
        app.chatView.rebuildChat(); app.tui.requestRender();
        app.extensionHost.dispatchEvent({ type: "tool_call_end", name: n, result: e.result, isError: e.isError });
      }
    }
    const r = await stream.result();
    app.spinner.stop(); app.abortController = null;
    app.transcript = r.messages;
    const u = computeCumulativeUsage(r.messages);
    app.cumulativeInput += u.input; app.cumulativeOutput += u.output;
    app.cumulativeCacheRead += u.cacheRead; app.cumulativeCacheWrite += u.cacheWrite; app.cumulativeCost += u.cost;
    app.chatView.rebuildFromTranscript(app.transcript,
      r.status === "max_steps" ? "Stopped after reaching max steps" : r.status === "aborted" ? "Interrupted" : r.status === "error" ? "Run failed" : undefined);
    app.updateHeader(); app.updateFooter(); app.statusLine.set("progress", undefined);
    app.running = false; app.chatView.rebuildChat(); app.tui.requestRender();
    app.extensionHost.dispatchEvent({ type: "turn_end", status: r.status, steps: app.transcript.length });
  })().catch((err: unknown) => {
    app.spinner.stop(); app.abortController = null; app.running = false;
    app.chatView.addMessage("system", err instanceof Error ? err.message : String(err));
    app.chatView.rebuildChat(); app.tui.requestRender();
  });
}

export function doSubmit(app: SubmitDeps, text: string): void {
  const t = text.trim(); if (!t) return;
  const { expanded } = processFileArguments(t, app.host.cwd);
  app.running = true; app.abortController = new AbortController();
  app.spinner.start(); if (app.workingIndicatorConfig) app.spinner.setIndicator(app.workingIndicatorConfig);
  app.statusLine.set("progress", "");
  app.chatView.addMessage("user", expanded); app.chatView.rebuildChat(); app.tui.requestRender();
  app.extensionHost.dispatchEvent({ type: "message", role: "user", content: expanded });
  doRunStream(app, app.host.streamPrompt(expanded, {}, app.abortController.signal));
}

export function doSubmitStream(app: SubmitDeps, factory: (sig: AbortSignal) => ReturnType<BaseApp["host"]["streamPrompt"]>, label: string): void {
  app.editor.setText("");
  app.running = true; app.abortController = new AbortController();
  const stream = factory(app.abortController.signal);
  app.spinner.start(); if (app.workingIndicatorConfig) app.spinner.setIndicator(app.workingIndicatorConfig);
  app.statusLine.set("progress", "");
  app.chatView.addMessage("user", label); app.chatView.rebuildChat(); app.tui.requestRender();
  app.extensionHost.dispatchEvent({ type: "message", role: "user", content: label });
  doRunStream(app, stream);
}
