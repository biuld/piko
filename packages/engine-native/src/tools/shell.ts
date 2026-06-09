import { spawn } from "node:child_process";
import { resolve } from "node:path";

export interface ShellArgs {
  command: string;
  timeout?: number;
  cwd?: string;
  login?: boolean;
}

export interface ShellResult {
  command: string;
  exitCode: number | null;
  stdout: string;
  stderr: string;
  durationMs: number;
  timedOut: boolean;
  truncated: boolean;
}

const MAX_OUTPUT_BYTES = 50 * 1024; // 50KB truncation

export async function shellTool(
  workspaceCwd: string,
  args: Record<string, unknown>,
): Promise<ShellResult> {
  const command = typeof args.command === "string" ? args.command : undefined;
  if (!command) throw new Error("shell requires a string command");

  const timeoutSeconds =
    typeof args.timeout === "number" && args.timeout > 0 ? args.timeout : undefined;
  const cwd =
    typeof args.cwd === "string" && args.cwd.trim()
      ? resolve(workspaceCwd, args.cwd)
      : workspaceCwd;
  const login = args.login === true;

  const shell = process.env.SHELL || "/bin/sh";
  const shellArgs = login
    ? ["-lc", command]
    : shell.includes("zsh") || shell.includes("bash")
      ? ["-lc", command]
      : ["-c", command];

  const startTime = Date.now();

  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(shell, shellArgs, {
      cwd,
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    let truncated = false;
    let timeoutHandle: ReturnType<typeof setTimeout> | undefined;

    if (timeoutSeconds) {
      timeoutHandle = setTimeout(() => {
        child.kill("SIGTERM");
      }, timeoutSeconds * 1000);
    }

    child.stdout?.on("data", (chunk: Buffer | string) => {
      const str = chunk.toString();
      if (stdout.length + str.length > MAX_OUTPUT_BYTES) {
        truncated = true;
        stdout += str.slice(0, MAX_OUTPUT_BYTES - stdout.length);
      } else {
        stdout += str;
      }
    });

    child.stderr?.on("data", (chunk: Buffer | string) => {
      const str = chunk.toString();
      if (stderr.length + str.length > MAX_OUTPUT_BYTES) {
        truncated = true;
        stderr += str.slice(0, MAX_OUTPUT_BYTES - stderr.length);
      } else {
        stderr += str;
      }
    });

    child.on("error", (error) => {
      if (timeoutHandle) clearTimeout(timeoutHandle);
      rejectPromise(error);
    });

    child.on("close", (code, signal) => {
      if (timeoutHandle) clearTimeout(timeoutHandle);
      const durationMs = Date.now() - startTime;
      const timedOut = signal === "SIGTERM" && timeoutSeconds !== undefined;
      resolvePromise({
        command,
        exitCode: code,
        stdout,
        stderr,
        durationMs,
        timedOut,
        truncated,
      });
    });
  });
}
