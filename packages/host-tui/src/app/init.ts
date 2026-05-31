import { Spacer } from "@earendil-works/pi-tui";
import { handleSlashCommand } from "../commands/index.js";
import { DynamicBorder } from "../components/dynamic-border.js";
import { getThemeManager } from "../theme/index.js";
import { getTheme, setTheme } from "../theme.js";
import type { AppConstructor, BaseApp } from "./base.js";
import { isImageData } from "./image-paste.js";
import type { RunTuiOptions } from "./types.js";

export function InitMixin<TBase extends AppConstructor<BaseApp>>(Base: TBase) {
  return class extends Base {
    async init(this: any, options: RunTuiOptions): Promise<void> {
      if (options.extensions?.length) await this.extensionHost.loadAll(options.extensions);

      getThemeManager().load(this.host.cwd);
      const st = options.settingsManager?.getTheme();
      if (st) { const s = getThemeManager().switchTo(st); if (s) setTheme(getThemeManager().get()); }

      this.transcript = await this.host.loadMessages();
      this.sessionName = await this.host.getSessionName();
      if (options.sessionName && !this.sessionName) { await this.host.setSessionName(options.sessionName); this.sessionName = options.sessionName; }

      this.host.onAfterRebind(async () => { await this.host.restoreFromSession(); await this.syncTranscript(); });

      const cmdCtx = this.buildCommandContext();

      this.editor.onSubmit = (text: string) => {
        const t = text.trim(); if (!t) return;
        if (this.running) {
          this.host.steer(t);
          this.chatView.addMessage("system", `Queued for next turn: ${t.slice(0, 80)}${t.length > 80 ? "..." : ""}`);
          this.chatView.rebuildChat(); this.tui.requestRender(); return;
        }
        const extCmd = this.extensionHost.findCommand(t);
        if (extCmd) { extCmd.handler(t.slice(extCmd.value.length).trim(), { theme: getTheme(), setEditorText: (s: string) => this.editor.setText(s), getEditorText: () => this.getEditorComponent().getText() } as any); return; }
        if (t.startsWith("/")) { handleSlashCommand(t, cmdCtx); return; }
        this.submit(t);
      };

      const tui = this.tui;
      tui.addChild(this.headerBox);
      tui.addChild(this.chatBox);
      tui.addChild(this.widgetSlotAbove);
      tui.addChild(this.spinner);
      tui.addChild(this.statusLine);
      tui.addChild(new Spacer(1));
      tui.addChild(this.widgetSlotBelow);
      tui.addChild(new DynamicBorder((s: string) => getTheme().fg("borderMuted", s)));
      tui.addChild(this.editor);
      tui.setFocus(this.editor);
      tui.addChild(this.getFooterComponent());

      let pb = "";
      tui.addInputListener((data: string) => {
        if (this.running) return undefined;
        if (data.includes("\x1b[200~")) { pb = data.replace("\x1b[200~", ""); return undefined; }
        if (data.includes("\x1b[201~")) {
          const ei = data.indexOf("\x1b[201~"); pb += data.slice(0, ei);
          if (pb.length > 100 && isImageData(Buffer.from(pb, "binary"))) { void this.__imagePaste(Buffer.from(pb, "binary")); pb = ""; return { consume: true }; }
          pb = ""; return undefined;
        }
        if (pb.length > 0) { pb += data; return undefined; }
        if (data === "\u0010") { void this.cycleModel(false); return { consume: true }; }
        if (data === "\u000e") { void this.cycleModel(true); return { consume: true }; }
        return undefined;
      });

      this.terminal.setTitle("piko");
      process.on("SIGINT", () => {
        if (this.abortController && !this.abortController.signal.aborted) { this.abortController.abort(); this.spinner.stop(); this.statusLine.set("progress", getTheme().fg("error", "Interrupted")); }
        else if (!this.abortController) process.exit(0);
      });

      this.updateHeader();
      if (this.host.sessionFile) { this.chatView.rebuildFromTranscript(this.transcript); await this.resume(); }
      else { this.chatView.addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help"); this.updateFooter(); this.chatView.rebuildChat(); }
      tui.start();
    }

    async __imagePaste(this: any, buf: Buffer): Promise<void> {
      const { handleImagePaste } = await import("./image-paste.js");
      await handleImagePaste(this, this.editor, () => this.getEditorComponent().getText(), buf);
    }
  };
}
