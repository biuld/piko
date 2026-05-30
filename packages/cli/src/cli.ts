import { findModel, listAvailableModels } from "piko-host-runtime";
import { runTui } from "piko-host-tui";

function printHelp(): void {
  console.log(`piko — stateless engine CLI

Usage:
  piko                  Start interactive TUI mode
  piko -c               Continue most recent session
  piko --session <id>   Resume a specific session by id/path
  piko -m <model>       Specify model
  piko --list-models    List available models
  piko -h, --help       Show this help
`);
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  let modelId: string | undefined;
  let providerName: string | undefined;
  let continueSession = false;
  let sessionSpecifier: string | undefined;

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    switch (arg) {
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

  await runTui(model, providerConfig, {
    session: sessionSpecifier ?? (continueSession ? "" : undefined),
  });
}

main().catch((err) => {
  console.error("Error:", err instanceof Error ? err.message : String(err));
  process.exit(1);
});
