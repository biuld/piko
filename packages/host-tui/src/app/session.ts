import type { AppConstructor, BaseApp } from "./base.js";

export function SessionMixin<TBase extends AppConstructor<BaseApp>>(Base: TBase) {
  return class extends Base {
    async syncTranscript(this: any, msg?: string): Promise<void> {
      const loaded = await this.host.loadMessages();
      this.sessionName = await this.host.getSessionName();
      this.transcript = [...loaded];
      this.updateHeader(); this.updateFooter();
      this.chatView.rebuildFromTranscript(this.transcript, msg);
      this.chatView.rebuildChat(); this.tui.requestRender();
    }

    async resume(this: any): Promise<void> {
      const loaded = await this.host.loadMessages();
      if (loaded.length === 0) { this.chatView.addMessage("system", `Session ${this.host.sessionId} not found or empty`); this.chatView.rebuildChat(); this.tui.requestRender(); return; }
      await this.host.restoreFromSession();
      this.currentModel = this.host.getConfig().model;
      this.currentProviderConfig = this.host.getConfig().provider;
      this.currentThinkingLevel = this.host.getThinkingLevel();
      this.chatView.addMessage("system", `Resumed session ${this.host.sessionId} (${loaded.length} messages)`);
      this.chatView.rebuildChat(); this.tui.requestRender();
    }

    async newSession(this: any): Promise<void> {
      await this.host.newSession();
      this.chatView.addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help");
      this.chatView.rebuildChat(); this.tui.requestRender();
    }

    async clone(this: any): Promise<void> {
      if (!this.host.isSessionPersisted()) { this.chatView.addMessage("system", "Clone requires a saved session"); this.chatView.rebuildChat(); this.tui.requestRender(); return; }
      await this.host.cloneSession();
      this.chatView.addMessage("system", `Cloned branch into session ${this.host.sessionId}`);
      this.chatView.rebuildChat(); this.tui.requestRender();
    }

    async fork(this: any, entryId: string): Promise<void> {
      if (!this.host.isSessionPersisted()) { this.chatView.addMessage("system", "Fork requires a saved session"); this.chatView.rebuildChat(); this.tui.requestRender(); return; }
      const result = await this.host.forkSession(entryId);
      const suffix = result.selectedText ? `\nOriginal prompt: ${result.selectedText}` : "";
      this.chatView.addMessage("system", `Forked into session ${this.host.sessionId}${suffix}`);
      this.chatView.rebuildChat(); this.tui.requestRender();
      if (result.selectedText) this.editor.setText(result.selectedText);
    }
  };
}
