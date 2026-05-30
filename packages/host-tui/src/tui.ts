import type { Model } from "@earendil-works/pi-ai";
import {
  type AutocompleteProvider,
  type AutocompleteSuggestions,
  Box,
  Editor,
  Markdown,
  ProcessTerminal,
  Text,
  TUI,
} from "@earendil-works/pi-tui";
import type { EngineProviderConfig, Message } from "piko-engine-protocol";
import {
  createDefaultSettings,
  createHostConfig,
  listAvailableModels,
  PikoHost,
  type StreamPromptResult,
} from "piko-host-runtime";
import { COMMANDS, type CommandContext, handleSlashCommand } from "./commands.js";
import { FooterComponent } from "./components/footer.js";
import { Spinner } from "./components/spinner.js";
import { StatusLine } from "./components/status-line.js";
import {
  type OverlayContext,
  openForkSelector,
  openResumeSelector,
  openTreeSelector,
} from "./overlays.js";
import { formatSessionTreeLines } from "./session-tree.js";
import { getEditorTheme, getMarkdownTheme } from "./theme.js";

export interface RunTuiOptions {
  session?: string;
}

function createAutocomplete(): AutocompleteProvider {
  return {
    async getSuggestions(
      lines: string[],
      cursorLine: number,
      cursorCol: number,
    ): Promise<AutocompleteSuggestions | null> {
      const line = lines[cursorLine] ?? "";
      const prefix = line.slice(0, cursorCol);
      if (!prefix.startsWith("/")) return null;
      return {
        items: COMMANDS.filter((c) => c.value.startsWith(prefix)),
        prefix: "/",
      };
    },
    applyCompletion(
      lines: string[],
      cursorLine: number,
      _cursorCol: number,
      item: { value: string; label: string },
      prefix: string,
    ) {
      const line = lines[cursorLine] ?? "";
      const slashIdx = line.indexOf(prefix);
      const before = line.slice(0, slashIdx);
      const newLine = `${before + item.value} `;
      return { lines: [newLine], cursorLine, cursorCol: newLine.length };
    },
  };
}

function makeHostOptions(
  model: Model<string>,
  providerConfig: EngineProviderConfig,
  sessionOptions: { session?: string },
): Parameters<typeof PikoHost.create>[0] {
  return {
    config: createHostConfig(
      model,
      providerConfig,
      createDefaultSettings({
        maxSteps: 10,
        parallelTools: false,
        allowToolCalls: true,
        allowApprovals: false,
      }),
    ),
    session: sessionOptions,
  };
}

function truncateText(text: string, maxLength = 200): string {
  if (text.length <= maxLength) return text;
  return `${text.slice(0, maxLength - 3)}...`;
}

