import { computeCumulativeUsage, processFileArguments } from "piko-host-runtime";
import { getTheme } from "../theme.js";
import type { AppConstructor, BaseApp } from "./base.js";

export function SubmitMixin<TBase extends AppConstructor<BaseApp>>(Base: TBase) {
  return class extends Base {
    _runStream(this: any, stream: ReturnType<typeof this.host.streamPrompt>): void {
      let hasAssistant = false;
      const tcIds = new Map<string, string>();
      const tcNames = new Map<string, string>();
      void (async () => {
        for await (const e of stream) {
          if (e.type === "message_delta") { (hasAssistant ? this.chatView.updateLastAssistant : (hasAssistant = true, this.chatView.addMessage))("assistant", (e as any).delta); this.chatView.rebuildChat(); this.tui.requestRender(); }
          else if (e.type === "thinking_delta") { this.statusLine.set("progress", getTheme().fg("muted", "Thinking...")); this.tui.requestRender(); }
          else if (e.type === "tool_call_start") {
            this.statusLine.set("progress", getTheme().fg("toolPendingBg", `Running ${e.name}...`));
            tcIds.set(e.id, this.chatView.startToolCall(e.name, e.args, this.host.cwd));
            tcNames.set(e.id, e.name);
            this.chatView.rebuildChat(); this.tui.requestRender();
            this.extensionHost.dispatchEvent({ type: "tool_call_start", name: e.name, args: e.args as Record<string, unknown> });
          } else if (e.type === "tool_call_end") {
            const n = tcNames.get(e.id) ?? "tool";
            this.statusLine.set("progress", getTheme().fg(e.isError ? "error" : "success", `${n} ${e.isError ? "failed" : "completed"}`));
            const tid = tcIds.get(e.id); if (tid) this.chatView.endToolCall(tid, e.result, e.isError);
            this.chatView.rebuildChat(); this.tui.requestRender();
            this.extensionHost.dispatchEvent({ type: "tool_call_end", name: n, result: e.result, isError: e.isError });
          }
        }
        const r = await stream.result();
        this.spinner.stop(); this.abortController = null;
        this.transcript = r.messages;
        const u = computeCumulativeUsage(r.messages);
        this.cumulativeInput += u.input; this.cumulativeOutput += u.output;
        this.cumulativeCacheRead += u.cacheRead; this.cumulativeCacheWrite += u.cacheWrite; this.cumulativeCost += u.cost;
        this.chatView.rebuildFromTranscript(this.transcript,
          r.status === "max_steps" ? "Stopped after reaching max steps" : r.status === "aborted" ? "Interrupted" : r.status === "error" ? "Run failed" : undefined);
        this.updateHeader(); this.updateFooter(); this.statusLine.set("progress", undefined);
        this.running = false; this.chatView.rebuildChat(); this.tui.requestRender();
        this.extensionHost.dispatchEvent({ type: "turn_end", status: r.status, steps: this.transcript.length });
      })().catch((err: unknown) => {
        this.spinner.stop(); this.abortController = null; this.running = false;
        this.chatView.addMessage("system", err instanceof Error ? err.message : String(err));
        this.chatView.rebuildChat(); this.tui.requestRender();
      });
    }

    submit(this: any, text: string): void {
      const t = text.trim(); if (!t) return;
      const { expanded } = processFileArguments(t, this.host.cwd);
      this.running = true; this.abortController = new AbortController();
      this.spinner.start(); if (this.workingIndicatorConfig) this.spinner.setIndicator(this.workingIndicatorConfig);
      this.statusLine.set("progress", "");
      this.chatView.addMessage("user", expanded); this.chatView.rebuildChat(); this.tui.requestRender();
      this.extensionHost.dispatchEvent({ type: "message", role: "user", content: expanded });
      this._runStream(this.host.streamPrompt(expanded, {}, this.abortController.signal));
    }

    submitStream(this: any, factory: (sig: AbortSignal) => ReturnType<typeof this.host.streamPrompt>, label: string): void {
      this.editor.setText("");
      this.running = true; this.abortController = new AbortController();
      const stream = factory(this.abortController.signal);
      this.spinner.start(); if (this.workingIndicatorConfig) this.spinner.setIndicator(this.workingIndicatorConfig);
      this.statusLine.set("progress", "");
      this.chatView.addMessage("user", label); this.chatView.rebuildChat(); this.tui.requestRender();
      this.extensionHost.dispatchEvent({ type: "message", role: "user", content: label });
      this._runStream(stream);
    }
  };
}
