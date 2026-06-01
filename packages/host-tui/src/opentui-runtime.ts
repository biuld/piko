// ============================================================================
// OpenTUI Runtime — wires PikoHost + TuiStore + OpenTUI renderer
// ============================================================================

import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import { PikoHost } from "piko-host-runtime";
import { makeHostOptions } from "./app/host-options.js";
import type { RunTuiOptions } from "./app/types.js";
import { runOpenTui } from "./renderer/opentui/App.js";
import { createDefaultStore } from "./renderer/opentui/store.js";

/**
 * Launch piko with the OpenTUI + SolidJS renderer.
 *
 * This is the Phase 2 entry point — minimal closed loop:
 * App shell, editor, chat, streaming, tool calls, abort, resize.
 */
export async function launchOpenTui(
  initialModel: Model<string>,
  initialProviderConfig: EngineProviderConfig,
  options: RunTuiOptions = {},
): Promise<void> {
  // Create the host (same as runTui)
  const host = await PikoHost.create({
    ...makeHostOptions(
      initialModel,
      initialProviderConfig,
      { session: options.session },
      options.settingsManager,
      options,
    ),
  });

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

  // Update session info
  store.dispatch({
    type: "session_info_updated",
    sessionName: sessionName ?? undefined,
    messageCount: messages.length,
  });

  // Launch the OpenTUI renderer
  await runOpenTui(store, host, options);
}

/**
 * Extract display text from a message content.
 */
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
