import {
  TUI,
  ProcessTerminal,
  Editor,
  Markdown,
  Box,
  Text,
  type EditorTheme,
  type MarkdownTheme,
  type AutocompleteProvider,
  type AutocompleteSuggestions,
} from "@earendil-works/pi-tui";
import type {
  EngineModel,
  EngineProviderConfig,
  EngineInput,
  EngineStepResult,
} from "piko-engine-protocol";
import { createNativeEngine } from "piko-engine-native";
import {
  createPiLlmCaller,
  listAvailableModels,
} from "piko-host-runtime";
import { saveSession, listSessions } from "./config.js";

// ---- Minimal themes ----

const id = (s: string): string => s;

const editorTheme: EditorTheme = {
  borderColor: id,
  selectList: {
    selectedPrefix: id,
    selectedText: id,
    description: id,
    scrollInfo: id,
    noMatch: id,
  },
};

const markdownTheme: MarkdownTheme = {
  heading: id,
  link: id,
  linkUrl: id,
  code: id,
  codeBlock: id,
  codeBlockBorder: id,
  quote: id,
  quoteBorder: id,
  hr: id,
  listBullet: id,
  bold: id,
  italic: id,
  strikethrough: id,
  underline: id,
};

// ---- Slash autocomplete ----

const COMMANDS = [
  { value: "/help", label: "/help", description: "Show help" },
  { value: "/model", label: "/model", description: "Show current model" },
  { value: "/models", label: "/models", description: "List available models" },
  { value: "/sessions", label: "/sessions", description: "List saved sessions" },
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
  const sessionId = `session-${Date.now()}`;

  const messages: Array<{ role: string; text: string }> = [];
  let running = false;

  function addMessage(role: string, text: string): void {
    messages.push({ role, text });
    if (messages.length > 50) messages.shift();
  }

  function rebuildChat(): void {
    chatBox.clear();
    for (const msg of messages) {
      if (msg.role === "user") {
        chatBox.addChild(new Markdown(`**You:** ${msg.text}`, 0, 0, markdownTheme));
      } else if (msg.role === "assistant") {
        chatBox.addChild(new Markdown(msg.text || "…", 0, 0, markdownTheme));
      } else if (msg.role === "system") {
        chatBox.addChild(new Text(msg.text));
      }
    }
  }

  // Components
  const headerBox = new Box(0, 0);
  headerBox.addChild(new Text(`piko  ${model.provider}/${model.id}  ~/.piko`));

  const chatBox = new Box(0, 0);

  const editor = new Editor(tui, editorTheme);
  editor.setAutocompleteProvider(createAutocomplete());
  editor.onSubmit = (text: string) => {
    if (running) return;
    const trimmed = text.trim();
    if (!trimmed) return;

    if (trimmed.startsWith("/")) {
      const cmd = trimmed.split(/\s+/)[0].toLowerCase();
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
          addMessage("system", s.length > 0 ? s.join("\n") : "(none)");
          rebuildChat(); tui.requestRender();
        });
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
    const assistIdx = messages.length;
    addMessage("assistant", "");
    rebuildChat();
    tui.requestRender();

    void runStreaming(model, providerConfig, trimmed, (partial) => {
      messages[assistIdx] = { role: "assistant", text: partial };
      rebuildChat();
      tui.requestRender();
    }).then((final) => {
      messages[assistIdx] = { role: "assistant", text: final };
      void saveSession(sessionId, [
        { role: "user", content: trimmed, timestamp: Date.now() },
        { role: "assistant", content: [{ type: "text", text: final }], timestamp: Date.now() },
      ]);
      running = false;
      rebuildChat();
      tui.requestRender();
    });
  };

  // Layout
  tui.addChild(headerBox);
  tui.addChild(chatBox);
  tui.addChild(new Text("─".repeat(80)));
  tui.addChild(editor);
  tui.setFocus(editor);

  terminal.setTitle("piko");

  addMessage("system", `Session ${sessionId}  |  Ctrl+D submit  Ctrl+C exit  /help`);
  rebuildChat();

  tui.start();
}

// ---- Streaming runner ----

async function runStreaming(
  model: EngineModel,
  providerConfig: EngineProviderConfig,
  prompt: string,
  onPartial: (text: string) => void,
): Promise<string> {
  const engine = createNativeEngine({ llmCaller: createPiLlmCaller() });

  const baseInput: Omit<EngineInput, "transcript" | "stepId"> = {
    runId: "tui-run",
    systemPrompt: "You are a helpful assistant. Be concise.",
    model,
    provider: providerConfig,
    tools: [],
    settings: {
      maxSteps: 1,
      parallelTools: false,
      allowToolCalls: false,
      allowApprovals: false,
      stopConditions: { stopOnAssistantMessage: true },
    },
  };

  const transcript: EngineInput["transcript"] = [
    { role: "user", content: prompt, timestamp: Date.now() },
  ];

  let allText = "";
  let stepCount = 0;

  while (stepCount < 5) {
    const stream = engine.executeStep({
      ...baseInput,
      transcript,
      stepId: `step-${stepCount}`,
    });

    let stepText = "";

    for await (const event of stream) {
      if (event.type === "message_delta") {
        stepText += (event as { delta: string }).delta;
        onPartial(allText + stepText);
      }
    }

    const result: EngineStepResult = await stream.result();

    if (result.appendedMessages.length > 0) {
      for (const msg of result.appendedMessages) {
        if (msg.role === "assistant") {
          for (const block of Array.isArray(msg.content) ? msg.content : []) {
            if (block.type === "text") {
              stepText = block.text;
            }
          }
          transcript.push(msg);
        }
      }
    }

    allText = stepText || allText;
    stepCount++;

    if (result.status === "completed" || result.status === "error") break;
  }

  return allText || "(empty response)";
}
