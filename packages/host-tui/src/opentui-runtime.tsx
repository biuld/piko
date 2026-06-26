// ============================================================================
// OpenTUI Runtime — wires PikoHost + TuiStore + OpenTUI renderer
// Owns CliRenderer lifecycle for safe terminal cleanup on exit.
// ============================================================================

import { createCliRenderer } from "@opentui/core";
import { render } from "@opentui/solid";
import type { Model, ModelProviderConfig } from "piko-host-runtime";
import { PikoHost } from "piko-host-runtime";
import { makeHostOptions } from "./app/host-options.js";
import type { RunTuiOptions } from "./app/types.js";
import { createApprovalBridge } from "./approval-bridge.js";
import { App } from "./renderer/opentui/App.js";
import { createDefaultStore } from "./renderer/opentui/store.js";
import { entriesToTranscript } from "./timeline/entries-to-transcript.js";

/**
 * Launch piko with the OpenTUI + SolidJS renderer.
 */
export async function launchOpenTui(
  initialModel: Model<string>,
  initialProviderConfig: ModelProviderConfig,
  options: RunTuiOptions,
): Promise<void> {
  try {
    // Approval bridge: shared state between host orchestrator (calls approvalHandler
    // during tool execution) and ActionService (dispatches UI events and resolves).
    // Created before Host so it can be passed to makeHostOptions and wired into
    // the orchestrator's ApprovalGateway.
    const approvalBridge = createApprovalBridge();

    // Create the host with the approval handler wired in
    const host = await PikoHost.create({
      ...makeHostOptions(
        initialModel,
        initialProviderConfig,
        { session: options.session },
        options.settingsManager,
        options,
        { approvalHandler: approvalBridge.handler },
      ),
    });

    if (options.debugTracePath) {
      host.debugTracePath = options.debugTracePath;
    }

    // Set session name from CLI if provided
    if (options.sessionName) {
      await host.setSessionName(options.sessionName);
    }

    // Restore host state (model, thinking level, active tools) from session log
    await host.restoreFromSession();
    const config = host.getConfig();
    const thinkingLevel = host.getThinkingLevel();

    // Create the state store
    const initialLayout = {
      hideThinking: options.settingsManager.getHideThinkingBlock(),
      theme: options.settingsManager.getTheme() ?? "dark",
    };
    const store = createDefaultStore(config.model, config.provider, host.cwd, initialLayout);

    options.settingsManager.onChange((newSettings) => {
      store.dispatch({
        type: "settings_updated",
        settings: {
          hideThinking: newSettings.hideThinkingBlock ?? false,
          theme: newSettings.theme ?? "dark",
        },
      });
      if (newSettings.defaultThinkingLevel !== undefined) {
        store.dispatch({
          type: "thinking_level_changed",
          level: newSettings.defaultThinkingLevel,
        });
      }
    });

    if (thinkingLevel !== undefined) {
      store.dispatch({
        type: "thinking_level_changed",
        level: thinkingLevel,
      });
    }

    // Load initial session data
    const messages = await host.loadMessages();
    const entries = await host.loadBranchEntries();
    const sessionName = await host.getSessionName();

    if (entries.length > 0) {
      store.dispatch({
        type: "session_resumed",
        sessionId: host.sessionFile ?? "",
        sessionName: sessionName ?? undefined,
        transcript: entriesToTranscript(entries),
      });
    }

    store.dispatch({
      type: "session_info_updated",
      sessionName: sessionName ?? undefined,
      messageCount: messages.length,
    });

    // ---- Renderer lifecycle with safe terminal cleanup ----
    let cliRenderer: Awaited<ReturnType<typeof createCliRenderer>> | undefined;
    try {
      cliRenderer = await createCliRenderer();
    } catch (err) {
      console.error(
        "Failed to create CliRenderer:",
        err instanceof Error ? err.message : String(err),
      );
      process.exit(1);
    }

    let destroyed = false;
    let resolveExit!: () => void;
    const exitPromise = new Promise<void>((resolve) => {
      resolveExit = resolve;
    });

    const destroy = () => {
      if (destroyed) return;
      destroyed = true;
      cliRenderer.destroy();
      resolveExit();
    };

    cliRenderer.once("destroy", () => {
      destroyed = true;
      resolveExit();
    });

    const shutdown = () => {
      destroy();
    };

    try {
      await render(
        () => (
          <App
            store={store}
            host={host}
            options={options}
            shutdown={shutdown}
            approvalBridge={approvalBridge}
          />
        ),
        cliRenderer,
      );
      await exitPromise;
    } catch (err) {
      // Ensure terminal is restored before printing error
      destroy();
      console.error("TUI render failed:", err instanceof Error ? err.message : String(err));
      console.error(err instanceof Error ? err.stack : "");
      process.exit(1);
    } finally {
      destroy();
    }

    // Execute post-render CLI features (skill, prompt template)
    if (options.skillName) {
      try {
        await host.runSkill(options.skillName);
      } catch {
        // Skill invocation failure is non-fatal
      }
    }

    if (options.promptTemplate) {
      try {
        await host.runPromptTemplate(options.promptTemplate);
      } catch {
        // Template invocation failure is non-fatal
      }
    }
  } catch (err) {
    console.error("launchOpenTui failed:", err instanceof Error ? err.message : String(err));
    if (err instanceof Error && err.stack) console.error(err.stack);
    process.exit(1);
  }
}
