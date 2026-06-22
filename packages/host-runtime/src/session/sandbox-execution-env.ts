import { err, FileError, ok, type Result, toError } from "piko-session";
import { isAbsolutePath, resolvePath } from "../utils/bun-path.js";
import { BunExecutionEnv } from "./bun-execution-env.js";
import type { ExecutionEnvExecOptions } from "./exec-env.js";
import { ExecutionError } from "./exec-env.js";

export interface SandboxExecutionEnvOptions {
  cwd: string;
  policyPath: string;
  binaryPath?: string;
  shellEnv?: NodeJS.ProcessEnv;
}

function resolveFrom(cwd: string, path: string): string {
  return isAbsolutePath(path) ? path : resolvePath(cwd, path);
}

/**
 * Execution environment whose process boundary is the standalone piko-sandbox
 * executable. Direct filesystem methods stay in-process, but are authorized by
 * the same executable before Bun touches the path.
 */
export class SandboxExecutionEnv extends BunExecutionEnv {
  private readonly binaryPath: string;
  private readonly policyPath: string;
  private readonly sandboxEnv?: NodeJS.ProcessEnv;

  constructor(options: SandboxExecutionEnvOptions) {
    super({ cwd: options.cwd, shellEnv: options.shellEnv });
    this.binaryPath = options.binaryPath ?? "piko-sandbox";
    this.policyPath = resolveFrom(options.cwd, options.policyPath);
    this.sandboxEnv = options.shellEnv;
  }

