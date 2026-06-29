import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

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

function defaultRustTuiCommand(): string | undefined {
  if (process.env.PIKO_TUI_PATH) return process.env.PIKO_TUI_PATH;

  const execPath = process.execPath;
  if (execPath) {
    const execDir = dirname(execPath);
    const localTui = join(execDir, "piko-tui");
    if (existsSync(localTui)) return localTui;
  }

  const repoRoot = findRepoRoot(__dirname) ?? resolve(__dirname, "../../..");
  const debugTui = join(repoRoot, "target/debug/piko-tui");
  if (existsSync(debugTui)) return debugTui;
  const releaseTui = join(repoRoot, "target/release/piko-tui");
  if (existsSync(releaseTui)) return releaseTui;

  return "piko-tui";
}

function configureTreeSitterWorkerPath() {
  if (process.env.OTUI_TREE_SITTER_WORKER_PATH) return;
  const workerPath = join(dirname(process.execPath), "parser.worker.js");
  if (existsSync(workerPath)) {
    process.env.OTUI_TREE_SITTER_WORKER_PATH = workerPath;
  }
}

function printHelp() {
  console.log(`piko — AI coding agent harness

Usage: piko [options]

Options:
  -m, --model <id>        Model ID to use
  -p, --provider <name>   Provider name
  -k, --api-key <key>     API key (passed to hostd)
  --thinking-level <l>    Thinking level (off|low|medium|high)
  --session <id>          Open specific session
  --continue              Continue last session
  --name <name>           Session name
  --system-prompt <text>  Custom system prompt
  --no-context-files      Skip context file loading
  --no-tools              Disable tools
  --prompt-template <id>  Run prompt template
  --skill <name>          Run skill
  --rust-tui              Use Rust ratatui TUI
  --legacy-tui            Use legacy TypeScript OpenTUI
  --help                  Show this help

Environment:
  PIKO_TUI=rust|legacy    Select TUI frontend
  PIKO_TUI_PATH=<path>    Rust TUI executable path
`);
}

function runRustTui(options: {
  tuiCommand: string;
  hostdCommand?: string;
  hostdArgs?: string[];
  modelId?: string;
  providerName?: string;
  apiKey?: string;
  thinkingLevel?: string;
  sessionSpecifier?: string;
  continueSession: boolean;
  sessionName?: string;
  noTools: boolean;
}): Promise<boolean> {
  return new Promise((resolve, reject) => {
    const args: string[] = [];
    if (options.hostdCommand) {
      args.push("--hostd", options.hostdCommand);
    }
    for (const arg of options.hostdArgs ?? []) {
      args.push("--hostd-arg", arg);
    }
    if (options.sessionSpecifier) {
      args.push("--session", options.sessionSpecifier);
    } else if (options.continueSession) {
      args.push("--continue");
    }
    if (options.modelId) {
      args.push("--model", options.modelId);
    }
    if (options.providerName) {
      args.push("--provider", options.providerName);
    }
    if (options.apiKey) {
      args.push("--api-key", options.apiKey);
    }
    if (options.thinkingLevel) {
      args.push("--thinking-level", options.thinkingLevel);
    }
    if (options.sessionName) {
      args.push("--name", options.sessionName);
    }
    if (options.noTools) {
      args.push("--no-tools");
    }

    const child = spawn(options.tuiCommand, args, {
      stdio: "inherit",
      env: process.env,
    });

    child.on("error", (err: NodeJS.ErrnoException) => {
      if (err.code === "ENOENT") {
        resolve(false);
      } else {
        reject(err);
      }
    });
    child.on("close", (code, signal) => {
      if (signal) {
        process.kill(process.pid, signal);
        return;
      }
      process.exitCode = code ?? 0;
      resolve(true);
    });
  });
}

