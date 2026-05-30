import { spawn } from "node:child_process";

export async function bashTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const command = typeof args.command === "string" ? args.command : undefined;
  if (!command) throw new Error("bash requires a string command");
  const timeoutSeconds =
    typeof args.timeout === "number" && args.timeout > 0 ? args.timeout : undefined;
  const shell = process.env.SHELL || "/bin/sh";
  const shellArgs =
    shell.includes("zsh") || shell.includes("bash") ? ["-lc", command] : ["-c", command];

  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(shell, shellArgs, {
      cwd,
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    let timeoutHandle: NodeJS.Timeout | undefined;
    if (timeoutSeconds)
      timeoutHandle = setTimeout(() => child.kill("SIGTERM"), timeoutSeconds * 1000);
    child.stdout?.on("data", (chunk: Buffer | string) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk: Buffer | string) => {
      stderr += chunk.toString();
    });
    child.on("error", (error) => {
      if (timeoutHandle) clearTimeout(timeoutHandle);
      rejectPromise(error);
    });
    child.on("close", (code, signal) => {
      if (timeoutHandle) clearTimeout(timeoutHandle);
      resolvePromise({
        command,
        exitCode: code,
        signal: signal ?? null,
        stdout,
        stderr,
        output: `${stdout}${stderr}`.trim(),
      });
    });
  });
}
