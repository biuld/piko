/**
 * Slash command handler — dispatches commands to their implementations.
 *
 * Aligned with pi's interactive-mode slash command handling.
 * pi reference: packages/coding-agent/src/modes/interactive/interactive-mode.ts
 */
import { COMMANDS } from "./definitions.js";
import type { CommandContext } from "./types.js";

function showError(ctx: CommandContext, error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  ctx.msg("system", message);
  ctx.render();
}

const ALL_COMMANDS = COMMANDS.map((c) => c.value);

export async function handleSlashCommand(trimmed: string, ctx: CommandContext): Promise<void> {
  const parts = trimmed.split(/\s+/);
  const cmd = parts[0].toLowerCase();

  // ---- pi-aligned commands ----

  if (cmd === "/quit") {
    process.exit(0);
  }

  if (cmd === "/settings") {
    void ctx.doSettingsSelector();
    return;
  }

  if (cmd === "/model" || cmd.startsWith("/model ")) {
    const searchTerm = cmd.startsWith("/model ") ? trimmed.slice(7).trim() : undefined;
    // pi: /model opens selector with optional search term
    void ctx.doModelSelector();
    return;
  }

  if (cmd === "/scoped-models") {
    void ctx.doModelScopeSelector();
    return;
  }

  if (cmd === "/export" || cmd.startsWith("/export ")) {
    const outputPath = parts[1];
    void ctx.host.loadMessages().then(async (msgs: any[]) => {
      try {
        if (outputPath?.endsWith(".jsonl")) {
          // JSONL export
          const { writeFileSync } = await import("node:fs");
          const lines = msgs.map((m) => JSON.stringify(m)).join("\n");
          writeFileSync(outputPath, lines, "utf-8");
          ctx.msg("system", `Session exported as JSONL to ${outputPath} (${msgs.length} messages)`);
        } else {
          // HTML export (default)
          const { exportToHtml } = await import("piko-host-runtime");
          const html = exportToHtml({
            messages: msgs,
            sessionName: await ctx.host.getSessionName(),
          });
          if (outputPath) {
            const { writeFileSync } = await import("node:fs");
            writeFileSync(outputPath, html, "utf-8");
            ctx.msg("system", `Session exported to ${outputPath}`);
          } else {
            process.stdout.write(html);
            ctx.msg("system", `Session exported (${msgs.length} messages, ${html.length} bytes)`);
          }
        }
      } catch (e: unknown) {
        ctx.msg("system", `Export failed: ${e instanceof Error ? e.message : String(e)}`);
      }
      ctx.render();
    });
    return;
  }

  if (cmd === "/import" || cmd.startsWith("/import ")) {
    const inputPath = trimmed.slice("/import".length).trim();
    if (!inputPath) {
      ctx.msg("system", "Usage: /import <path>");
      ctx.render();
      return;
    }
    void ctx.host
      .importSession(inputPath)
      .then(() => ctx.doResume())
      .catch((e) => showError(ctx, e));
    return;
  }

  if (cmd === "/copy") {
    void ctx.host.loadMessages().then(async (msgs: any[]) => {
      try {
        let lastText = "";
        for (let i = msgs.length - 1; i >= 0; i--) {
          const m = msgs[i];
          if (m.role === "assistant") {
            if (typeof m.content === "string") {
              lastText = m.content;
            } else if (Array.isArray(m.content)) {
              lastText = m.content
                .filter((c: any) => c.type === "text")
                .map((c: any) => c.text)
                .join("\n");
            }
            break;
          }
        }
        if (!lastText) {
          ctx.msg("system", "No agent messages to copy yet.");
        } else {
          const { execSync } = await import("node:child_process");
          try {
            if (process.platform === "darwin") {
              execSync("pbcopy", { input: lastText });
            } else if (process.platform === "linux") {
              execSync("xclip -selection clipboard", { input: lastText });
            } else if (process.platform === "win32") {
              execSync("clip", { input: lastText });
            }
            ctx.msg("system", "Copied last agent message to clipboard");
          } catch {
            ctx.msg(
              "system",
              `Clipboard copy failed. Text (${lastText.length} chars):\n${lastText.slice(0, 200)}...`,
            );
          }
        }
      } catch (e: unknown) {
        ctx.msg("system", `Copy failed: ${e instanceof Error ? e.message : String(e)}`);
      }
      ctx.render();
    });
    return;
  }

  if (cmd === "/name" || cmd.startsWith("/name ")) {
    const title = trimmed.slice("/name".length).trim();
    if (!title) {
      // pi: show current name when no arg
      void ctx.host.getSessionName().then((n) => {
        if (n) ctx.msg("system", `Session name: ${n}`);
        else ctx.msg("system", "Usage: /name <name>");
        ctx.render();
      });
      return;
    }
    void ctx.host
      .setSessionName(title)
      .then(() => {
        ctx.setSessionName(title);
        ctx.refreshHeader();
        ctx.refreshFooter();
        ctx.msg("system", `Session name set: ${title}`);
        ctx.render();
      })
      .catch((e) => showError(ctx, e));
    return;
  }

  if (cmd === "/session") {
    void ctx.host.getSessionName().then((currentSessionName) => {
      void ctx.host.loadMessages().then((msgs: any[]) => {
        const userMsgs = msgs.filter((m: any) => m.role === "user").length;
        const assistantMsgs = msgs.filter((m: any) => m.role === "assistant").length;

        // Count tool calls from assistant messages
        let toolCalls = 0;
        let toolResults = 0;
        for (const m of msgs) {
          if (m.role === "assistant" && Array.isArray(m.content)) {
            for (const b of m.content) {
              if (b.type === "toolCall") toolCalls++;
            }
          }
          if (m.role === "toolResult") toolResults++;
        }

        // Estimate tokens
        let totalInput = 0;
        let totalOutput = 0;
        for (const m of msgs) {
          if ((m as any).usage) {
            totalInput += (m as any).usage.input || 0;
            totalOutput += (m as any).usage.output || 0;
          }
        }

        ctx.msg(
          "system",
          [
            `Session Info`,
            ``,
            `Name: ${currentSessionName ?? "(none)"}`,
            `File: ${ctx.host.sessionFile ?? "In-memory"}`,
            `ID: ${ctx.host.sessionId}`,
            ``,
            `Messages`,
            `User: ${userMsgs}`,
            `Assistant: ${assistantMsgs}`,
            `Tool Calls: ${toolCalls}`,
            `Tool Results: ${toolResults}`,
            `Total: ${msgs.length}`,
            ``,
            `Tokens`,
            `Input: ${totalInput.toLocaleString()}`,
            `Output: ${totalOutput.toLocaleString()}`,
            `Total: ${(totalInput + totalOutput).toLocaleString()}`,
          ].join("\n"),
        );
        ctx.render();
      });
    });
    return;
  }

  if (cmd === "/hotkeys") {
    const { getKeybindings } = await import("@earendil-works/pi-tui");
    const kb = getKeybindings();
    const groups: Record<string, string[]> = {
      Editing: [
        "tui.editor.cursorUp",
        "tui.editor.cursorDown",
        "tui.editor.cursorLeft",
        "tui.editor.cursorRight",
        "tui.editor.cursorWordLeft",
        "tui.editor.cursorWordRight",
        "tui.editor.cursorLineStart",
        "tui.editor.cursorLineEnd",
        "tui.editor.pageUp",
        "tui.editor.pageDown",
        "tui.editor.deleteCharBackward",
        "tui.editor.deleteWordBackward",
        "tui.editor.deleteWordForward",
        "tui.editor.deleteToLineStart",
        "tui.editor.deleteToLineEnd",
        "tui.editor.yank",
        "tui.editor.undo",
      ],
      Input: ["tui.input.submit", "tui.input.newLine"],
      Selection: ["tui.select.up", "tui.select.down", "tui.select.confirm", "tui.select.cancel"],
    };
    const lines: string[] = [];
    for (const [group, actions] of Object.entries(groups)) {
      lines.push(`${group}:`);
      for (const action of actions) {
        try {
          const def = kb.getDefinition(action as any);
          const keys = kb.getKeys(action as any);
          if (keys.length > 0) {
            lines.push(`  ${keys.join(" / ")} — ${def.description ?? action}`);
          }
        } catch {
          /* skip */
        }
      }
      lines.push("");
    }
    lines.push("Application:");
    lines.push("  Ctrl+D — submit / send message");
    lines.push("  Ctrl+C — exit / abort");
    lines.push("  Ctrl+P — previous model");
    lines.push("  Ctrl+N — next model");
    lines.push("  Ctrl+T — toggle theme");
    ctx.msg("system", `Keyboard Shortcuts:\n${lines.join("\n")}`);
    ctx.render();
    return;
  }

  if (cmd === "/fork") {
    void ctx.doForkSelector().catch((e) => showError(ctx, e));
    return;
  }

  if (cmd === "/clone") {
    void ctx.doClone().catch((e) => showError(ctx, e));
    return;
  }

  if (cmd === "/tree") {
    void ctx.doTreeSelector();
    return;
  }

  if (cmd === "/login") {
    const provider = parts[1] || "anthropic";
    void ctx.doLoginSelector(provider);
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

  if (cmd === "/new") {
    void ctx.doNewSession();
    return;
  }

  if (cmd === "/compact" || cmd.startsWith("/compact ")) {
    const customInstructions = cmd.startsWith("/compact ") ? trimmed.slice(9).trim() : undefined;
    ctx.msg("system", "Compacting session...");
    ctx.render();
    void ctx.host
      .compact(customInstructions)
      .then((result: any) => {
        if (result?.compacted) {
          ctx.msg(
            "system",
            `Session compacted. Tokens: ${result.tokensBefore?.toLocaleString()} → ~${result.tokensKept?.toLocaleString()}`,
          );
        } else {
          ctx.msg("system", `Compaction skipped: ${result?.skippedReason ?? "nothing to compact"}`);
        }
        ctx.render();
      })
      .catch((e: unknown) => {
        ctx.msg("system", `Compaction failed: ${e instanceof Error ? e.message : String(e)}`);
        ctx.render();
      });
    return;
  }

  if (cmd === "/resume") {
    void ctx.doResumeSelector();
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

  // ---- Dynamic commands (templates, skills, unknown) ----

  // Check if it's a dynamic prompt template (/template-name)
  const templates: any[] = (ctx.host as any).promptTemplates ?? [];
  const templateMatch = templates.find(
    (t: any) => `/${t.name}` === cmd || trimmed.startsWith(`/${t.name} `),
  );
  if (templateMatch) {
    if (ctx.submitStream && (ctx.host as any).streamPromptTemplate) {
      const rest = trimmed.slice(templateMatch.name.length + 1).trim();
      const { parseCommandArgs } = await import("piko-host-runtime");
      const parsedArgs = parseCommandArgs(rest);
      ctx.submitStream(
        (signal) => (ctx.host as any).streamPromptTemplate(templateMatch.name, parsedArgs, signal),
        `Run template: ${trimmed}`,
        "template",
      );
      return;
    }
  }

  // Check if it's a dynamic skill (/skill-name)
  const skills: any[] = (ctx.host as any).skills ?? [];
  const skillMatch = skills.find(
    (s: any) => `/${s.name}` === cmd || trimmed.startsWith(`/${s.name} `),
  );
  if (skillMatch) {
    const additionalInstructions = trimmed.slice(skillMatch.name.length + 1).trim() || undefined;
    if (ctx.submitStream && (ctx.host as any).streamSkill) {
      ctx.submitStream(
        (signal) => (ctx.host as any).streamSkill(skillMatch.name, additionalInstructions, signal),
        `Invoke skill: ${skillMatch.name}`,
        "skill",
      );
      return;
    }
  }

  // Show help: list all builtin + dynamic commands
  const dynamicCommands: string[] = [];
  for (const t of templates) {
    dynamicCommands.push(`  /${t.name} — ${t.description ?? "prompt template"}`);
  }
  for (const s of skills) {
    dynamicCommands.push(`  /${s.name} — ${s.description ?? "skill"}`);
  }

  const builtinHelp = COMMANDS.map((c) => `  ${c.value} — ${c.description}`).join("\n");
  const helpText =
    dynamicCommands.length > 0 ? `${builtinHelp}\n\n${dynamicCommands.join("\n")}` : builtinHelp;

  ctx.msg("system", `Unknown command: ${cmd}\n\nAvailable commands:\n${helpText}`);
  ctx.render();
}
