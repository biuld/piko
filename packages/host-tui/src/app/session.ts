import type { BaseApp } from "./base.js";

export interface SessionDeps extends BaseApp {
  updateHeader(): void;
  updateFooter(): void;
}

export async function doSyncTranscript(app: SessionDeps, msg?: string): Promise<void> {
  const loaded = await app.host.loadMessages();
  app.sessionName = await app.host.getSessionName();
  app.transcript = [...loaded];
  app.updateHeader();
  app.updateFooter();
  app.chatView.rebuildFromTranscript(app.transcript, msg);
  app.chatView.rebuildChat();
  app.tui.requestRender();
}

export async function doResume(app: SessionDeps): Promise<void> {
  const loaded = await app.host.loadMessages();
  if (loaded.length === 0) {
    app.chatView.addMessage("system", `Session ${app.host.sessionId} not found or empty`);
    app.chatView.rebuildChat();
    app.tui.requestRender();
    return;
  }
  await app.host.restoreFromSession();
  app.currentModel = app.host.getConfig().model;
  app.currentProviderConfig = app.host.getConfig().provider;
  app.currentThinkingLevel = app.host.getThinkingLevel();
  app.chatView.addMessage(
    "system",
    `Resumed session ${app.host.sessionId} (${loaded.length} messages)`,
  );
  app.chatView.rebuildChat();
  app.tui.requestRender();
}

export async function doNewSession(app: SessionDeps): Promise<void> {
  await app.host.newSession();
  app.chatView.addMessage("system", "New session  |  Enter submit  Ctrl+D exit  /help");
  app.chatView.rebuildChat();
  app.tui.requestRender();
}

export async function doClone(app: SessionDeps): Promise<void> {
  if (!app.host.isSessionPersisted()) {
    app.chatView.addMessage("system", "Clone requires a saved session");
    app.chatView.rebuildChat();
    app.tui.requestRender();
    return;
  }
  await app.host.cloneSession();
  app.chatView.addMessage("system", `Cloned branch into session ${app.host.sessionId}`);
  app.chatView.rebuildChat();
  app.tui.requestRender();
}

export async function doFork(app: SessionDeps, entryId: string): Promise<void> {
  if (!app.host.isSessionPersisted()) {
    app.chatView.addMessage("system", "Fork requires a saved session");
    app.chatView.rebuildChat();
    app.tui.requestRender();
    return;
  }
  const result = await app.host.forkSession(entryId);
  const suffix = result.selectedText ? `\nOriginal prompt: ${result.selectedText}` : "";
  app.chatView.addMessage("system", `Forked into session ${app.host.sessionId}${suffix}`);
  app.chatView.rebuildChat();
  app.tui.requestRender();
  if (result.selectedText) app.editor.setText(result.selectedText);
}