async function launchLegacyTui(options: {
  cwd: string;
  modelId?: string;
  providerName?: string;
  apiKey?: string;
  thinkingLevel?: string;
  sessionSpecifier?: string;
  continueSession: boolean;
  sessionName?: string;
  noContextFiles: boolean;
  noTools: boolean;
  systemPrompt?: string;
  promptTemplate?: string;
  skillName?: string;
  hostdEnabled: boolean;
  hostdCommand?: string;
  hostdArgs?: string[];
}) {
  const importModule = new Function("specifier", "return import(specifier)") as (
    specifier: string,
  ) => Promise<any>;
  let legacyModule: any;
  try {
    legacyModule = await importModule("piko-host-tui");
  } catch (err) {
    throw new Error(
      `legacy TypeScript TUI is not available in this build; use the Rust TUI or install piko-host-tui separately (${err instanceof Error ? err.message : String(err)})`,
    );
  }
  const { launchOpenTui, TuiPreferences } = legacyModule;
  const preferences = await TuiPreferences.create(options.cwd);
  const model = options.modelId ?? "claude-sonnet-4-5-20250929";
  const provider = options.providerName ?? "anthropic";
  const modelInfo = { id: model, name: model, provider };
  const providerConfig = { provider, apiKey: options.apiKey ?? "" };

  await launchOpenTui(
    modelInfo as any,
    providerConfig as any,
    {
      session: options.sessionSpecifier ?? (options.continueSession ? "" : undefined),
      preferences,
      sessionName: options.sessionName,
      noContextFiles: options.noContextFiles,
      noTools: options.noTools,
      systemPrompt: options.systemPrompt,
      promptTemplate: options.promptTemplate,
      skillName: options.skillName,
      hostd: options.hostdEnabled
        ? { enabled: true, command: options.hostdCommand, args: options.hostdArgs }
        : { enabled: false },
      debugTracePath: process.env.PIKO_DEBUG_TRACE
        ? join(options.cwd, ".piko", "debug-traces")
        : undefined,
      thinkingLevel: options.thinkingLevel,
      apiKey: options.apiKey,
    } as any,
  );
}

// ---- Main ----

async function main() {
  configureTreeSitterWorkerPath();

  const args = process.argv.slice(2);

  let modelId: string | undefined;
  let providerName: string | undefined;
  let apiKey: string | undefined;
  let thinkingLevel: string | undefined;
  let sessionSpecifier: string | undefined;
  let continueSession = false;
  let sessionName: string | undefined;
  let noContextFiles = false;
  let noTools = false;
  let systemPrompt: string | undefined;
  let promptTemplate: string | undefined;
  let skillName: string | undefined;
  let tuiMode = process.env.PIKO_TUI === "legacy" ? "legacy" : "rust";

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
      case "--rust-tui":
        tuiMode = "rust";
        break;
      case "--legacy-tui":
        tuiMode = "legacy";
        break;
      case "-h":
      case "--help":
        printHelp();
        return;
    }
  }

  const cwd = process.cwd();

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

  if (tuiMode === "rust") {
    const launched = await runRustTui({
      tuiCommand: defaultRustTuiCommand() ?? "piko-tui",
      hostdCommand: hostdEnabled ? hostdCommand : undefined,
      hostdArgs,
      modelId,
      providerName,
      apiKey,
      thinkingLevel,
      sessionSpecifier,
      continueSession,
      sessionName,
      noTools,
    });
    if (launched) return;
    console.warn("Rust TUI executable not found; falling back to legacy TypeScript TUI.");
  }

  await launchLegacyTui({
    cwd,
    modelId,
    providerName,
    apiKey,
    thinkingLevel,
    sessionSpecifier,
    continueSession,
    sessionName,
    noContextFiles,
    noTools,
    systemPrompt,
    promptTemplate,
    skillName,
    hostdEnabled,
    hostdCommand,
    hostdArgs,
  });
}

main().catch((err) => {
  console.error("Fatal:", err instanceof Error ? err.message : String(err));
  process.exit(1);
});
