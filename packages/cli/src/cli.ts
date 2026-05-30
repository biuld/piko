import * as readline from "node:readline";
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

function printHelp(): void {
  console.log(`piko — stateless engine CLI

Usage:
  piko [options]
  piko -p "your prompt"

Options:
  -p, --prompt <text>    Run a single prompt (non-interactive)
  -m, --model <id>       Model ID to use
  --provider <name>      Provider name
  --list-models          List available models
  -h, --help             Show this help

Interactive mode commands:
  /help                  Show this help
  /model <id>            Switch model
  /exit, /quit           Exit
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

async function runInteractive(
  model: EngineModel,
  providerConfig: EngineProviderConfig,
): Promise<void> {
  console.log("piko interactive mode");
  console.log(`Model: ${model.provider}/${model.id}`);
  console.log('Type /help for commands, /exit to quit.\n');

  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: "> ",
  });

  let currentModel = model;
  let currentProviderConfig = providerConfig;
  let systemPrompt = "You are a helpful assistant. Be concise.";

  const askQuestion = (): Promise<string> => {
    return new Promise((resolve) => {
      rl.question("> ", (answer) => {
        resolve(answer.trim());
      });
    });
  };

  while (true) {
    const input = await askQuestion();

    if (input === "") continue;

    // Handle slash commands
    if (input.startsWith("/")) {
      const parts = input.split(/\s+/);
      const cmd = parts[0].toLowerCase();

      switch (cmd) {
        case "/exit":
        case "/quit":
          console.log("Goodbye.");
          rl.close();
          return;
        case "/help":
          console.log("\nCommands:");
          console.log("  /help          Show this help");
          console.log("  /model <id>    Switch model");
          console.log("  /provider <name> Switch provider");
          console.log("  /system <text>  Set system prompt");
          console.log("  /exit, /quit   Exit\n");
          continue;
        case "/model": {
          const modelId = parts[1];
          if (!modelId) {
            console.log("Usage: /model <model-id>");
            continue;
          }
          const found = findModel(modelId);
          if (found) {
            currentModel = found.model;
            currentProviderConfig = found.providerConfig;
            console.log(`Switched to ${found.model.provider}/${found.model.id}`);
          } else {
            console.log(`Model "${modelId}" not found.`);
          }
          continue;
        }
        case "/provider": {
          const providerName = parts[1];
          if (!providerName) {
            console.log("Usage: /provider <provider-name>");
            continue;
          }
          const found = findModel(undefined, providerName);
          if (found) {
            currentModel = found.model;
            currentProviderConfig = found.providerConfig;
            console.log(`Switched to ${found.model.provider}/${found.model.id}`);
          } else {
            console.log(`Provider "${providerName}" not found or has no models.`);
          }
          continue;
        }
        case "/system": {
          const text = parts.slice(1).join(" ");
          if (text) {
            systemPrompt = text;
            console.log(`System prompt set.`);
          } else {
            console.log(`Current system prompt: ${systemPrompt}`);
          }
          continue;
        }
        default:
          console.log(`Unknown command: ${cmd}. Type /help for commands.`);
          continue;
      }
    }

    // Run the prompt
    const engine = createNativeEngine({ llmCaller: createPiLlmCaller() });
    const config = createHostConfig(
      currentModel,
      currentProviderConfig,
      createDefaultSettings({ allowToolCalls: false }),
    );

    const host = new PikoHost({
      engine,
      config,
      systemPrompt,
      approvalHandler: createAutoAcceptHandler(),
    });

    process.stdout.write("\n");
    const result = await host.run(input);

    for (const msg of result.messages) {
      if (msg.role === "assistant") {
        for (const block of msg.content) {
          if (block.type === "text") {
            console.log(block.text);
          }
        }
      }
    }
    console.log(""); // Blank line after response
  }
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  // Parse args to find model/provider early
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

  // Resolve model
  const found = findModel(modelId, providerName);
  if (!found) {
    console.error("No model found. Ensure API keys are set and try --list-models.");
    console.error("Common env vars: ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.");
    process.exit(1);
  }
  // TypeScript can't narrow after process.exit
  const { model, providerConfig } = found;

  if (prompt) {
    await runPrompt(prompt, model, providerConfig);
  } else {
    await runInteractive(model, providerConfig);
  }
}

main().catch((err) => {
  console.error("Error:", err instanceof Error ? err.message : String(err));
  process.exit(1);
});
