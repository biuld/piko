import {
  createForkSessionPanelSession,
  createImportSessionPanelSession,
  createRenameSessionPanelSession,
  createResumePanelSession,
  createTreePanelSession,
} from "../../panels/panel-factories.js";
import type { CommandDefinition } from "../types.js";
import type { BuiltinCommandContext } from "./types.js";

export function createSessionCommands(ctx: BuiltinCommandContext): CommandDefinition[] {
  return [
    {
      id: "piko.session.resume",
      slash: {
        name: "/resume",
        aliases: ["/r"],
        description: "Resume a previous session",
        argumentHint: "[query]",
      },
      keybindings: ["app.session.resume"],
      requiresIdle: true,
      run(_ctx, args) {
        const panel = createResumePanelSession();
        if (args) panel.state.filterText = args;
        ctx().openPanel({
          placement: "full",
          panel,
        });
      },
    },
    {
      id: "piko.session.new",
      slash: {
        name: "/new",
        description: "Start a new session",
      },
      requiresIdle: true,
      async run(_ctx) {
        const host = ctx().host;
        try {
          await host.newSession();
          await dispatchCurrentSession(ctx);
          ctx().notify("New session started", "success");
        } catch (e: any) {
          ctx().notify(`Failed to start new session: ${e.message}`, "error");
        }
      },
    },
    {
      id: "piko.session.compact",
      slash: {
        name: "/compact",
        description: "Compact the current session",
      },
      requiresIdle: true,
      async run(_ctx) {
        const host = ctx().host;
        try {
          const result = await host.compact();
          if (result.compacted) {
            ctx().notify(
              `Compacted: ${result.tokensBefore ?? "?"} → ${result.tokensKept ?? "?"} tokens`,
              "success",
            );
          } else {
            ctx().notify(result.skippedReason ?? "Compaction not needed", "info");
          }
        } catch (e: any) {
          ctx().notify(`Compaction failed: ${e.message}`, "error");
        }
      },
    },
    {
      id: "piko.session.fork",
      slash: {
        name: "/fork",
        description: "Fork session at a message",
      },
      requiresIdle: true,
      run(_ctx) {
        ctx().openPanel({
          placement: "full",
          panel: createForkSessionPanelSession(),
        });
      },
    },
    {
      id: "piko.session.clone",
      slash: {
        name: "/clone",
        description: "Clone current session",
      },
      requiresIdle: true,
      async run(_ctx) {
        const host = ctx().host;
        try {
          await host.cloneSession();
          await dispatchCurrentSession(ctx);
          ctx().notify("Session cloned", "success");
        } catch (e: any) {
          ctx().notify(`Clone failed: ${e.message}`, "error");
        }
      },
    },
    {
      id: "piko.session.tree",
      slash: {
        name: "/tree",
        description: "Show session tree",
      },
      keybindings: ["app.session.tree"],
      requiresIdle: true,
      run(_ctx) {
        ctx().openPanel({
          placement: "full",
          panel: createTreePanelSession(),
        });
      },
    },
    {
      id: "piko.session.rename",
      slash: {
        name: "/name",
        description: "Rename current session",
        argumentHint: "[name]",
      },
      requiresIdle: true,
      async run(_ctx, args) {
        if (!args) {
          ctx().openPanel({
            placement: "partial",
            inputPolicy: "capture",
            panel: createRenameSessionPanelSession(),
          });
          return;
        }
        const host = ctx().host;
        try {
          await host.setSessionName(args);
          ctx().notify(`Session renamed to "${args}"`, "success");
        } catch (e: any) {
          ctx().notify(`Rename failed: ${e.message}`, "error");
        }
      },
    },
    {
      id: "piko.session.export",
      slash: {
        name: "/export",
        description: "Show session file path",
      },
      requiresIdle: true,
      run(_ctx) {
        const file = ctx().host.sessionFile;
        if (file) {
          ctx().notify(`Session file: ${file}`, "info");
        } else {
          ctx().notify("Session not yet saved to file", "warning");
        }
      },
    },
    {
      id: "piko.session.import",
      slash: {
        name: "/import",
        description: "Import session from JSONL file",
        argumentHint: "<path>",
      },
      requiresIdle: true,
      async run(_ctx, args) {
        if (!args) {
          ctx().openPanel({
            placement: "partial",
            inputPolicy: "capture",
            panel: createImportSessionPanelSession(),
          });
          return;
        }
        const host = ctx().host;
        try {
          await host.importSession(args);
          await dispatchCurrentSession(ctx);
          ctx().notify(`Imported session from ${args}`, "success");
        } catch (e: any) {
          ctx().notify(`Import failed: ${e.message}`, "error");
        }
      },
    },
    {
      id: "piko.session.share",
      slash: {
        name: "/share",
        description: "Share session",
      },
      requiresIdle: true,
      run(_ctx) {
        ctx().notify("Share not yet implemented", "warning");
      },
    },
    {
      id: "piko.session.copy",
      slash: {
        name: "/copy",
        description: "Copy session content",
      },
      run(_ctx) {
        ctx().notify("Copy not yet implemented", "warning");
      },
    },
  ];
}

async function dispatchCurrentSession(ctx: BuiltinCommandContext): Promise<void> {
  const host = ctx().host;
  const sessionId = host.sessionId;
  const sessionName = await host.getSessionName();
  const entries = await host.loadBranchEntries();
  const { entriesToTranscript } = await import("../../timeline/entries-to-transcript.js");
  const transcript = entriesToTranscript(entries);

  ctx().dispatch({
    type: "session_resumed",
    sessionId,
    sessionName: sessionName ?? undefined,
    transcript,
  });
}
