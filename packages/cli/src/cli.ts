import { createNativeEngine } from "piko-engine-native";
import {
  PikoHost,
  createHostConfig,
  createDefaultSettings,
  createAutoAcceptHandler,
  listAvailableModels,
  findModel,
  createPiLlmCaller,
} from "piko-host-runtime";
import type { EngineModel, EngineProviderConfig } from "piko-engine-protocol";
import { runTui } from "./tui.js";

function printHelp(): void {
  console.log(`piko — stateless engine CLI

Usage:
  piko                  Start interactive TUI mode
  piko -p "prompt"      Run a single prompt (non-interactive)
  piko -m <model>       Specify model
  piko --list-models    List available models
  piko -h, --help       Show this help
`);
}

async function runPrompt(
  prompt: string,
  model: EngineModel,
  providerConfig: EngineProviderConfig,
): Promise<void> {
  const engine = createNativeEngine({ llmCaller: createPiLlmCaller() });
  const config = createHostConfig(
    model,
    providerConfig,
    createDefaultSettings({ allowToolCalls: false }),
  );

  const host = new PikoHost({
    engine,
    config,
    systemPrompt: "You are a helpful assistant. Be concise.",
  });

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
    await runPrompt(prompt, model, providerConfig);
  } else {
    await runTui(model, providerConfig);
  }
}

main().catch((err) => {
  console.error("Error:", err instanceof Error ? err.message : String(err));
  process.exit(1);
});
