import type { PikoHost, SessionMeta } from "piko-host-runtime";

export interface Command {
  value: string;
  label: string;
  description: string;
}

/** The closures and state that slash commands need from the TUI. */
export interface CommandContext {
  host: PikoHost;
  model: { provider: string; id: string; name: string };
  sessionName: string | undefined;
  setSessionName: (name: string | undefined) => void;
  transcriptLength: number;
  msg: (role: string, text: string) => void;
  render: () => void;
  refreshHeader: () => void;
  refreshFooter: () => void;
  resync: (sysMsg?: string) => Promise<void>;
  doResume: () => Promise<void>;
  doNewSession: () => Promise<void>;
  doTreeSelector: () => Promise<void>;
  doForkSelector: () => Promise<void>;
  doClone: () => Promise<void>;
  doFork: (entryId: string) => Promise<void>;
  doResumeSelector: () => Promise<void>;
  doModelSelector: () => Promise<void>;
  cycleModelForward: () => Promise<void>;
  cycleModelBackward: () => Promise<void>;
  thinkingLevel: string;
  setThinkingLevel: (level: string) => void;
  listModels: () => { provider: string; models: { id: string; name: string }[] }[];
  formatSessions: (sessions: SessionMeta[]) => string[];
}

export const COMMANDS: Command[] = [
  { value: "/help", label: "/help", description: "Show help" },
  { value: "/model", label: "/model [next|prev]", description: "Show or switch model" },
  { value: "/models", label: "/models", description: "List available models" },
  { value: "/sessions", label: "/sessions", description: "List saved sessions" },
  { value: "/import", label: "/import <path>", description: "Import a session JSONL file" },
  { value: "/name", label: "/name <title>", description: "Set the current session title" },
  { value: "/tree", label: "/tree [entry-id]", description: "Show or switch the current branch" },
  {
    value: "/fork",
    label: "/fork <entry-id>",
    description: "Create a new session from an earlier user message",
  },
  {
    value: "/clone",
    label: "/clone",
    description: "Duplicate the current branch into a new session",
  },
  { value: "/resume", label: "/resume <id>", description: "Resume a saved session" },
  { value: "/session", label: "/session", description: "Show current session info" },
  { value: "/new", label: "/new", description: "Start a new session" },
  { value: "/clear", label: "/clear", description: "Clear chat" },
  {
    value: "/thinking",
    label: "/thinking [off|low|medium|high|xhigh]",
    description: "Set thinking level",
  },
  { value: "/exit", label: "/exit", description: "Exit piko" },
];

function showError(ctx: CommandContext, error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  ctx.msg("system", message);
  ctx.render();
}

export function handleSlashCommand(trimmed: string, ctx: CommandContext): void {
  const parts = trimmed.split(/\s+/);
  const cmd = parts[0].toLowerCase();

  if (cmd === "/exit") {
    process.exit(0);
  }
  if (cmd === "/clear" || cmd === "/new") {
    void ctx.doNewSession();
    return;
  }
  if (cmd === "/help") {
    ctx.msg("system", COMMANDS.map((c) => `${c.value} — ${c.description}`).join("\n"));
    ctx.render();
    return;
  }
  if (cmd === "/thinking") {
    const level = parts[1];
    const validLevels = ["off", "low", "medium", "high", "xhigh"];
    if (level && validLevels.includes(level)) {
      ctx.setThinkingLevel(level);
      ctx.msg("system", `Thinking level set to: ${level}`);
    } else if (level) {
      ctx.msg("system", `Invalid level. Use: ${validLevels.join(", ")}`);
    } else {
      ctx.msg(
        "system",
        `Current thinking level: ${ctx.thinkingLevel}. Use /thinking [${validLevels.join("|")}]`,
      );
    }
    ctx.render();
    return;
  }
  if (cmd === "/model") {
    const sub = parts[1];
    if (sub === "next") {
      void ctx.cycleModelForward();
    } else if (sub === "prev") {
      void ctx.cycleModelBackward();
    } else {
      void ctx.doModelSelector();
    }
    return;
  }
  if (cmd === "/models") {
    const models = ctx.listModels();
    ctx.msg(
      "system",
      models.flatMap((p) => p.models.map((m) => `${p.provider}/${m.id}`)).join("\n"),
    );
    ctx.render();
    return;
  }
  if (cmd === "/sessions") {
    void ctx.host.listSessions().then((sessions) => {
      if (sessions.length === 0) {
        ctx.msg("system", "No saved sessions. /resume <id> to load");
      } else {
        const lines = ctx.formatSessions(sessions);
        ctx.msg("system", `Sessions:\n${lines.join("\n")}\n\n/resume <id> to load`);
      }
      ctx.render();
    });
    return;
  }
  if (cmd === "/import") {
    const inputPath = trimmed.slice("/import".length).trim();
    if (!inputPath) {
      ctx.msg("system", "Usage: /import <session.jsonl>");
      ctx.render();
      return;
    }
    void ctx.host
      .importSession(inputPath)
      .then(() => ctx.doResume())
      .catch((e) => showError(ctx, e));
    return;
  }
  if (cmd === "/name") {
    const title = trimmed.slice("/name".length).trim();
    void ctx.host
      .setSessionName(title || undefined)
      .then(() => {
        ctx.setSessionName(title || undefined);
        ctx.refreshHeader();
        ctx.refreshFooter();
        ctx.msg("system", title ? `Session renamed to: ${title}` : "Session title cleared");
        ctx.render();
      })
      .catch((e) => showError(ctx, e));
    return;
  }
  if (cmd === "/tree") {
    const entryId = parts[1];
    if (!entryId) {
      void ctx.doTreeSelector();
      return;
    }
    void ctx.host
      .branchToEntry(entryId)
      .then(async () => {
        await ctx.resync(`Switched branch to ${ctx.host.getLeafId()}`);
      })
      .catch((e) => showError(ctx, e));
    return;
  }
  if (cmd === "/clone") {
    void ctx.doClone().catch((e) => showError(ctx, e));
    return;
  }
  if (cmd === "/fork") {
    const entryId = parts[1];
    if (!entryId) {
      void ctx.doForkSelector();
      return;
    }
    void ctx.doFork(entryId).catch((e) => showError(ctx, e));
    return;
  }
  if (cmd === "/session") {
    void ctx.host.getSessionName().then((currentSessionName) => {
      ctx.msg(
        "system",
        [
          `Session ID: ${ctx.host.sessionId}`,
          `Session Name: ${currentSessionName ?? "(none)"}`,
          `Session File: ${ctx.host.sessionFile ?? "(new session)"}`,
          `Parent Session: ${ctx.host.getParentSessionPath() ?? "(none)"}`,
          `CWD: ${ctx.host.cwd}`,
          `Messages: ${ctx.transcriptLength}`,
          `Leaf: ${ctx.host.getLeafId() ?? "(none)"}`,
          `Model: ${ctx.model.provider}/${ctx.model.id}`,
        ].join("\n"),
      );
      ctx.render();
    });
    return;
  }
  if (cmd === "/resume") {
    const id = parts[1];
    if (id) {
      void ctx.host.switchSession(id).then((resolved) => {
        if (!resolved) {
          ctx.msg("system", `Session ${id} not found`);
          ctx.render();
          return;
        }
        void ctx.doResume();
      });
    } else {
      void ctx.doResumeSelector();
    }
    return;
  }
  ctx.msg("system", `Unknown: ${cmd}`);
  ctx.render();
}
