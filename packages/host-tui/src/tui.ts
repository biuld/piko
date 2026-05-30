import {
  TUI,
  ProcessTerminal,
  Editor,
  Markdown,
  Box,
  Text,
  getKeybindings,
  type AutocompleteProvider,
  type AutocompleteSuggestions,
} from "@earendil-works/pi-tui";
import type {
  EngineModel,
  EngineProviderConfig,
  Message,
} from "piko-engine-protocol";
import {
  PikoHost,
  createHostConfig,
  createDefaultSettings,
  createPiLlmCaller,
  listAvailableModels,
  PikoSessionRuntime,
  SessionManager,
  type SessionMeta,
  type StreamPromptResult,
} from "piko-host-runtime";
import { createNativeEngine } from "piko-engine-native";
import { SelectorOverlay } from "./selector-overlay.js";
import { PromptOverlay } from "./prompt-overlay.js";
import { createThreadedSessionSelectItems, formatSessionTreeLines } from "./session-tree.js";
import { getEditorTheme, getMarkdownTheme } from "./theme.js";

export interface RunTuiOptions {
  session?: string;
}

const COMMANDS = [
  { value: "/help", label: "/help", description: "Show help" },
  { value: "/model", label: "/model", description: "Show current model" },
  { value: "/models", label: "/models", description: "List available models" },
  { value: "/sessions", label: "/sessions", description: "List saved sessions" },
  { value: "/name", label: "/name <title>", description: "Set the current session title" },
  { value: "/tree", label: "/tree [entry-id]", description: "Show or switch the current branch" },
  { value: "/fork", label: "/fork <entry-id>", description: "Create a new session from an earlier user message" },
  { value: "/clone", label: "/clone", description: "Duplicate the current branch into a new session" },
  { value: "/resume", label: "/resume <id>", description: "Resume a saved session" },
  { value: "/session", label: "/session", description: "Show current session info" },
  { value: "/new", label: "/new", description: "Start a new session" },
  { value: "/clear", label: "/clear", description: "Clear chat" },
  { value: "/exit", label: "/exit", description: "Exit piko" },
];

function createAutocomplete(): AutocompleteProvider {
  return {
    async getSuggestions(lines: string[], cursorLine: number, cursorCol: number): Promise<AutocompleteSuggestions | null> {
      const line = lines[cursorLine] ?? "";
      const prefix = line.slice(0, cursorCol);
      if (!prefix.startsWith("/")) return null;
      return {
        items: COMMANDS.filter((c) => c.value.startsWith(prefix)),
        prefix: "/",
      };
    },
    applyCompletion(
      lines: string[], cursorLine: number, _cursorCol: number,
      item: { value: string; label: string }, prefix: string,
    ) {
      const line = lines[cursorLine] ?? "";
      const slashIdx = line.indexOf(prefix);
      const before = line.slice(0, slashIdx);
      const newLine = before + item.value + " ";
      return { lines: [newLine], cursorLine, cursorCol: newLine.length };
    },
  };
}

