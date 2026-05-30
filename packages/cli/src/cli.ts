import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import {
  createDefaultSettings,
  createHostConfig,
  findModel,
  listAvailableModels,
  PikoHost,
} from "piko-host-runtime";
import { runTui } from "piko-host-tui";

function printHelp(): void {
  console.log(`piko — stateless engine CLI

Usage:
  piko                  Start interactive TUI mode
  piko -c               Continue most recent session
  piko --session <id>   Resume a specific session by id/path
  piko -p "prompt"      Run a single prompt (non-interactive)
  piko -m <model>       Specify model
  piko --list-models    List available models
  piko -h, --help       Show this help
`);
}

async function runPrompt(
  prompt: string,
  model: Model<string>,
  providerConfig: EngineProviderConfig,
  options?: {
    continueSession?: boolean;
    session?: string;
  },
): Promise<void> {
  const host = await PikoHost.create({
    config: createHostConfig(
      model,
      providerConfig,
      createDefaultSettings({ allowToolCalls: true, maxSteps: 10 }),
    ),
    session: {
      session: options?.session,
    },
  });

  // For non-interactive -c, try to continue recent; fallback is handled by the host factory.
  if (options?.continueSession && !host.sessionFile) {
    // Already attempted by PikoSessionRuntime.create — if no recent session,
    // we silently start a new one (host already did).
  }

  const result = await host.run(prompt);

  for (const msg of result.messages) {
    if (msg.role === "assistant") {
      for (const block of msg.content) {
        if (block.type === "text") {
          console.log(block.text);
        }
      }
    }
  }
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  let modelId: string | undefined;
  let providerName: string | undefined;
  let prompt: string | undefined;
  let continueSession = false;
  let sessionSpecifier: string | undefined;

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    switch (arg) {
      case "-p":
      case "--prompt":
        prompt = args[++i];
        break;
      case "-m":
      case "--model":
        modelId = args[++i];
        break;
      case "--provider":
        providerName = args[++i];
        break;
      case "-c":
      case "--continue":
        continueSession = true;
        break;
      case "--session":
        sessionSpecifier = args[++i];
        break;
      case "--list-models": {
        const allModels = listAvailableModels();
        for (const p of allModels) {
          console.log(`${p.provider}:`);
          for (const m of p.models) {
            console.log(`  ${m.id} — ${m.name}`);
          }
        }
        return;
      }
      case "-h":
      case "--help":
        printHelp();
        return;
    }
  }

  const found = findModel(modelId, providerName);
  if (!found) {
    console.error("No model found. Ensure API keys are set and try --list-models.");
    console.error("Common env vars: ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.");
    process.exit(1);
  }
  const { model, providerConfig } = found;

  if (prompt) {
    await runPrompt(prompt, model, providerConfig, {
      continueSession,
      session: sessionSpecifier,
    });
  } else {
    await runTui(model, providerConfig, {
      session: sessionSpecifier ?? (continueSession ? "" : undefined),
    });
  }
}

main().catch((err) => {
  console.error("Error:", err instanceof Error ? err.message : String(err));
  process.exit(1);
});
