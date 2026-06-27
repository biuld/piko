// ============================================================================
// OpenTUI Runtime — wires HostdFacade + TuiStore + OpenTUI renderer
// Owns CliRenderer lifecycle for safe terminal cleanup on exit.
// ============================================================================

import { createCliRenderer } from "@opentui/core";
import { render } from "@opentui/solid";
import { createHostConfig } from "./app/host-config.js";
import { createHostdFacade } from "./app/hostd-facade.js";
import type { RunTuiOptions } from "./app/types.js";
import { createApprovalBridge } from "./approval-bridge.js";
import { HostdClient } from "./client/hostd-client.js";
import { App } from "./renderer/opentui/App.js";
import { createDefaultStore } from "./renderer/opentui/store.js";
import type { Model, ModelProviderConfig } from "./shared/index.js";

/**
 * Launch piko with the OpenTUI + SolidJS renderer.
 * Requires hostd mode (legacy PikoHost has been removed).
 */
export async function launchOpenTui(
  initialModel: Model<string>,
  initialProviderConfig: ModelProviderConfig,
  options: RunTuiOptions,
): Promise<void> {
  try {
    if (!options.hostd?.enabled) {
      throw new Error("hostd mode is required — legacy PikoHost has been removed");
    }

    // Approval bridge: shared state between host orchestrator and ActionService
    const approvalBridge = createApprovalBridge();

    // Create HostdClient — the TUI's wire to the Rust hostd process
    const hostdClient = new HostdClient({
      command: options.hostd?.command,
      args: options.hostd?.args,
    });

    const host = createHostdFacade(hostdClient, {
      session: options.session,
      debugTracePath: options.debugTracePath,
    });
    host.setConfig(createHostConfig(initialModel, initialProviderConfig));

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

    // Push initial model config to hostd
    hostdClient
      .send({
        type: "config_set",
        command_id: crypto.randomUUID(),
        default_model: initialModel.id,
        default_provider: initialModel.provider,
      })
      .catch(() => {});

    // Create the state store
    const initialLayout = {
      hideThinking: options.preferences.getHideThinkingBlock(),
      theme: options.preferences.getTheme() ?? "dark",
    };
    const store = createDefaultStore(config.model, config.provider, host.cwd, initialLayout);

    options.preferences.onChange((newSettings) => {
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
    store.dispatch({
      type: "session_info_updated",
      sessionName: undefined,
      messageCount: 0,
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
  } catch (err) {
    console.error("launchOpenTui failed:", err instanceof Error ? err.message : String(err));
    if (err instanceof Error && err.stack) console.error(err.stack);
    process.exit(1);
  }
}
