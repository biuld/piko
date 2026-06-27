import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { launchOpenTui, TuiPreferences } from "piko-host-tui";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

function findRepoRoot(start: string): string | undefined {
  let dir = start;
  for (;;) {
    if (existsSync(join(dir, "Cargo.toml")) && existsSync(join(dir, "packages/hostd/Cargo.toml"))) {
      return dir;
    }
    const parent = dirname(dir);
    if (parent === dir) return undefined;
    dir = parent;
  }
}

function defaultHostdCommand(): string[] {
  if (process.env.PIKO_HOSTD_PATH) return [process.env.PIKO_HOSTD_PATH];

  const execPath = process.execPath;
  if (execPath) {
    const execDir = dirname(execPath);
    const localHostd = join(execDir, "hostd");
    if (existsSync(localHostd)) {
      return [localHostd];
    }
  }

  // Fallback: cargo build output
  const repoRoot = findRepoRoot(__dirname) ?? resolve(__dirname, "../../..");
  const workspaceHostd = join(repoRoot, "target/debug/hostd");
  if (existsSync(workspaceHostd)) {
    return [workspaceHostd];
  }
  const packageHostd = join(repoRoot, "packages/hostd/target/debug/hostd");
  if (existsSync(packageHostd)) {
    return [packageHostd];
  }

  return ["hostd"];
}

function printHelp() {
  console.log(`piko — AI coding agent harness

Usage: piko [options]

Options:
  -m, --model <id>        Model ID to use
  -p, --provider <name>   Provider name
  -k, --api-key <key>     API key (passed to hostd)
  --thinking-level <l>    Thinking level (off|low|medium|high)
  --session-dir <dir>     Session storage directory
  --session <id>          Open specific session
  --continue              Continue last session
  --name <name>           Session name
  --system-prompt <text>  Custom system prompt
  --no-context-files      Skip context file loading
  --no-tools              Disable tools
  --prompt-template <id>  Run prompt template
  --skill <name>          Run skill
  --help                  Show this help
`);
}

// ---- Main ----

async function main() {
  const args = process.argv.slice(2);

  let modelId: string | undefined;
  let providerName: string | undefined;
  let apiKey: string | undefined;
  let thinkingLevel: string | undefined;
  let sessionDir: string | undefined;
  let sessionSpecifier: string | undefined;
  let continueSession = false;
  let sessionName: string | undefined;
  let noContextFiles = false;
  let noTools = false;
  let systemPrompt: string | undefined;
  let promptTemplate: string | undefined;
  let skillName: string | undefined;

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    switch (arg) {
      case "-m":
      case "--model":
        modelId = args[++i];
        break;
      case "-p":
      case "--provider":
        providerName = args[++i];
        break;
      case "-k":
      case "--api-key":
        apiKey = args[++i];
        break;
      case "--thinking-level":
        thinkingLevel = args[++i];
        break;
      case "--session-dir":
        sessionDir = args[++i];
        break;
      case "--session":
        sessionSpecifier = args[++i];
        break;
      case "--continue":
        continueSession = true;
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
      case "-h":
      case "--help":
        printHelp();
        return;
    }
  }

  const cwd = process.cwd();

  // Read settings (display preferences only — hostd handles auth/models)
  const preferences = await TuiPreferences.create(cwd);

  const overrides: Record<string, unknown> = {};
  if (thinkingLevel) overrides.defaultThinkingLevel = thinkingLevel;
  if (sessionDir) overrides.sessionDir = sessionDir;
  if (Object.keys(overrides).length > 0) {
    preferences.applyOverrides(overrides as any);
  }

  // Resolve model (use CLI flags or settings defaults)
  const defaultModel = preferences.getDefaultModel();
  const defaultProvider = preferences.getDefaultProvider();
  const model = modelId ?? defaultModel ?? "claude-sonnet-4-20250514";
  const provider = providerName ?? defaultProvider ?? "anthropic";

  // Build model info for TUI (hostd handles auth and actual model setup)
  const modelInfo = { id: model, name: model, provider };
  const providerConfig = { provider, apiKey: apiKey ?? "" };

  // Resolve hostd command
  const hostdEnabled =
    process.env.PIKO_HOST_BACKEND !== "facade" &&
    process.env.PIKO_HOSTD !== "0" &&
    process.env.PIKO_NO_HOSTD !== "1";

  let hostdCommand = process.env.PIKO_HOSTD_COMMAND;
  let hostdArgs = process.env.PIKO_HOSTD_ARGS
    ? process.env.PIKO_HOSTD_ARGS.split(" ").filter(Boolean)
    : undefined;

  if (hostdEnabled && !hostdCommand) {
    const resolvedCmd = defaultHostdCommand();
    hostdCommand = resolvedCmd[0];
    if (resolvedCmd.length > 1) {
      hostdArgs = [...resolvedCmd.slice(1), ...(hostdArgs || [])];
    }
  }

  // Launch TUI (hostd handles all runtime: auth, models, session, turn execution)
  await launchOpenTui(
    modelInfo as any,
    providerConfig as any,
    {
      session: sessionSpecifier ?? (continueSession ? "" : undefined),
      preferences,
      sessionName,
      noContextFiles,
      noTools,
      systemPrompt,
      promptTemplate,
      skillName,
      hostd: hostdEnabled
        ? { enabled: true, command: hostdCommand, args: hostdArgs }
        : { enabled: false },
      debugTracePath: process.env.PIKO_DEBUG_TRACE ? join(cwd, ".piko", "debug-traces") : undefined,
      thinkingLevel,
      apiKey,
    } as any,
  );
}

main().catch((err) => {
  console.error("Fatal:", err instanceof Error ? err.message : String(err));
  process.exit(1);
});
