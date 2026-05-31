/**
 * Slash command handler — dispatches commands to their implementations.
 */
import type { PromptTemplate } from "piko-host-runtime";
import { expandPromptTemplate, formatSkillPrompt, parseCommandArgs } from "piko-host-runtime";
import { COMMANDS } from "./definitions.js";
import type { CommandContext } from "./types.js";

function showError(ctx: CommandContext, error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  ctx.msg("system", message);
  ctx.render();
}

export async function handleSlashCommand(trimmed: string, ctx: CommandContext): Promise<void> {
  const parts = trimmed.split(/\s+/);
  const cmd = parts[0].toLowerCase();

  if (cmd === "/exit") {
    process.exit(0);
  }
  if (cmd === "/compact") {
    ctx.msg("system", "Compacting session...");
    ctx.render();
    void ctx.host.compact().then(() => {
      ctx.msg("system", "Session compacted.");
      ctx.render();
    }).catch((e: unknown) => {
      ctx.msg("system", `Compaction failed: ${e instanceof Error ? e.message : String(e)}`);
      ctx.render();
    });
    return;
  }
  if (cmd === "/export") {
    const outputPath = parts[1];
    void ctx.host.loadMessages().then(async (msgs: any[]) => {
      try {
        const { exportToHtml } = await import("piko-host-runtime");
        const html = exportToHtml({ messages: msgs, sessionName: await ctx.host.getSessionName() });
        if (outputPath) {
          const { writeFileSync } = await import("node:fs");
          writeFileSync(outputPath, html, "utf-8");
          ctx.msg("system", `Session exported to ${outputPath}`);
        } else {
          process.stdout.write(html);
          ctx.msg("system", `Session exported (${msgs.length} messages, ${html.length} bytes)`);
        }
      } catch (e: unknown) {
        ctx.msg("system", `Export failed: ${e instanceof Error ? e.message : String(e)}`);
      }
      ctx.render();
    });
    return;
  }
  if (cmd === "/reload") {
    ctx.msg("system", "Reloading settings, skills and templates...");
    ctx.render();
    try {
      if (ctx.reloadRuntime) await ctx.reloadRuntime();
      ctx.msg("system", "Reloaded. Use /resync to refresh the transcript.");
    } catch (e: unknown) {
      ctx.msg("system", `Reload failed: ${e instanceof Error ? e.message : String(e)}`);
    }
    ctx.render();
    return;
  }
  if (cmd === "/logout") {
    const provider = parts[1];
    if (!provider) {
      ctx.msg("system", "Usage: /logout <provider>");
      ctx.render();
      return;
    }
    try {
      const { AuthStorage } = await import("piko-host-runtime");
      const auth = AuthStorage.create();
      auth.remove(provider);
      ctx.msg("system", `Removed API key for ${provider}.`);
    } catch (e: unknown) {
      ctx.msg("system", `Logout failed: ${e instanceof Error ? e.message : String(e)}`);
    }
    ctx.render();
    return;
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
      void ctx.doThinkingSelector();
    }
    ctx.render();
    return;
  }
  if (cmd === "/theme") {
    const name = parts[1];
    if (name) {
      const ok = ctx.switchTheme(name);
      ctx.msg("system", ok ? `Theme switched to: ${name}` : `Unknown theme: ${name}. Use: dark, light`);
    } else {
      ctx.msg("system", `Current theme: ${ctx.currentTheme}. Available: dark, light`);
    }
    ctx.render();
    return;
  }
  if (cmd === "/login") {
    const provider = parts[1] || "anthropic";
    void ctx.doLoginSelector(provider);
    return;
  }
  if (cmd === "/settings") {
    void ctx.doSettingsSelector();
    return;
  }
  if (cmd === "/template") {
    const rest = trimmed.slice("/template".length).trim();
    if (!rest) {
      const templates: PromptTemplate[] = (ctx.host as any).promptTemplates ?? [];
      if (templates.length === 0) {
        ctx.msg("system", "No prompt templates available. Add .md files to .piko/prompts/");
      } else {
        const lines = templates.map((t) => `  /${t.name} — ${t.description}`);
        ctx.msg("system", `Available templates:\n${lines.join("\n")}`);
      }
      ctx.render();
      return;
    }
    const parsedArgs = parseCommandArgs(rest);
    const templateName = parsedArgs[0]!;
    const args = parsedArgs.slice(1);
    if (!(ctx.host as any).streamPromptTemplate) {
      const templates: PromptTemplate[] = (ctx.host as any).promptTemplates ?? [];
      const expanded = expandPromptTemplate(`/${rest}`, templates);
      if (expanded === `/${rest}`) {
        ctx.msg("system", `Unknown template: ${rest.split(/\s+/)[0]}`);
        ctx.render();
        return;
      }
      if (ctx.submitUserMessage) ctx.submitUserMessage(expanded);
      else if (ctx.setEditorText) ctx.setEditorText(expanded);
      return;
    }
    if (ctx.submitStream) {
      ctx.submitStream(
        (signal) => (ctx.host as any).streamPromptTemplate(templateName, args, signal),
        `Run template /${templateName} ${args.join(" ")}`,
      );
    } else if (ctx.submitUserMessage) {
      const templates: PromptTemplate[] = (ctx.host as any).promptTemplates ?? [];
      const expanded = expandPromptTemplate(`/${rest}`, templates);
      ctx.submitUserMessage(expanded);
    }
    return;
  }
  if (cmd === "/skill") {
    const name = parts[1];
    const additionalInstructions = parts.slice(2).join(" ") || undefined;
    if (!name) {
      const skills = (ctx.host as any).skills ?? [];
      if (skills.length === 0) {
        ctx.msg("system", "No skills available. Add .md files to .piko/skills/");
      } else {
        const lines = skills.map((s: any) => `  ${s.name} — ${s.description ?? "(no description)"}`);
        ctx.msg("system", `Available skills:\n${lines.join("\n")}`);
      }
      ctx.render();
      return;
    }
    if (ctx.submitStream && (ctx.host as any).streamSkill) {
      ctx.submitStream(
        (signal) => (ctx.host as any).streamSkill(name, additionalInstructions, signal),
        `Invoke skill: ${name}`,
      );
      return;
    }
    const skills = (ctx.host as any).skills ?? [];
    const skill = skills.find((s: any) => s.name === name);
    if (!skill) {
      ctx.msg("system", `Unknown skill: ${name}`);
      ctx.render();
      return;
    }
    const prompt = formatSkillPrompt(skill, additionalInstructions);
    if (ctx.submitUserMessage) {
      ctx.submitUserMessage(prompt);
    } else if (ctx.setEditorText) {
      ctx.setEditorText(prompt);
    } else {
      ctx.msg("system", `Skill: ${name}`);
    }
    return;
  }
  if (cmd === "/model") {
    const sub = parts[1];
    if (sub === "next") {
      void ctx.cycleModelForward();
    } else if (sub === "prev") {
      void ctx.cycleModelBackward();
    } else if (sub === "scope") {
      void ctx.doModelScopeSelector();
    } else {
      void ctx.doModelSelector();
    }
    return;
  }
  if (cmd === "/models") {
    const models = ctx.listModels();
    ctx.msg("system", models.flatMap((p) => p.models.map((m) => `${p.provider}/${m.id}`)).join("\n"));
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
    void ctx.host.importSession(inputPath).then(() => ctx.doResume()).catch((e) => showError(ctx, e));
    return;
  }
  if (cmd === "/name") {
    const title = trimmed.slice("/name".length).trim();
    void ctx.host.setSessionName(title || undefined).then(() => {
      ctx.setSessionName(title || undefined);
      ctx.refreshHeader();
      ctx.refreshFooter();
      ctx.msg("system", title ? `Session renamed to: ${title}` : "Session title cleared");
      ctx.render();
    }).catch((e) => showError(ctx, e));
    return;
  }
  if (cmd === "/tree") {
    const entryId = parts[1];
    if (!entryId) {
      void ctx.doTreeSelector();
      return;
    }
    void ctx.host.branchToEntry(entryId).then(async () => {
      await ctx.resync(`Switched branch to ${ctx.host.getLeafId()}`);
    }).catch((e) => showError(ctx, e));
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
      ctx.msg("system", [
        `Session ID: ${ctx.host.sessionId}`,
        `Session Name: ${currentSessionName ?? "(none)"}`,
        `Session File: ${ctx.host.sessionFile ?? "(new session)"}`,
        `Parent Session: ${ctx.host.getParentSessionPath() ?? "(none)"}`,
        `CWD: ${ctx.host.cwd}`,
        `Messages: ${ctx.transcriptLength}`,
        `Leaf: ${ctx.host.getLeafId() ?? "(none)"}`,
        `Model: ${ctx.model.provider}/${ctx.model.id}`,
      ].join("\n"));
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
