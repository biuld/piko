import type { Model } from "@earendil-works/pi-ai";
import {
  type AutocompleteProvider,
  type AutocompleteSuggestions,
  Box,
  Editor,
  getKeybindings,
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
import { buildSessionTree, TreeSelectorComponent } from "./components/tree-selector.js";
import { PromptOverlay } from "./prompt-overlay.js";
import { SelectorOverlay } from "./selector-overlay.js";
import { createThreadedSessionSelectItems, formatSessionTreeLines } from "./session-tree.js";
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

  function closeOverlay(): void {
    activeOverlay?.hide();
    activeOverlay = null;
    tui.setFocus(editor);
    tui.requestRender();
  }

  async function openResumeSelector(): Promise<void> {
    let scope: "current" | "all" = "current";
    let namedOnly = false;

    async function loadSessions() {
      return host.listSessions({ scope, namedOnly });
    }

    let sessions = await loadSessions();
    if (sessions.length === 0) {
      const allSessions = await host.listSessions({ scope: "all" });
      if (allSessions.length === 0 && !namedOnly) {
        addMessage("system", "No saved sessions. /resume <id> to load");
        rebuildChat();
        tui.requestRender();
        return;
      }
      scope = "all";
      sessions = await loadSessions();
      if (sessions.length === 0) {
        addMessage(
          "system",
          namedOnly ? "No named sessions found" : "No saved sessions. /resume <id> to load",
        );
        rebuildChat();
        tui.requestRender();
        return;
      }
    }

    const updateOverlayState = (overlay: SelectorOverlay): void => {
      overlay.setTitle(scope === "current" ? "Resume Session (Current)" : "Resume Session (All)");
      overlay.setItems(createThreadedSessionSelectItems(sessions));
      overlay.setFooterLines([
        "Enter resume  Tab scope  Ctrl+N named-only  Ctrl+R rename  Ctrl+D delete  Esc cancel",
        `Scope: ${scope === "current" ? "current" : "all"}  Name: ${namedOnly ? "named" : "all"}`,
      ]);
    };

    const overlay = new SelectorOverlay(
      "",
      createThreadedSessionSelectItems(sessions),
      "",
      (item) => {
        void host.switchSession(item.value).then((resolved) => {
          closeOverlay();
          if (!resolved) {
            addMessage("system", `Session ${item.label} not found`);
            rebuildChat();
            tui.requestRender();
            return;
          }
          void resumeSession();
        });
      },
      () => closeOverlay(),
      (data) => {
        const kb = getKeybindings();
        const toggleNamedFilterKey = "app.session.toggleNamedFilter" as Parameters<
          typeof kb.matches
        >[1];
        if (
          !kb.matches(data, "tui.input.tab") &&
          !kb.matches(data, toggleNamedFilterKey) &&
          data !== "\u0012" &&
          data !== "\u0004"
        ) {
          return false;
        }
        void (async () => {
          if (data === "\u0012") {
            const selected = createThreadedSessionSelectItems(sessions).find(
              (item) => item.value === overlay.getSelectedValue(),
            );
            if (!selected) return;
            const currentName = sessions.find((s) => s.path === selected.value)?.name ?? "";
            const prompt = new PromptOverlay(
              "Rename Session",
              currentName,
              "Enter save  Esc cancel",
              (value) => {
                void host.renameSession(selected.value, value).then(async () => {
                  sessions = await loadSessions();
                  updateOverlayState(overlay);
                  tui.hideOverlay();
                  tui.requestRender();
                });
              },
              () => {
                tui.hideOverlay();
                tui.requestRender();
              },
            );
            tui.showOverlay(prompt, {
              anchor: "center",
              width: "70%",
              maxHeight: "30%",
            });
            return;
          }
          if (data === "\u0004") {
            const selectedValue = overlay.getSelectedValue();
            if (!selectedValue) return;
            if (selectedValue === host.sessionFile) {
              addMessage("system", "Cannot delete the current active session");
              rebuildChat();
              tui.requestRender();
              return;
            }
            await host.deleteSession(selectedValue);
            sessions = await loadSessions();
            updateOverlayState(overlay);
            tui.requestRender();
            return;
          }

          if (kb.matches(data, "tui.input.tab")) {
            scope = scope === "current" ? "all" : "current";
          } else {
            namedOnly = !namedOnly;
          }
          sessions = await loadSessions();
          updateOverlayState(overlay);
          tui.requestRender();
        })();
        return true;
      },
    );
    updateOverlayState(overlay);
    activeOverlay = tui.showOverlay(overlay, {
      anchor: "center",
      width: "80%",
      maxHeight: "60%",
    });
  }

  async function openTreeSelector(): Promise<void> {
    const treeEntries = await host.getTreeEntries();
    if (treeEntries.length === 0) {
      addMessage("system", "Current session has no saved entries yet");
      rebuildChat();
      tui.requestRender();
      return;
    }

    const tree = buildSessionTree(treeEntries);
    const component = new TreeSelectorComponent(
      tree,
      host.getLeafId(),
      process.stdout.rows ?? 40,
      (entryId) => {
        void host
          .branchToEntry(entryId)
          .then(async () => {
            closeOverlay();
            await syncSessionTranscript(`Switched branch to ${host.getLeafId()}`);
          })
          .catch((error: unknown) => {
            closeOverlay();
            const message = error instanceof Error ? error.message : String(error);
            addMessage("system", message);
            rebuildChat();
            tui.requestRender();
          });
      },
      () => closeOverlay(),
    );
    activeOverlay = tui.showOverlay(component, {
      anchor: "center",
      width: "80%",
      maxHeight: "70%",
    });
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
    if (result.selectedText) {
      editor.setText(result.selectedText);
    }
  }

  async function openForkSelector(): Promise<void> {
    if (!host.isSessionPersisted()) {
      addMessage("system", "Fork requires a saved session");
      rebuildChat();
      tui.requestRender();
      return;
    }

    const branch = await host.getBranchEntries();
    const items = branch
      .filter(
        (entry): entry is Extract<(typeof branch)[number], { type: "message" }> =>
          entry.type === "message",
      )
      .filter((entry) => entry.message.role === "user")
      .map((entry) => ({
        value: entry.id,
        label: entry.id,
        description:
          typeof entry.message.content === "string"
            ? entry.message.content.slice(0, 120)
            : entry.message.content
                .filter((block) => block.type === "text")
                .map((block) => block.text)
                .join(" ")
                .slice(0, 120),
      }))
      .reverse();

    if (items.length === 0) {
      addMessage("system", "Current branch has no user messages to fork from");
      rebuildChat();
      tui.requestRender();
      return;
    }

    const overlay = new SelectorOverlay(
      "Fork From User Message",
      items,
      "Enter fork  Esc cancel  ↑↓ select",
      (item) => {
        void forkSessionCmd(item.value)
          .then(() => closeOverlay())
          .catch((error: unknown) => {
            closeOverlay();
            const message = error instanceof Error ? error.message : String(error);
            addMessage("system", message);
            rebuildChat();
            tui.requestRender();
          });
      },
      () => closeOverlay(),
    );
    activeOverlay = tui.showOverlay(overlay, {
      anchor: "center",
      width: "80%",
      maxHeight: "60%",
    });
  }

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
    doTreeSelector: openTreeSelector,
    doForkSelector: openForkSelector,
    doClone: cloneSessionCmd,
    doFork: forkSessionCmd,
    doResumeSelector: openResumeSelector,
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
