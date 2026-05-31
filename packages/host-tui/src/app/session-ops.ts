import type { TuiContext } from "./context.js";

export function createSessionOps(ctx: TuiContext) {
  async function syncSessionTranscript(systemMessage?: string): Promise<void> {
    const loaded = await ctx.host.loadMessages();
    ctx.sessionName = await ctx.host.getSessionName();
    ctx.transcript = [...loaded];
    ctx.updateHeader();
    ctx.updateFooter();
    ctx.chatView.rebuildFromTranscript(ctx.transcript, systemMessage);
    ctx.chatView.rebuildChat();
    ctx.tui.requestRender();
  }

  async function resumeSession(): Promise<void> {
    const loaded = await ctx.host.loadMessages();
    if (loaded.length === 0) {
      ctx.chatView.addMessage("system", `Session ${ctx.host.sessionId} not found or empty`);
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
      return;
    }
    await ctx.host.restoreFromSession();
    ctx.currentModel = ctx.host.getConfig().model;
    ctx.currentProviderConfig = ctx.host.getConfig().provider;
    ctx.currentThinkingLevel = ctx.host.getThinkingLevel();
    ctx.chatView.addMessage("system", `Resumed session ${ctx.host.sessionId} (${loaded.length} messages)`);
    ctx.chatView.rebuildChat();
    ctx.tui.requestRender();
  }

  async function createNewSession(): Promise<void> {
    await ctx.host.newSession();
    ctx.chatView.addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help");
    ctx.chatView.rebuildChat();
    ctx.tui.requestRender();
  }

  async function cloneSessionCmd(): Promise<void> {
    if (!ctx.host.isSessionPersisted()) {
      ctx.chatView.addMessage("system", "Clone requires a saved session");
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
      return;
    }
    await ctx.host.cloneSession();
    ctx.chatView.addMessage("system", `Cloned branch into session ${ctx.host.sessionId}`);
    ctx.chatView.rebuildChat();
    ctx.tui.requestRender();
  }

  async function forkSessionCmd(entryId: string, setEditorText: (t: string) => void): Promise<void> {
    if (!ctx.host.isSessionPersisted()) {
      ctx.chatView.addMessage("system", "Fork requires a saved session");
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
      return;
    }
    const result = await ctx.host.forkSession(entryId);
    const suffix = result.selectedText ? `\nOriginal prompt: ${result.selectedText}` : "";
    ctx.chatView.addMessage("system", `Forked into session ${ctx.host.sessionId}${suffix}`);
    ctx.chatView.rebuildChat();
    ctx.tui.requestRender();
    if (result.selectedText) setEditorText(result.selectedText);
  }

  return { syncSessionTranscript, resumeSession, createNewSession, cloneSessionCmd, forkSessionCmd };
}