  private async invoke(
    args: string[],
    options: ExecutionEnvExecOptions & { stdin?: string | Uint8Array } = {},
  ): Promise<Result<{ stdout: string; stderr: string; exitCode: number }, ExecutionError>> {
    if (options.abortSignal?.aborted) return err(new ExecutionError("aborted", "aborted"));
    let child: Bun.Subprocess<"pipe" | "ignore", "pipe", "pipe">;
    try {
      child = Bun.spawn({
        cmd: [this.binaryPath, "--policy", this.policyPath, ...args],
        cwd: this.cwd,
        env: { ...process.env, ...this.sandboxEnv, ...options.env },
        stdin: options.stdin === undefined ? "ignore" : "pipe",
        stdout: "pipe",
        stderr: "pipe",
        detached: process.platform !== "win32",
      });
    } catch (error) {
      const cause = toError(error);
      return err(new ExecutionError("sandbox_unavailable", cause.message, cause));
    }
    if (options.stdin !== undefined && child.stdin) {
      child.stdin.write(options.stdin);
      child.stdin.end();
    }
    let timedOut = false;
    const killTree = () => {
      if (process.platform !== "win32") {
        try {
          process.kill(-child.pid, "SIGKILL");
          return;
        } catch {}
      }
      child.kill("SIGKILL");
    };
    const abort = () => killTree();
    options.abortSignal?.addEventListener("abort", abort, { once: true });
    const timer = options.timeout
      ? setTimeout(() => {
          timedOut = true;
          killTree();
        }, options.timeout * 1000)
      : undefined;
    try {
      const consume = async (
        stream: ReadableStream<Uint8Array>,
        callback?: (chunk: string) => void,
      ): Promise<string> => {
        const reader = stream.getReader();
        const decoder = new TextDecoder();
        let output = "";
        const emit = (chunk: string) => {
          try {
            callback?.(chunk);
          } catch (error) {
            killTree();
            const cause = toError(error);
            throw new ExecutionError("callback_error", cause.message, cause);
          }
        };
        try {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            const chunk = decoder.decode(value, { stream: true });
            output += chunk;
            emit(chunk);
          }
          const tail = decoder.decode();
          output += tail;
          if (tail) emit(tail);
          return output;
        } finally {
          reader.releaseLock();
        }
      };
      const [stdout, stderr, exitCode] = await Promise.all([
        consume(child.stdout, options.onStdout),
        consume(child.stderr, options.onStderr),
        child.exited,
      ]);
      if (timedOut) return err(new ExecutionError("timeout", `timeout:${options.timeout}`));
      if (options.abortSignal?.aborted) return err(new ExecutionError("aborted", "aborted"));
      if (exitCode === 126) {
        return err(
          new ExecutionError("sandbox_denied", stderr.trim() || "sandbox denied operation"),
        );
      }
      return ok({ stdout, stderr, exitCode });
    } catch (error) {
      if (error instanceof ExecutionError) return err(error);
      const cause = toError(error);
      return err(new ExecutionError("spawn_error", cause.message, cause));
    } finally {
      if (timer) clearTimeout(timer);
      options.abortSignal?.removeEventListener("abort", abort);
    }
  }

  override exec(command: string, options: ExecutionEnvExecOptions = {}) {
    const cwd = resolveFrom(this.cwd, options.cwd ?? this.cwd);
    return this.invoke(["exec", "--cwd", cwd, "--", command], options);
  }

  private async authorize(
    path: string,
    access: "read" | "write",
  ): Promise<Result<string, FileError>> {
    const resolved = resolveFrom(this.cwd, path);
    const result = await this.invoke([
      "check-path",
      "--cwd",
      this.cwd,
      "--path",
      resolved,
      "--access",
      access,
      ...(access === "write" ? ["--allow-missing"] : []),
    ]);
    if (!result.ok) return err(new FileError("permission_denied", result.error.message, resolved));
    return ok(resolved);
  }

  override async readTextFile(path: string, signal?: AbortSignal) {
    const allowed = await this.authorize(path, "read");
    return allowed.ok ? super.readTextFile(allowed.value, signal) : allowed;
  }

  override async readBinaryFile(path: string, signal?: AbortSignal) {
    const allowed = await this.authorize(path, "read");
    return allowed.ok ? super.readBinaryFile(allowed.value, signal) : allowed;
  }

  override async readTextLines(
    path: string,
    options?: { maxLines?: number; abortSignal?: AbortSignal },
  ) {
    const allowed = await this.authorize(path, "read");
    return allowed.ok ? super.readTextLines(allowed.value, options) : allowed;
  }

  override async writeFile(
    path: string,
    content: string | Uint8Array,
    signal?: AbortSignal,
  ): Promise<Result<void, FileError>> {
    if (signal?.aborted) return err(new FileError("aborted", "aborted", path));
    const resolved = resolveFrom(this.cwd, path);
    const result = await this.invoke(["write", "--cwd", this.cwd, "--path", resolved], {
      abortSignal: signal,
      stdin: content,
    });
    if (!result.ok) return err(new FileError("permission_denied", result.error.message, resolved));
    return ok(undefined);
  }

  override async appendFile(path: string, content: string | Uint8Array) {
    const allowed = await this.authorize(path, "write");
    return allowed.ok ? super.appendFile(allowed.value, content) : allowed;
  }

  override async fileInfo(path: string) {
    const allowed = await this.authorize(path, "read");
    return allowed.ok ? super.fileInfo(allowed.value) : allowed;
  }

  override async listDir(path: string, signal?: AbortSignal) {
    const allowed = await this.authorize(path, "read");
    return allowed.ok ? super.listDir(allowed.value, signal) : allowed;
  }

  override async canonicalPath(path: string) {
    const allowed = await this.authorize(path, "read");
    return allowed.ok ? super.canonicalPath(allowed.value) : allowed;
  }

  override async exists(path: string) {
    const allowed = await this.authorize(path, "read");
    return allowed.ok ? super.exists(allowed.value) : allowed;
  }

  override async createDir(path: string, options?: { recursive?: boolean }) {
    const allowed = await this.authorize(path, "write");
    return allowed.ok ? super.createDir(allowed.value, options) : allowed;
  }

  override async remove(path: string, options?: { recursive?: boolean; force?: boolean }) {
    const allowed = await this.authorize(path, "write");
    return allowed.ok ? super.remove(allowed.value, options) : allowed;
  }
}
