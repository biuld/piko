import { err, FileError, ok, type Result, toError } from "piko-session";
import { dirnamePath, isAbsolutePath, resolvePath } from "../utils/bun-path.js";
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
 * Returns true when this Node/Bun process is already running inside an Apple
 * App Sandbox (APP_SANDBOX_CONTAINER_ID is set by the OS). In that situation
 * /usr/bin/sandbox-exec cannot re-initialise the kernel seatbelt and will
 * SIGABRT. We detect it here so we can degrade gracefully — the piko-sandbox
 * binary's `check` / `check-path` subcommands still enforce the policy ACL
 * before any filesystem access, so the security boundary is maintained.
 */
function isAppSandboxed(): boolean {
  return typeof process.env.APP_SANDBOX_CONTAINER_ID === "string";
}

/**
 * Execution environment whose process boundary is the standalone piko-sandbox
 * executable. Direct filesystem methods stay in-process, but are authorized by
 * the same executable before Bun touches the path.
 *
 * When the host process is already running inside an Apple App Sandbox
 * (detected via APP_SANDBOX_CONTAINER_ID), the sandbox-exec wrapper is skipped
 * to avoid a nested-sandbox SIGABRT. The piko-sandbox ACL check subcommands
 * are still invoked before every filesystem operation.
 */
export class SandboxExecutionEnv extends BunExecutionEnv {
  private binaryPath: string;
  private binaryResolved = false;
  private readonly policyPath: string;
  private readonly sandboxEnv?: NodeJS.ProcessEnv;
  /**
   * False when the process is running inside a nested sandbox (e.g. App
   * Sandbox / Xcode task runner) where sandbox-exec cannot be used. When
   * false, exec() falls back to direct execution after ACL-checking the
   * command via the piko-sandbox `check` subcommand.
   */
  private readonly sandboxAvailable: boolean;

  constructor(options: SandboxExecutionEnvOptions) {
    super({ cwd: options.cwd, shellEnv: options.shellEnv });
    this.binaryPath = options.binaryPath ?? "piko-sandbox";
    this.policyPath = resolveFrom(options.cwd, options.policyPath);
    this.sandboxEnv = options.shellEnv;
    this.sandboxAvailable = !isAppSandboxed();
    if (!this.sandboxAvailable) {
      console.warn(
        "[piko-sandbox] Nested App Sandbox detected (APP_SANDBOX_CONTAINER_ID is set). " +
          "sandbox-exec will be skipped; ACL checks via piko-sandbox check remain active.",
      );
    }
  }

  private async resolveBinaryPath(): Promise<string> {
    if (this.binaryResolved) {
      return this.binaryPath;
    }
    this.binaryResolved = true;
    if (this.binaryPath === "piko-sandbox") {
      const candidates: string[] = [];

      // 1. Next to the executing process (works for compiled binaries)
      const execDir = resolvePath(process.execPath, "..");
      candidates.push(resolvePath(execDir, "piko-sandbox"));

      // 2. Next to the current source module file (works for bun run and dev)
      if (import.meta.path) {
        const moduleDir = resolvePath(import.meta.path, "..");
        candidates.push(resolvePath(moduleDir, "piko-sandbox"));

        // 3. Workspace target/release directory (works for local dev compilation)
        const workspaceRoot = resolvePath(moduleDir, "..", "..", "..", "..");
        candidates.push(
          resolvePath(workspaceRoot, "packages", "sandbox", "target", "release", "piko-sandbox"),
        );
        candidates.push(resolvePath(workspaceRoot, "dist-bundle", "piko-sandbox"));
      }

      for (const path of candidates) {
        if (await Bun.file(path).exists()) {
          this.binaryPath = path;
          break;
        }
      }
      console.warn(
        `[piko-sandbox-debug] checked candidates: [${candidates.join(", ")}], resolved to: ${this.binaryPath}`,
      );
    }
    return this.binaryPath;
  }

  private async ensurePolicyFile(): Promise<void> {
    const file = Bun.file(this.policyPath);
    if (await file.exists()) {
      return;
    }
    // Write default policy
    const defaultPolicy = {
      version: 1,
      read: [".", "/usr", "/bin", "/private/tmp", "/System", "/private/var"],
      write: [".", "/private/tmp"],
      deny: [".git", ".piko/sandbox.json"],
      allowedCommands: [
        "bun",
        "git",
        "rg",
        "sed",
        "tsc",
        "npm",
        "node",
        "cat",
        "ls",
        "find",
        "bash",
        "sh",
        "sleep",
        "printf",
        "echo",
      ],
      allowNetwork: false,
    };
    try {
      const dir = dirnamePath(this.policyPath);
      const { mkdir } = await import("node:fs/promises");
      await mkdir(dir, { recursive: true });
      await Bun.write(this.policyPath, JSON.stringify(defaultPolicy, null, 2));
    } catch {}
  }

  private async invoke(
    args: string[],
    options: ExecutionEnvExecOptions & { stdin?: string | Uint8Array } = {},
  ): Promise<Result<{ stdout: string; stderr: string; exitCode: number }, ExecutionError>> {
    if (options.abortSignal?.aborted) return err(new ExecutionError("aborted", "aborted"));
    await this.ensurePolicyFile();
    const bin = await this.resolveBinaryPath();
    let child: Bun.Subprocess<"pipe" | "ignore", "pipe", "pipe">;
    try {
      child = Bun.spawn({
        cmd: [bin, "--policy", this.policyPath, ...args],
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

  override async exec(command: string, options: ExecutionEnvExecOptions = {}) {
    const cwd = resolveFrom(this.cwd, options.cwd ?? this.cwd);

    // When running inside a nested sandbox we cannot use sandbox-exec.
    // Fall back to: ACL-check the command via `check`, then run directly.
    if (!this.sandboxAvailable) {
      const checked = await this.invoke(["check", "--cwd", cwd, "--", command], options);
      if (!checked.ok) return checked;
      return super.exec(command, { ...options, cwd });
    }

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
