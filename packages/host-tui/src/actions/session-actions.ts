import type { NotifyInput } from "../notifications/types.js";
import type { TreeNavigationResult } from "../shared/index.js";
import type { TreeNavigationViewResult, TuiEvent } from "../state/events.js";
import { entriesToTranscript } from "../timeline/entries-to-transcript.js";

export interface SessionHostPort {
  navigateToEntry(entryId: string): Promise<TreeNavigationResult>;
  forkSession(entryId: string): Promise<{ selectedText?: string }>;
  importSession(path: string): Promise<void>;
  renameSession(sessionId: string, name: string): Promise<void>;
  setSessionName(name?: string): Promise<void>;
  switchSession(specifier: string): Promise<any>;
  newSession(): Promise<void>;
  cloneSession(): Promise<void>;
  restoreFromSession(): Promise<void>;
  sessionId: string;
  getConfig(): any;
  getThinkingLevel(): string | undefined;
}

export interface SessionActionDeps {
  host: SessionHostPort;
  dispatch(event: TuiEvent): void;
  closeSurface(surfaceId: string): void;
  notify(notification: NotifyInput): void;
  nextOperationId(): string;
  getCurrentRevision(): number;
  isOperationActive(operationId: string): boolean;
}

export class SessionActions {
  constructor(private deps: SessionActionDeps) {}

  // syncSessionState removed — hostd snapshot events (session_opened /
  // state_snapshot) are the single source of truth for session state.

  async navigateTree(entryId: string, surfaceId: string): Promise<void> {
    const operationId = this.deps.nextOperationId();
    this.deps.dispatch({ type: "tree_navigation_started", operationId, entryId });

    try {
      const domainResult = await this.deps.host.navigateToEntry(entryId);

      const transcript = entriesToTranscript(domainResult.branchEntries);

      let editorDraft: TreeNavigationViewResult["editorDraft"];
      if (domainResult.editorContent !== undefined) {
        const content = domainResult.editorContent;
        const text =
          typeof content === "string"
            ? content
            : Array.isArray(content)
              ? content
                  .filter((part): part is { type: "text"; text: string } => part.type === "text")
                  .map((part) => part.text)
                  .join("\n")
              : "";

        editorDraft = {
          text,
          revision: this.deps.getCurrentRevision() + 1,
          source: {
            kind: "session_tree",
            sessionId: domainResult.sessionId,
            entryId: domainResult.selectedEntryId,
          },
        };
      }

      const result: TreeNavigationViewResult = {
        status: domainResult.status,
        sessionId: domainResult.sessionId,
        oldLeafId: domainResult.oldLeafId,
        newLeafId: domainResult.newLeafId,
        selectedEntryId: domainResult.selectedEntryId,
        transcript,
        editorDraft,
        surfaceId,
      };

      if (!this.deps.isOperationActive(operationId)) {
        return; // Stale navigation ignored
      }

      this.deps.dispatch({ type: "tree_navigation_succeeded", operationId, result });

      this.deps.closeSurface(surfaceId);

      if (result.status === "already_current") {
        this.deps.notify({
          message: "Already at this point",
          severity: "info",
          source: "session",
        });
      } else {
        this.deps.notify({
          message: "Navigated to entry",
          severity: "success",
          source: "session",
        });
      }
    } catch (error) {
      if (!this.deps.isOperationActive(operationId)) {
        return; // Stale navigation failed ignored
      }
      this.deps.dispatch({
        type: "tree_navigation_failed",
        operationId,
        error: error instanceof Error ? error.message : String(error),
      });
      this.deps.notify({
        message: `Navigation failed: ${error instanceof Error ? error.message : String(error)}`,
        severity: "error",
        source: "session",
      });
    }
  }

  async forkSession(entryId: string, surfaceId: string): Promise<void> {
    try {
      const result = await this.deps.host.forkSession(entryId);

      this.deps.closeSurface(surfaceId);
      this.deps.notify({
        message: "Forked to new session",
        severity: "success",
        source: "session",
      });

      if (result.selectedText) {
        this.deps.dispatch({
          type: "editor_draft_replaced",
          text: result.selectedText,
        });
      }
    } catch (error) {
      this.deps.notify({
        message: `Fork failed: ${error instanceof Error ? error.message : String(error)}`,
        severity: "error",
        source: "session",
      });
    }
  }

  async importSession(path: string, surfaceId?: string): Promise<void> {
    try {
      await this.deps.host.importSession(path);

      if (surfaceId) {
        this.deps.closeSurface(surfaceId);
      }
      this.deps.notify({
        message: "Session imported",
        severity: "success",
        source: "session",
      });
    } catch (error) {
      this.deps.notify({
        message: `Import failed: ${error instanceof Error ? error.message : String(error)}`,
        severity: "error",
        source: "session",
      });
    }
  }

  async renameSession(name: string, sessionId?: string, surfaceId?: string): Promise<void> {
    try {
      if (sessionId) {
        await this.deps.host.renameSession(sessionId, name);
      } else {
        await this.deps.host.setSessionName(name);
      }
      this.deps.notify({
        message: `Session renamed to "${name}"`,
        severity: "success",
        source: "session",
      });
      this.deps.dispatch({
        type: "session_info_updated",
        sessionId: sessionId || this.deps.host.sessionId,
        sessionName: name,
      });
      if (surfaceId) {
        this.deps.closeSurface(surfaceId);
      }
    } catch (error) {
      this.deps.notify({
        message: `Rename failed: ${error instanceof Error ? error.message : String(error)}`,
        severity: "error",
        source: "session",
      });
    }
  }

  async switchSession(specifier: string, surfaceId?: string): Promise<void> {
    try {
      const sessionManager = await this.deps.host.switchSession(specifier);
      if (!sessionManager) {
        this.deps.notify({
          message: `Session not found: ${specifier}`,
          severity: "warning",
          source: "session",
        });
        return;
      }

      await this.deps.host.restoreFromSession();
      const config = this.deps.host.getConfig();
      const restoredThinking = this.deps.host.getThinkingLevel();

      this.deps.dispatch({
        type: "model_changed",
        model: config.model,
        providerConfig: config.provider,
      });
      if (restoredThinking !== undefined) {
        this.deps.dispatch({
          type: "thinking_level_changed",
          level: restoredThinking,
        });
      }

      if (surfaceId) {
        this.deps.closeSurface(surfaceId);
      }

      this.deps.notify({
        message: `Session: ${specifier.slice(0, 20)}`,
        severity: "success",
        source: "session",
      });
    } catch (error) {
      this.deps.notify({
        message: `Failed to switch session: ${error instanceof Error ? error.message : String(error)}`,
        severity: "error",
        source: "session",
      });
    }
  }

  async newSession(): Promise<void> {
    try {
      await this.deps.host.newSession();
      this.deps.notify({
        message: "New session started",
        severity: "success",
        source: "session",
      });
    } catch (error) {
      this.deps.notify({
        message: `Failed to start new session: ${error instanceof Error ? error.message : String(error)}`,
        severity: "error",
        source: "session",
      });
    }
  }

  async cloneSession(): Promise<void> {
    try {
      await this.deps.host.cloneSession();
      this.deps.notify({
        message: "Session cloned",
        severity: "success",
        source: "session",
      });
    } catch (error) {
      this.deps.notify({
        message: `Clone failed: ${error instanceof Error ? error.message : String(error)}`,
        severity: "error",
        source: "session",
      });
    }
  }
}
