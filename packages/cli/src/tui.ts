import {
  TUI,
  ProcessTerminal,
  Editor,
  Markdown,
  Box,
  Text,
  type AutocompleteProvider,
  type AutocompleteSuggestions,
} from "@earendil-works/pi-tui";
import type {
  EngineModel,
  EngineProviderConfig,
  EngineInput,
} from "piko-engine-protocol";
import { createNativeEngine } from "piko-engine-native";
import {
  createPiLlmCaller,
  listAvailableModels,
} from "piko-host-runtime";
import { saveSession, loadSession, listSessions } from "./config.js";
import { getEditorTheme, getMarkdownTheme } from "./theme.js";

// ---- Slash autocomplete ----

const COMMANDS = [
  { value: "/help", label: "/help", description: "Show help" },
  { value: "/model", label: "/model", description: "Show current model" },
  { value: "/models", label: "/models", description: "List available models" },
  { value: "/sessions", label: "/sessions", description: "List saved sessions" },
  { value: "/resume", label: "/resume <id>", description: "Resume a saved session" },
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

// ---- TUI App ----

export async function runTui(
  model: EngineModel,
  providerConfig: EngineProviderConfig,
): Promise<void> {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  let sessionId = `session-${Date.now()}`;

  const messages: Array<{ role: string; text: string }> = [];
  const transcript: EngineInput["transcript"] = [];
  let running = false;

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

  async function resumeSession(id: string): Promise<void> {
    const loaded = await loadSession(id);
    if (loaded.length === 0) {
      addMessage("system", `Session ${id} not found or empty`);
    } else {
      sessionId = id;
      transcript.length = 0;
      transcript.push(...loaded);
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
      addMessage("system", `Resumed session ${id} (${loaded.length} messages)`);
    }
    rebuildChat();
    tui.requestRender();
  }

  // Components
  const headerBox = new Box(0, 0);
  headerBox.addChild(new Text(`piko  ${model.provider}/${model.id}  ~/.piko`));

  const chatBox = new Box(0, 0);

  const footerBox = new Box(0, 0);
  footerBox.addChild(new Text(" Ctrl+D submit  Ctrl+C exit  /help  /clear  /exit "));

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
      if (cmd === "/clear") { messages.length = 0; rebuildChat(); tui.requestRender(); return; }
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
        void listSessions().then((s) => {
          const list = s.length > 0
            ? s.map((id, i) => `${i + 1}. ${id}`).join("\n")
            : "(none)";
          addMessage("system", `Saved sessions:\n${list}\n/resume <id> to load`);
          rebuildChat(); tui.requestRender();
        });
        return;
      }
      if (cmd === "/resume") {
        const id = parts[1];
        if (id) {
          void resumeSession(id);
        } else {
          addMessage("system", "Usage: /resume <session-id>");
          rebuildChat(); tui.requestRender();
        }
        return;
      }
      addMessage("system", `Unknown: ${cmd}`);
      rebuildChat();
      tui.requestRender();
      return;
    }

    // Run
    running = true;
    addMessage("user", trimmed);
    transcript.push({ role: "user", content: trimmed, timestamp: Date.now() });
    const assistIdx = messages.length;
    addMessage("assistant", "");
    rebuildChat();
    tui.requestRender();

    void runStreaming(model, providerConfig, transcript, (partial) => {
      messages[assistIdx] = { role: "assistant", text: partial };
      rebuildChat();
      tui.requestRender();
    }).then((final) => {
      messages[assistIdx] = { role: "assistant", text: final };
      void saveSession(sessionId, transcript);
      running = false;
      rebuildChat();
      tui.requestRender();
    });
  };

  // Layout
  tui.addChild(headerBox);
  tui.addChild(chatBox);
  tui.addChild(footerBox);
  tui.addChild(new Text("─".repeat(80)));
  tui.addChild(editor);
  tui.setFocus(editor);

  terminal.setTitle("piko");

  // Auto-load latest session on startup
  const sessions = await listSessions();
  if (sessions.length > 0) {
    const latest = sessions[sessions.length - 1];
    await resumeSession(latest);
  } else {
    addMessage("system", `New session ${sessionId}  |  Ctrl+D submit  Ctrl+C exit  /help`);
    rebuildChat();
  }

  tui.start();
}

// ---- Streaming runner ----

async function runStreaming(
  model: EngineModel,
  providerConfig: EngineProviderConfig,
  transcript: EngineInput["transcript"],
  onPartial: (text: string) => void,
): Promise<string> {
  const engine = createNativeEngine({ llmCaller: createPiLlmCaller() });

  const stream = engine.executeStep({
    runId: "tui-run",
    stepId: `step-${Date.now()}`,
    systemPrompt: "You are a helpful assistant. Be concise.",
    model,
    provider: providerConfig,
    tools: [],
    transcript,
    settings: {
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
    transcript.push(msg);
    if (msg.role === "assistant") {
      for (const block of Array.isArray(msg.content) ? msg.content : []) {
        if (block.type === "text") {
          text = block.text;
        }
      }
    }
  }

  return text || "(empty response)";
}
