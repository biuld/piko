import { matchesKey, Spacer } from "@earendil-works/pi-tui";
import { handleSlashCommand } from "../commands/index.js";
import { DynamicBorder } from "../components/dynamic-border.js";
import { getThemeManager } from "../theme/index.js";
import { getTheme, setTheme } from "../theme.js";
import type { BaseApp } from "./base.js";
import { isImageData } from "./image-paste.js";
import type { RunTuiOptions } from "./types.js";

export interface InitDeps extends BaseApp {
  updateHeader(): void;
  updateFooter(): void;
  syncTranscript(msg?: string): Promise<void>;
  resume(): Promise<void>;
  shutdown(): Promise<void>;
  buildCommandContext(): import("../commands/index.js").CommandContext;
  submit(text: string): void;
  cycleModel(forward: boolean): Promise<void>;
  getEditorComponent(): import("@earendil-works/pi-tui").EditorComponent;
  getFooterComponent(): import("@earendil-works/pi-tui").Component;
}

export async function initApp(app: InitDeps, options: RunTuiOptions): Promise<void> {
  if (options.extensions?.length) await app.extensionHost.loadAll(options.extensions);

  getThemeManager().load(app.host.cwd);
  const st = options.settingsManager?.getTheme();
  if (st) {
    const s = getThemeManager().switchTo(st);
    if (s) setTheme(getThemeManager().get());
  }

  app.transcript = await app.host.loadMessages();
  app.sessionName = await app.host.getSessionName();
  if (options.sessionName && !app.sessionName) {
    await app.host.setSessionName(options.sessionName);
    app.sessionName = options.sessionName;
  }

  app.host.onAfterRebind(async () => {
    await app.host.restoreFromSession();
    await app.syncTranscript();
  });

  const cmdCtx = app.buildCommandContext();

  app.editor.onSubmit = (text: string) => {
    const t = text.trim();
    if (!t) return;
    if (app.running) {
      app.host.steer(t);
      app.chatView.addMessage(
        "system",
        `Queued for next turn: ${t.slice(0, 80)}${t.length > 80 ? "..." : ""}`,
      );
      app.chatView.rebuildChat();
      app.tui.requestRender();
      return;
    }
    const extCmd = app.extensionHost.findCommand(t);
    if (extCmd) {
      extCmd.handler(t.slice(extCmd.value.length).trim(), {
        theme: getTheme(),
        setEditorText: (s: string) => app.editor.setText(s),
        getEditorText: () => app.getEditorComponent().getText(),
      } as any);
      return;
    }
    if (t.startsWith("/")) {
      handleSlashCommand(t, cmdCtx);
      return;
    }
    app.submit(t);
  };

  const tui = app.tui;
  tui.addChild(app.headerBox);
  tui.addChild(app.chatBox);
  tui.addChild(app.widgetSlotAbove);
  tui.addChild(app.spinner);
  tui.addChild(app.statusLine);
  tui.addChild(new Spacer(1));
  tui.addChild(app.widgetSlotBelow);
  tui.addChild(new DynamicBorder((s: string) => getTheme().fg("borderMuted", s)));
  app.editorContainer.addChild(app.editor);
  tui.addChild(app.editorContainer);
  tui.setFocus(app.editor);
  tui.addChild(app.getFooterComponent());

  let pb = "";
  tui.addInputListener((data: string) => {
    if (app.activeOverlay || app.tui.hasOverlay()) return undefined;
    // Use matchesKey() to support both legacy raw bytes and Kitty CSI-u sequences
    if (matchesKey(data, "ctrl+c")) {
      if (app.running) {
        if (app.abortController && !app.abortController.signal.aborted) {
          app.abortController.abort();
          app.spinner.stop();
          app.statusLine.set("progress", getTheme().fg("error", "Interrupted"));
          app.tui.requestRender();
        }
        return { consume: true };
      }
      const now = Date.now();
      if (now - app.lastSigintTime < 500) {
        void app.shutdown();
      } else {
        app.editor.setText("");
        app.lastSigintTime = now;
        app.tui.requestRender();
      }
      return { consume: true };
    }
    if (
      matchesKey(data, "ctrl+d") &&
      !app.running &&
      app.getEditorComponent().getText().length === 0
    ) {
      void app.shutdown();
      return { consume: true };
    }
    if (app.running) return undefined;
    if (data.includes("\x1b[200~")) {
      pb = data.replace("\x1b[200~", "");
      return undefined;
    }
    if (data.includes("\x1b[201~")) {
      const ei = data.indexOf("\x1b[201~");
      pb += data.slice(0, ei);
      if (pb.length > 100 && isImageData(Buffer.from(pb, "binary"))) {
        void _imagePaste(app, Buffer.from(pb, "binary"));
        pb = "";
        return { consume: true };
      }
      pb = "";
      return undefined;
    }
    if (pb.length > 0) {
      pb += data;
      return undefined;
    }
    if (matchesKey(data, "ctrl+p")) {
      void app.cycleModel(false);
      return { consume: true };
    }
    if (matchesKey(data, "ctrl+n")) {
      void app.cycleModel(true);
      return { consume: true };
    }
    return undefined;
  });

  app.terminal.setTitle("piko");
  process.on("SIGINT", () => {
    if (app.abortController && !app.abortController.signal.aborted) {
      app.abortController.abort();
      app.spinner.stop();
      app.statusLine.set("progress", getTheme().fg("error", "Interrupted"));
    } else if (!app.abortController) {
      const now = Date.now();
      if (now - app.lastSigintTime < 500) {
        void app.shutdown();
      } else {
        app.editor.setText("");
        app.lastSigintTime = now;
        app.tui.requestRender();
      }
    }
  });

  app.updateHeader();
  if (app.host.sessionFile) {
    app.chatView.rebuildFromTranscript(app.transcript);
    await app.resume();
  } else {
    app.chatView.addMessage("system", "New session  |  Enter submit  Ctrl+D exit  /help");
    app.updateFooter();
    app.chatView.rebuildChat();
  }
  tui.start();
}

async function _imagePaste(app: InitDeps, buf: Buffer): Promise<void> {
  const { handleImagePaste } = await import("./image-paste.js");
  await handleImagePaste(app, app.editor, () => app.getEditorComponent().getText(), buf);
}