export async function runTui(
  model: EngineModel,
  providerConfig: EngineProviderConfig,
  options: RunTuiOptions = {},
): Promise<void> {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  const sessionRuntime = await PikoSessionRuntime.create({ session: options.session });
  let sessionManager: SessionManager = sessionRuntime.getSessionManager();
  const messages: Array<{ role: string; text: string }> = [];
  let transcript = await sessionManager.loadMessages();
  let sessionName = await sessionManager.getSessionName();
  let running = false;
  let host = createStreamingHost(model, providerConfig, sessionManager);
  let activeOverlay: { hide(): void } | null = null;

  sessionRuntime.setOnSessionReplaced(() => {
    sessionManager = sessionRuntime.getSessionManager();
  });

  function createStreamingHost(
    currentModel: EngineModel,
    currentProviderConfig: EngineProviderConfig,
    currentSessionManager: SessionManager,
  ): PikoHost {
    return new PikoHost({
      engine: createNativeEngine({ llmCaller: createPiLlmCaller() }),
      config: createHostConfig(
        currentModel,
        currentProviderConfig,
        createDefaultSettings({
          maxSteps: 1,
          parallelTools: false,
          allowToolCalls: false,
          allowApprovals: false,
          stopConditions: { stopOnAssistantMessage: true },
        }),
      ),
      systemPrompt: "You are a helpful assistant. Be concise.",
      sessionManager: currentSessionManager,
      cwd: currentSessionManager.getCwd(),
    });
  }

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

  async function syncSessionTranscript(systemMessage?: string): Promise<void> {
    const loaded = await sessionManager.loadMessages();
    sessionName = await sessionManager.getSessionName();
    transcript = [...loaded];
    host = createStreamingHost(model, providerConfig, sessionManager);
    updateHeader();
    messages.length = 0;
    for (const msg of loaded) {
      if (msg.role === "user") {
        addMessage("user", typeof msg.content === "string" ? msg.content : "");
      } else if (msg.role === "assistant") {
        const text = Array.isArray(msg.content)
          ? msg.content.filter((c): c is { type: "text"; text: string } => c.type === "text").map((c) => c.text).join("\n")
          : "";
        addMessage("assistant", text);
      }
    }
    if (systemMessage) {
      addMessage("system", systemMessage);
    }
    rebuildChat();
    tui.requestRender();
  }

  async function resumeSession(nextSessionManager: SessionManager): Promise<void> {
    const loaded = await nextSessionManager.loadMessages();
    if (loaded.length === 0) {
      addMessage("system", `Session ${nextSessionManager.getSessionId()} not found or empty`);
      rebuildChat();
      tui.requestRender();
      return;
    }

    sessionManager = nextSessionManager;
    await syncSessionTranscript(`Resumed session ${sessionManager.getSessionId()} (${loaded.length} messages)`);
  }

  async function createNewSession(): Promise<void> {
    await sessionRuntime.newSession();
    sessionManager = sessionRuntime.getSessionManager();
    await syncSessionTranscript("New session  |  Ctrl+D submit  Ctrl+C exit  /help");
  }

  function formatEntrySummary(entry: Awaited<ReturnType<SessionManager["getTree"]>>[number]): string {
    if (entry.type === "model_change") {
      return `model ${entry.modelId}`;
    }
    if (entry.type === "session_info") {
      return `title ${entry.name ?? "(cleared)"}`;
    }
    if (entry.message.role === "user") {
      return `user ${typeof entry.message.content === "string" ? entry.message.content : ""}`;
    }
    const text = entry.message.content
      .filter((block) => block.type === "text")
      .map((block) => block.text)
      .join(" ");
    return `assistant ${text}`;
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

    async function loadSessions(): Promise<SessionMeta[]> {
      const all = scope === "current"
        ? await SessionManager.list(sessionManager.getCwd())
        : await SessionManager.listAll();
      return namedOnly ? all.filter((session) => Boolean(session.name)) : all;
    }

    let sessions = await loadSessions();
    if (sessions.length === 0) {
      const allSessions = await SessionManager.listAll();
      if (allSessions.length === 0 && !namedOnly) {
        addMessage("system", "No saved sessions. /resume <id> to load");
        rebuildChat();
        tui.requestRender();
        return;
      }
      scope = "all";
      sessions = await loadSessions();
      if (sessions.length === 0) {
        addMessage("system", namedOnly ? "No named sessions found" : "No saved sessions. /resume <id> to load");
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
        void SessionManager.open(item.value, sessionManager.getCwd()).then((resolved) => {
          closeOverlay();
          if (!resolved) {
            addMessage("system", `Session ${item.label} not found`);
            rebuildChat();
            tui.requestRender();
            return;
          }
          void sessionRuntime.switchSession(resolved.getSessionFile() ?? item.value).then((nextSessionManager) => {
            if (nextSessionManager) {
              void resumeSession(nextSessionManager);
            }
          });
        });
      },
      () => closeOverlay(),
      (data) => {
        const kb = getKeybindings();
        const toggleNamedFilterKey = "app.session.toggleNamedFilter" as Parameters<typeof kb.matches>[1];
        if (!kb.matches(data, "tui.input.tab") && !kb.matches(data, toggleNamedFilterKey) && data !== "\u0012" && data !== "\u0004") {
          return false;
        }
        void (async () => {
          if (data === "\u0012") {
            const selected = createThreadedSessionSelectItems(sessions).find((item) => item.value === overlay.getSelectedValue());
            if (!selected) return;
            const currentName = sessions.find((session) => session.path === selected.value)?.name ?? "";
            const prompt = new PromptOverlay(
              "Rename Session",
              currentName,
              "Enter save  Esc cancel",
              (value) => {
                void SessionManager.rename(selected.value, value, sessionManager.getCwd()).then(async () => {
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
            if (selectedValue === sessionManager.getSessionFile()) {
              addMessage("system", "Cannot delete the current active session");
              rebuildChat();
              tui.requestRender();
              return;
            }
            await SessionManager.delete(selectedValue, sessionManager.getCwd());
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
    const tree = await sessionManager.getTree();
    if (tree.length === 0) {
      addMessage("system", "Current session has no saved entries yet");
      rebuildChat();
      tui.requestRender();
      return;
    }

    const items = tree.map((entry) => {
      const prefix = entry.isLeaf ? "* " : entry.isOnCurrentBranch ? "| " : "  ";
      return {
        value: entry.id,
        label: `${prefix}${entry.id}`,
        description: formatEntrySummary(entry).slice(0, 80),
      };
    });
    const overlay = new SelectorOverlay(
      "Session Tree",
      items,
      "Enter switch  Esc cancel  ↑↓ select",
      (item) => {
        void sessionManager.branch(item.value).then(async () => {
          closeOverlay();
          await syncSessionTranscript(`Switched branch to ${sessionManager.getLeafId()}`);
        }).catch((error: unknown) => {
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

  async function cloneSession(): Promise<void> {
    if (!sessionManager.isPersisted()) {
      addMessage("system", "Clone requires a saved session");
      rebuildChat();
      tui.requestRender();
      return;
    }
    await sessionRuntime.cloneSession();
    sessionManager = sessionRuntime.getSessionManager();
    await syncSessionTranscript(`Cloned branch into session ${sessionManager.getSessionId()}`);
  }

  async function forkSession(entryId: string): Promise<void> {
    if (!sessionManager.isPersisted()) {
      addMessage("system", "Fork requires a saved session");
      rebuildChat();
      tui.requestRender();
      return;
    }
    const result = await sessionRuntime.forkSession(entryId);
    sessionManager = sessionRuntime.getSessionManager();
    const suffix = result.selectedText ? `\nOriginal prompt: ${result.selectedText}` : "";
    await syncSessionTranscript(`Forked into session ${sessionManager.getSessionId()}${suffix}`);
    if (result.selectedText) {
      editor.setText(result.selectedText);
    }
  }

  async function openForkSelector(): Promise<void> {
    if (!sessionManager.isPersisted()) {
      addMessage("system", "Fork requires a saved session");
      rebuildChat();
      tui.requestRender();
      return;
    }

    const branch = await sessionManager.getBranch();
    const items = branch
      .filter((entry): entry is Extract<(typeof branch)[number], { type: "message" }> => entry.type === "message")
      .filter((entry) => entry.message.role === "user")
      .map((entry) => ({
        value: entry.id,
        label: entry.id,
        description: typeof entry.message.content === "string"
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
        void forkSession(item.value).then(() => {
          closeOverlay();
        }).catch((error: unknown) => {
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
    headerBox.addChild(new Text(` piko  ${model.provider}/${model.id}  session ${sessionName ?? sessionManager.getSessionId().slice(-8)}  ${transcript.length} msgs `));
  }

  const chatBox = new Box(0, 0);

  const footerBox = new Box(0, 0);
  footerBox.addChild(new Text(" Ctrl+D submit  Ctrl+C exit  /help  /new  /session  /exit "));

  const editor = new Editor(tui, getEditorTheme());
  editor.setAutocompleteProvider(createAutocomplete());
  editor.onSubmit = (text: string) => {
    if (running) return;
    const trimmed = text.trim();
    if (!trimmed) return;

    if (trimmed.startsWith("/")) {
      const parts = trimmed.split(/\s+/);
      const cmd = parts[0].toLowerCase();
      if (cmd === "/exit") { process.exit(0); }
      if (cmd === "/clear" || cmd === "/new") { void createNewSession(); return; }
      if (cmd === "/help") {
        addMessage("system", COMMANDS.map((c) => `${c.value} — ${c.description}`).join("\n"));
        rebuildChat(); tui.requestRender(); return;
      }
      if (cmd === "/model") {
        addMessage("system", `${model.provider}/${model.id} — ${model.name}`);
        rebuildChat(); tui.requestRender(); return;
      }
      if (cmd === "/models") {
        const models = listAvailableModels();
        addMessage("system", models.flatMap((p) => p.models.map((m) => `${p.provider}/${m.id}`)).join("\n"));
        rebuildChat(); tui.requestRender(); return;
      }
      if (cmd === "/sessions") {
        void SessionManager.list(sessionManager.getCwd()).then((sessions: SessionMeta[]) => {
          if (sessions.length === 0) {
            addMessage("system", "No saved sessions. /resume <id> to load");
          } else {
            const lines = formatSessionTreeLines(sessions);
            addMessage("system", `Sessions:\n${lines.join("\n")}\n\n/resume <id> to load`);
          }
          rebuildChat(); tui.requestRender();
        });
        return;
      }
      if (cmd === "/name") {
        const title = trimmed.slice("/name".length).trim();
        void sessionManager.setSessionName(title || undefined).then(() => {
          sessionName = title || undefined;
          updateHeader();
          addMessage("system", title ? `Session renamed to: ${title}` : "Session title cleared");
          rebuildChat();
          tui.requestRender();
        }).catch((error: unknown) => {
          const message = error instanceof Error ? error.message : String(error);
          addMessage("system", message);
          rebuildChat();
          tui.requestRender();
        });
        return;
      }
      if (cmd === "/tree") {
        const entryId = parts[1];
        if (!entryId) {
          void openTreeSelector();
          return;
        }
        void sessionManager.branch(entryId).then(async () => {
          await syncSessionTranscript(`Switched branch to ${sessionManager.getLeafId()}`);
        }).catch((error: unknown) => {
          const message = error instanceof Error ? error.message : String(error);
          addMessage("system", message);
          rebuildChat();
          tui.requestRender();
        });
        return;
      }
      if (cmd === "/clone") {
        void cloneSession().catch((error: unknown) => {
          const message = error instanceof Error ? error.message : String(error);
          addMessage("system", message);
          rebuildChat();
          tui.requestRender();
        });
        return;
      }
      if (cmd === "/fork") {
        const entryId = parts[1];
        if (!entryId) {
          void openForkSelector();
          return;
        }
        void forkSession(entryId).catch((error: unknown) => {
          const message = error instanceof Error ? error.message : String(error);
          addMessage("system", message);
          rebuildChat();
          tui.requestRender();
        });
        return;
      }
      if (cmd === "/session") {
        void sessionManager.getSessionName().then((currentSessionName) => {
          addMessage(
            "system",
            [
              `Session ID: ${sessionManager.getSessionId()}`,
              `Session Name: ${currentSessionName ?? "(none)"}`,
              `Session File: ${sessionManager.getSessionFile() ?? "(new session)"}`,
              `Parent Session: ${sessionManager.getParentSessionPath() ?? "(none)"}`,
              `CWD: ${sessionManager.getCwd()}`,
              `Messages: ${transcript.length}`,
              `Leaf: ${sessionManager.getLeafId() ?? "(none)"}`,
              `Model: ${model.provider}/${model.id}`,
            ].join("\n"),
          );
          rebuildChat();
          tui.requestRender();
        });
        return;
      }
      if (cmd === "/resume") {
        const id = parts[1];
        if (id) {
          void SessionManager.open(id, sessionManager.getCwd()).then((resolved) => {
            if (!resolved) {
              addMessage("system", `Session ${id} not found`);
              rebuildChat();
              tui.requestRender();
              return;
            }
            void sessionRuntime.switchSession(resolved.getSessionFile() ?? id).then((nextSessionManager) => {
              if (nextSessionManager) {
                void resumeSession(nextSessionManager);
              }
            });
          });
        } else {
          void openResumeSelector();
        }
        return;
      }
      addMessage("system", `Unknown: ${cmd}`);
      rebuildChat();
      tui.requestRender();
      return;
    }

    running = true;
    addMessage("user", trimmed);
    const assistIdx = messages.length;
    addMessage("assistant", "");
    rebuildChat();
    tui.requestRender();

    void runStreaming(host, trimmed, (partial) => {
      messages[assistIdx] = { role: "assistant", text: partial };
      rebuildChat();
      tui.requestRender();
    }).then((result) => {
      const final = result.text;
      messages[assistIdx] = { role: "assistant", text: final };
      transcript = result.messages;
      updateHeader();
      running = false;
      rebuildChat();
      tui.requestRender();
    });
  };

  tui.addChild(headerBox);
  tui.addChild(chatBox);
  tui.addChild(footerBox);
  tui.addChild(new Text("─".repeat(80)));
  tui.addChild(editor);
  tui.setFocus(editor);

  terminal.setTitle("piko");

  if (options.session && sessionManager.getSessionFile()) {
    await resumeSession(sessionManager);
  } else {
    if (sessionManager.getSessionFile()) {
      await resumeSession(sessionManager);
    } else {
      addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help");
      updateHeader();
      rebuildChat();
    }
  }

  tui.start();
}

async function runStreaming(
  host: PikoHost,
  prompt: string,
  onPartial: (text: string) => void,
): Promise<{ text: string; messages: Message[] }> {
  const stream = host.streamPrompt(prompt, {
    settingsOverride: {
      maxSteps: 1,
      parallelTools: false,
      allowToolCalls: false,
      allowApprovals: false,
      stopConditions: { stopOnAssistantMessage: true },
    },
  });
  let text = "";

  for await (const event of stream) {
    if (event.type === "message_delta") {
      text += (event as { delta: string }).delta;
      onPartial(text);
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
  };
}
