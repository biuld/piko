import {
  AuthStorage,
  antigravityOAuthProvider,
  createAntigravityModels,
  installDebugTraceFromEnv,
  listAvailableModels,
  ModelRegistry,
  registerProvider,
  SettingsManager,
} from "piko-host-runtime";
import { launchOpenTui } from "piko-host-tui";
import { streamNoagy } from "piko-orchestrator";

function printHelp(): void {
  console.log(`piko — stateless engine CLI

Usage:
  piko                         Start interactive TUI mode
  piko -c / --continue         Continue most recent session
  piko --session <id>          Resume a specific session by id/path
  piko -m <model>              Specify model (e.g. "claude-sonnet-4-5-20250929")
  piko --provider <name>       Specify provider (e.g. "anthropic")
  piko --thinking <level>      Thinking level: off|minimal|low|medium|high|xhigh
  piko --api-key <key>         API key (provider detected from model)
  piko --system-prompt <text>  Custom system prompt
  piko --append-system-prompt <text>  Append to default system prompt
  piko --session-dir <path>    Custom session storage directory
  piko --name <name>           Set session name
  piko --no-context-files      Skip loading AGENTS.md / CLAUDE.md
  piko --no-tools              Disable tool calling
  piko --prompt-template <n>   Invoke a prompt template on startup
  piko --skill <name>          Invoke a skill on startup
  piko --list-models           List available models
  piko -h, --help              Show this help
`);
}

async function main(): Promise<void> {
  const debugTracePath = installDebugTraceFromEnv();
  const args = process.argv.slice(2);

  let modelId: string | undefined;
  let providerName: string | undefined;
  let continueSession = false;
  let sessionSpecifier: string | undefined;
  let thinkingLevel: string | undefined;
  let apiKey: string | undefined;
  let systemPrompt: string | undefined;
  let appendSystemPrompt: string | undefined;
  let sessionDir: string | undefined;
  let sessionName: string | undefined;
  let noContextFiles = false;
  let noTools = false;
  let promptTemplate: string | undefined;
  let skillName: string | undefined;

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
      case "--thinking":
        thinkingLevel = args[++i];
        break;
      case "--api-key":
        apiKey = args[++i];
        break;
      case "--system-prompt":
        systemPrompt = args[++i];
        break;
      case "--append-system-prompt":
        appendSystemPrompt = args[++i];
        break;
      case "--session-dir":
        sessionDir = args[++i];
        break;
      case "--name":
        sessionName = args[++i];
        break;
      case "--no-context-files":
        noContextFiles = true;
        break;
      case "--no-tools":
        noTools = true;
        break;
      case "--prompt-template":
        promptTemplate = args[++i];
        break;
      case "--skill":
        skillName = args[++i];
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

  const cwd = process.cwd();

  // ---- Wire up SettingsManager, AuthStorage, ModelRegistry ----

  // 1. Auth storage (file-backed, ~/.piko/auth.json)
  const authStorage = await AuthStorage.create();

  // 2. Settings manager (layered: defaults → global → project → CLI overrides)
  const settingsManager = await SettingsManager.create(cwd);

  // Apply CLI overrides to settings
  const overrides: Record<string, unknown> = {};
  if (thinkingLevel) overrides.defaultThinkingLevel = thinkingLevel;
  if (sessionDir) overrides.sessionDir = sessionDir;
  if (Object.keys(overrides).length > 0) {
    settingsManager.applyOverrides(overrides as any);
  }

  // 3. Model registry (integrates auth storage + scoped models from settings)
  const enabledModels = settingsManager.getEnabledModels();
  const modelRegistry = new ModelRegistry(authStorage, enabledModels ?? []);

  // Register built-in providers (OAuth + models + stream)
  registerProvider(modelRegistry, {
    id: "antigravity",
    oauth: antigravityOAuthProvider,
    models: createAntigravityModels(),
    stream: { api: "antigravity-api", handler: streamNoagy as any },
  });

  // Apply runtime API key override
  if (apiKey && providerName) {
    authStorage.setRuntimeApiKey(providerName, apiKey);
  }

  // 4. Resolve model through registry (uses settings defaults if no CLI flag)
  const defaultModel = settingsManager.getDefaultModel();
  const defaultProvider = settingsManager.getDefaultProvider();

  const resolved = modelRegistry.resolve(modelId ?? defaultModel, providerName ?? defaultProvider);

  if (!resolved) {
    console.error("No model found. Ensure API keys are set and try --list-models.");
    console.error("Use /login <provider> in TUI or set env vars (ANTHROPIC_API_KEY, etc.).");
    console.error("Run 'piko --help' for more options.");
    process.exit(1);
  }

  const { model, providerConfig } = resolved;

  // Launch with OpenTUI + SolidJS renderer
  await launchOpenTui(model, providerConfig, {
    session: sessionSpecifier ?? (continueSession ? "" : undefined),
    settingsManager,
    modelRegistry,
    authStorage,
    sessionName,
    noContextFiles,
    noTools,
    systemPrompt,
    appendSystemPrompt,
    promptTemplate,
    skillName,
    debugTracePath,
  });
}

main().catch((err) => {
  console.error("Error:", err instanceof Error ? err.message : String(err));
  process.exit(1);
});