function stringifyValue(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function summarizeToolCall(name: string, args: unknown): string {
  const suffix = truncateText(stringifyValue(args), 160);
  return `[tool] ${name} ${suffix}`;
}

function summarizeToolResult(name: string, result: unknown, isError: boolean): string {
  const prefix = isError ? `[tool error] ${name}` : `[tool result] ${name}`;
  return `${prefix} ${truncateText(stringifyValue(result), 200)}`;
}

function extractAssistantText(message: Extract<Message, { role: "assistant" }>): string {
  return message.content
    .filter(
      (block): block is Extract<(typeof message.content)[number], { type: "text" }> =>
        block.type === "text",
    )
    .map((block) => block.text)
    .join("\n");
}

export async function runTui(
  model: Model<string>,
  providerConfig: EngineProviderConfig,
  options: RunTuiOptions = {},
): Promise<void> {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);

  // Single host for the entire TUI lifecycle.
  const host = await PikoHost.create(
    makeHostOptions(model, providerConfig, {
      session: options.session,
    }),
  );

  const messages: Array<{ role: string; text: string }> = [];
  let transcript = await host.loadMessages();
  let sessionName = await host.getSessionName();
  let running = false;
  let abortController: AbortController | null = null;
  let activeOverlay: { hide(): void } | null = null;

  // When the host replaces the session (switch/new/fork/clone/import),
  // the afterRebind hook reloads the transcript automatically.
  host.onAfterRebind(async () => {
    await syncSessionTranscript();
  });

  function addMessage(role: string, text: string): void {
    messages.push({ role, text });
    if (messages.length > 50) messages.shift();
  }

  function rebuildChat(): void {
    chatBox.clear();
    for (const msg of messages) {
      if (msg.role === "user") {
        chatBox.addChild(new Markdown(`**You:** ${msg.text}`, 0, 0, getMarkdownTheme()));
      } else if (msg.role === "assistant") {
        chatBox.addChild(new Markdown(msg.text || "…", 0, 0, getMarkdownTheme()));
      } else if (msg.role === "system") {
        chatBox.addChild(new Text(msg.text));
      }
    }
  }

  function rebuildMessagesFromTranscript(systemMessage?: string): void {
    messages.length = 0;
    for (const msg of transcript) {
      if (msg.role === "user") {
        addMessage("user", typeof msg.content === "string" ? msg.content : "");
        continue;
      }
      if (msg.role === "assistant") {
        const text = extractAssistantText(msg);
        if (text.trim()) {
          addMessage("assistant", text);
        }
        for (const block of msg.content) {
          if (block.type === "toolCall") {
            addMessage("system", summarizeToolCall(block.name, block.arguments));
          }
        }
        continue;
      }
      addMessage(
        "system",
        summarizeToolResult(msg.toolName, msg.details ?? msg.content, msg.isError),
      );
    }
    if (systemMessage) {
      addMessage("system", systemMessage);
    }
  }

  async function syncSessionTranscript(systemMessage?: string): Promise<void> {
    const loaded = await host.loadMessages();
    sessionName = await host.getSessionName();
    transcript = [...loaded];
    updateHeader();
    updateFooter();
    rebuildMessagesFromTranscript(systemMessage);
    rebuildChat();
    tui.requestRender();
  }

  async function resumeSession(): Promise<void> {
    const loaded = await host.loadMessages();
    if (loaded.length === 0) {
      addMessage("system", `Session ${host.sessionId} not found or empty`);
      rebuildChat();
      tui.requestRender();
      return;
    }
    // onAfterRebind auto-syncs the transcript; add a system line on top.
    addMessage("system", `Resumed session ${host.sessionId} (${loaded.length} messages)`);
    rebuildChat();
    tui.requestRender();
  }

  async function createNewSession(): Promise<void> {
    await host.newSession();
    addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help");
    rebuildChat();
    tui.requestRender();
  }

  async function cloneSessionCmd(): Promise<void> {
    if (!host.isSessionPersisted()) {
      addMessage("system", "Clone requires a saved session");
      rebuildChat();
      tui.requestRender();
      return;
    }
    await host.cloneSession();
    addMessage("system", `Cloned branch into session ${host.sessionId}`);
    rebuildChat();
    tui.requestRender();
  }

  async function forkSessionCmd(entryId: string): Promise<void> {
    if (!host.isSessionPersisted()) {
      addMessage("system", "Fork requires a saved session");
      rebuildChat();
      tui.requestRender();
      return;
    }
    const result = await host.forkSession(entryId);
    const suffix = result.selectedText ? `\nOriginal prompt: ${result.selectedText}` : "";
    addMessage("system", `Forked into session ${host.sessionId}${suffix}`);
    rebuildChat();
    tui.requestRender();
    if (result.selectedText) editor.setText(result.selectedText);
  }

  const overlayCtx: OverlayContext = {
    tui,
    host,
    msg: addMessage,
    render: () => tui.requestRender(),
    resync: syncSessionTranscript,
    doResume: resumeSession,
    doFork: forkSessionCmd,
    setEditorText: (text) => editor.setText(text),
    getActiveOverlay: () => activeOverlay,
    setActiveOverlay: (o) => {
      activeOverlay = o;
    },
  };

  const headerBox = new Box(0, 0);

  function updateHeader(): void {
    headerBox.clear();
    headerBox.addChild(
      new Text(
        ` piko  ${model.provider}/${model.id}  session ${
          sessionName ?? host.sessionId.slice(-8)
        }  ${transcript.length} msgs `,
      ),
    );
  }

  const footerComponent = new FooterComponent({
    model,
    sessionName,
    messageCount: transcript.length,
    cwd: host.cwd,
  });

  function updateFooter(): void {
    footerComponent.update({
      model,
      sessionName,
      messageCount: transcript.length,
      cwd: host.cwd,
    });
  }

  const spinner = new Spinner();
  const statusLine = new StatusLine();

  const chatBox = new Box(0, 0);

  const editor = new Editor(tui, getEditorTheme());
  editor.setAutocompleteProvider(createAutocomplete());
  editor.onSubmit = (text: string) => {
    if (running) return;
    const trimmed = text.trim();
    if (!trimmed) return;

    if (trimmed.startsWith("/")) {
      handleSlashCommand(trimmed, cmdCtx);
      return;
    }

    running = true;
    abortController = new AbortController();
    spinner.start();
    statusLine.setStatus("");
    addMessage("user", trimmed);
    rebuildChat();
    tui.requestRender();

    let assistantIndex: number | null = null;
    void runStreaming(host, trimmed, abortController.signal, {
      onAssistantDelta: (partial) => {
        if (assistantIndex === null) {
          assistantIndex = messages.length;
          addMessage("assistant", partial);
        } else {
          messages[assistantIndex] = { role: "assistant", text: partial };
        }
        rebuildChat();
        tui.requestRender();
      },
      onThinkingDelta: () => {
        statusLine.setStatus("Thinking...");
        tui.requestRender();
      },
      onToolCallStart: (name, args) => {
        statusLine.setStatus(`Running ${name}...`);
        addMessage("system", summarizeToolCall(name, args));
        rebuildChat();
        tui.requestRender();
      },
      onToolCallEnd: (name, result, isError) => {
        statusLine.setStatus(isError ? `${name} failed` : `${name} completed`);
        addMessage("system", summarizeToolResult(name, result, isError));
        rebuildChat();
        tui.requestRender();
      },
    })
      .then((result) => {
        spinner.stop();
        abortController = null;
        transcript = result.messages;
        rebuildMessagesFromTranscript(
          result.status === "max_steps"
            ? "Stopped after reaching max steps"
            : result.status === "aborted"
              ? "Interrupted"
              : result.status === "error"
                ? "Run failed"
                : undefined,
        );
        updateHeader();
        updateFooter();
        statusLine.setStatus("");
        running = false;
        rebuildChat();
        tui.requestRender();
      })
      .catch((error: unknown) => {
        spinner.stop();
        abortController = null;
        running = false;
        const message = error instanceof Error ? error.message : String(error);
        addMessage("system", message);
        rebuildChat();
        tui.requestRender();
      });
  };

  tui.addChild(headerBox);

  const cmdCtx: CommandContext = {
    host,
    model: { provider: model.provider, id: model.id, name: model.name },
    sessionName,
    setSessionName: (name: string | undefined) => {
      sessionName = name;
    },
    transcriptLength: transcript.length,
    msg: addMessage,
    render: () => tui.requestRender(),
    refreshHeader: updateHeader,
    refreshFooter: updateFooter,
    resync: syncSessionTranscript,
    doResume: resumeSession,
    doNewSession: createNewSession,
    doTreeSelector: () => openTreeSelector(overlayCtx),
    doForkSelector: () => openForkSelector(overlayCtx),
    doClone: cloneSessionCmd,
    doFork: forkSessionCmd,
    doResumeSelector: () => openResumeSelector(overlayCtx),
    listModels: listAvailableModels,
    formatSessions: formatSessionTreeLines,
  };
  tui.addChild(chatBox);
  tui.addChild(spinner);
  tui.addChild(statusLine);
  tui.addChild(footerComponent);
  tui.addChild(new Text("─".repeat(80)));
  tui.addChild(editor);
  tui.setFocus(editor);

  terminal.setTitle("piko");

  // Ctrl+C during streaming: abort current request
  process.on("SIGINT", () => {
    if (abortController && !abortController.signal.aborted) {
      abortController.abort();
      spinner.stop();
      statusLine.setStatus("Interrupted");
    } else if (!abortController) {
      process.exit(0);
    }
  });

  if (host.sessionFile) {
    await resumeSession();
  } else {
    addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help");
    updateHeader();
    updateFooter();
    rebuildChat();
  }

  tui.start();
}

