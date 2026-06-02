// ============================================================================
// OpenTUI Runtime — wires PikoHost + TuiStore + OpenTUI renderer
// Owns CliRenderer lifecycle for safe terminal cleanup on exit.
// ============================================================================

import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import { createCliRenderer } from "@opentui/core";
import { render } from "@opentui/solid";
import { PikoHost } from "piko-host-runtime";
import { makeHostOptions } from "./app/host-options.js";
import type { RunTuiOptions } from "./app/types.js";
import { App } from "./renderer/opentui/App.js";
import { createDefaultStore } from "./renderer/opentui/store.js";

/**
 * Launch piko with the OpenTUI + SolidJS renderer.
 */
export async function launchOpenTui(
  initialModel: Model<string>,
  initialProviderConfig: EngineProviderConfig,
  options: RunTuiOptions = {},
): Promise<void> {
  try {
    // Create the host
    const host = await PikoHost.create({
      ...makeHostOptions(
        initialModel,
        initialProviderConfig,
        { session: options.session },
        options.settingsManager,
        options,
      ),
    });

    // Set session name from CLI if provided
    if (options.sessionName) {
      await host.setSessionName(options.sessionName);
    }

    // Create the state store
    const store = createDefaultStore(initialModel, initialProviderConfig, host.cwd);

    // Load initial session data
    const messages = await host.loadMessages();
    const sessionName = await host.getSessionName();

    if (messages.length > 0) {
      store.dispatch({
        type: "session_resumed",
        sessionId: host.sessionFile ?? "",
        sessionName: sessionName ?? undefined,
        transcript: messages.map((msg, i) => ({
          id: `msg-${i}`,
          role: msg.role as "user" | "assistant" | "tool",
          text: typeof msg.content === "string" ? msg.content : extractText(msg),
        })),
      });
    }

    store.dispatch({
      type: "session_info_updated",
      sessionName: sessionName ?? undefined,
      messageCount: messages.length,
    });

    // ---- Renderer lifecycle with safe terminal cleanup ----
    let cliRenderer;
    try {
      cliRenderer = await createCliRenderer();
    } catch (err) {
      console.error("Failed to create CliRenderer:", err instanceof Error ? err.message : String(err));
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

function extractText(msg: { role: string; content: unknown }): string {
  if (typeof msg.content === "string") return msg.content;
  if (Array.isArray(msg.content)) {
    return msg.content
      .filter(
        (block): block is { type: "text"; text: string } =>
          typeof block === "object" && block !== null && (block as any).type === "text",
      )
      .map((block) => block.text)
      .join("\n");
  }
  return "";
}