async function runStreaming(
  host: PikoHost,
  prompt: string,
  signal: AbortSignal,
  handlers: {
    onAssistantDelta: (text: string) => void;
    onThinkingDelta: (delta: string) => void;
    onToolCallStart: (name: string, args: unknown) => void;
    onToolCallEnd: (name: string, result: unknown, isError: boolean) => void;
  },
): Promise<{
  text: string;
  messages: import("piko-engine-protocol").Message[];
  status: StreamPromptResult["status"];
}> {
  const stream = host.streamPrompt(
    prompt,
    {
      settingsOverride: {
        maxSteps: 10,
        parallelTools: false,
        allowToolCalls: true,
        allowApprovals: false,
      },
    },
    signal,
  );
  let text = "";
  const toolCallNames = new Map<string, string>();

  for await (const event of stream) {
    if (event.type === "message_delta") {
      text += (event as { delta: string }).delta;
      handlers.onAssistantDelta(text);
    } else if (event.type === "thinking_delta") {
      handlers.onThinkingDelta(event.delta);
    } else if (event.type === "tool_call_start") {
      toolCallNames.set(event.id, event.name);
      handlers.onToolCallStart(event.name, event.args);
    } else if (event.type === "tool_call_end") {
      const toolName = toolCallNames.get(event.id) ?? event.id;
      handlers.onToolCallEnd(toolName, event.result, event.isError);
    }
  }

  const result = await stream.result();
  for (const msg of result.appendedMessages) {
    if (msg.role === "assistant") {
      for (const block of msg.content) {
        if (block.type === "text") {
          text = block.text;
        }
      }
    }
  }

  return {
    text: text || "(empty response)",
    messages: (result as StreamPromptResult).messages,
    status: result.status,
  };
}
