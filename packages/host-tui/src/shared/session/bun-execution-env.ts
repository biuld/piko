import { makeTempDir, mkdirp, realPath, removePath } from "../utils/bun-fs.js";
import { isAbsolutePath, joinPath, resolvePath as resolveFsPath } from "../utils/bun-path.js";
import {
  type ExecutionEnv,
  ExecutionError,
  err,
  FileError,
  type FileInfo,
  type FileKind,
  ok,
  type Result,
  toError,
} from "./exec-env.js";

function resolvePath(cwd: string, path: string): string {
  return isAbsolutePath(path) ? path : resolveFsPath(cwd, path);
}

function fileKindFromStats(stats: {
  isFile(): boolean;
  isDirectory(): boolean;
  isSymbolicLink(): boolean;
}): FileKind | undefined {
  if (stats.isFile()) return "file";
  if (stats.isDirectory()) return "directory";
  if (stats.isSymbolicLink()) return "symlink";
  return undefined;
}

function fileInfoFromStats(
  path: string,
  stats: {
    isFile(): boolean;
    isDirectory(): boolean;
    isSymbolicLink(): boolean;
    size: number;
    mtimeMs: number;
  },
): Result<FileInfo, FileError> {
  const kind = fileKindFromStats(stats);
  if (!kind) return err(new FileError("invalid", "Unsupported file type", path));
  return ok({
    name: path.replace(/\/+$/, "").split("/").pop() ?? path,
    path,
    kind,
    size: stats.size,
    mtimeMs: stats.mtimeMs,
  });
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

function toFileError(error: unknown, path?: string): FileError {
  if (error instanceof FileError) return error;
  const cause = toError(error);
  if (isNodeError(error)) {
    const message = error.message;
    switch (error.code) {
      case "ABORT_ERR":
        return new FileError("aborted", message, path, cause);
      case "ENOENT":
        return new FileError("not_found", message, path, cause);
      case "EACCES":
      case "EPERM":
        return new FileError("permission_denied", message, path, cause);
      case "ENOTDIR":
        return new FileError("not_directory", message, path, cause);
      case "EISDIR":
        return new FileError("is_directory", message, path, cause);
      case "EINVAL":
        return new FileError("invalid", message, path, cause);
    }
  }
  return new FileError("unknown", cause.message, path, cause);
}

function abortResult<TValue>(
  signal: AbortSignal | undefined,
  path?: string,
): Result<TValue, FileError> | undefined {
  return signal?.aborted ? err(new FileError("aborted", "aborted", path)) : undefined;
}

async function pathExists(path: string): Promise<boolean> {
  return Bun.file(path).exists();
}

async function runCommand(
  command: string,
  args: string[],
  timeoutMs: number,
): Promise<{ stdout: string; status: number | null }> {
  try {
    const result = Bun.spawnSync([command, ...args], {
      stdin: "ignore",
      stdout: "pipe",
      stderr: "ignore",
      timeout: timeoutMs,
    });
    return {
      stdout: new TextDecoder().decode(result.stdout),
      status: result.exitCode,
    };
  } catch {
    return { stdout: "", status: null };
  }
}

async function findBashOnPath(): Promise<string | null> {
  const result =
    process.platform === "win32"
      ? await runCommand("where", ["bash.exe"], 5000)
      : await runCommand("which", ["bash"], 5000);
  if (result.status !== 0 || !result.stdout) return null;
  const firstMatch = result.stdout.trim().split(/\r?\n/)[0];
  return firstMatch && (await pathExists(firstMatch)) ? firstMatch : null;
}

async function getShellConfig(
  customShellPath?: string,
): Promise<Result<{ shell: string; args: string[] }, ExecutionError>> {
  if (customShellPath) {
    if (await pathExists(customShellPath)) {
      return ok({ shell: customShellPath, args: ["-c"] });
    }
    return err(
      new ExecutionError("shell_unavailable", `Custom shell path not found: ${customShellPath}`),
    );
  }
  if (process.platform === "win32") {
    const candidates: string[] = [];
    const programFiles = process.env.ProgramFiles;
    if (programFiles) candidates.push(`${programFiles}\\Git\\bin\\bash.exe`);
    const programFilesX86 = process.env["ProgramFiles(x86)"];
    if (programFilesX86) candidates.push(`${programFilesX86}\\Git\\bin\\bash.exe`);
    for (const candidate of candidates) {
      if (await pathExists(candidate)) {
        return ok({ shell: candidate, args: ["-c"] });
      }
    }
    const bashOnPath = await findBashOnPath();
    if (bashOnPath) {
      return ok({ shell: bashOnPath, args: ["-c"] });
    }
    return err(new ExecutionError("shell_unavailable", "No bash shell found"));
  }

  if (await pathExists("/bin/bash")) {
    return ok({ shell: "/bin/bash", args: ["-c"] });
  }
  const bashOnPath = await findBashOnPath();
  if (bashOnPath) {
    return ok({ shell: bashOnPath, args: ["-c"] });
  }
  return ok({ shell: "sh", args: ["-c"] });
}

function getShellEnv(
  baseEnv?: NodeJS.ProcessEnv,
  extraEnv?: Record<string, string>,
): NodeJS.ProcessEnv {
  return {
    ...process.env,
    ...baseEnv,
    ...extraEnv,
  };
}

function killProcessTree(pid: number): void {
  if (process.platform === "win32") {
    try {
      Bun.spawn(["taskkill", "/F", "/T", "/PID", String(pid)], {
        stdin: "ignore",
        stdout: "ignore",
        stderr: "ignore",
      });
    } catch {
      // Ignore errors.
    }
    return;
  }

  try {
    process.kill(-pid, "SIGKILL");
  } catch {
    try {
      process.kill(pid, "SIGKILL");
    } catch {
      // Process already dead.
    }
  }
}

export class BunExecutionEnv implements ExecutionEnv {
  cwd: string;
  private shellPath?: string;
  private shellEnv?: NodeJS.ProcessEnv;

  constructor(options: { cwd: string; shellPath?: string; shellEnv?: NodeJS.ProcessEnv }) {
    this.cwd = options.cwd;
    this.shellPath = options.shellPath;
    this.shellEnv = options.shellEnv;
  }

  async absolutePath(path: string): Promise<Result<string, FileError>> {
    return ok(resolvePath(this.cwd, path));
  }

  async joinPath(parts: string[]): Promise<Result<string, FileError>> {
    return ok(joinPath(...parts));
  }

  async exec(
    command: string,
    options?: {
      cwd?: string;
      env?: Record<string, string>;
      timeout?: number;
      abortSignal?: AbortSignal;
      onStdout?: (chunk: string) => void;
      onStderr?: (chunk: string) => void;
    },
  ): Promise<Result<{ stdout: string; stderr: string; exitCode: number }, ExecutionError>> {
    if (options?.abortSignal?.aborted) return err(new ExecutionError("aborted", "aborted"));

    const cwd = options?.cwd ? resolvePath(this.cwd, options.cwd) : this.cwd;
    const shellConfig = await getShellConfig(this.shellPath);
    if (!shellConfig.ok) return shellConfig;

    return await new Promise((resolvePromise) => {
      let stdout = "";
      let stderr = "";
      let settled = false;
      let timedOut = false;
      let callbackError: ExecutionError | undefined;
      let child: Bun.Subprocess<"ignore", "pipe", "pipe"> | undefined;
      let timeoutId: ReturnType<typeof setTimeout> | undefined;

      const onAbort = () => {
        if (child?.pid) {
          killProcessTree(child.pid);
        }
      };

      const settle = (
        result: Result<{ stdout: string; stderr: string; exitCode: number }, ExecutionError>,
      ) => {
        if (timeoutId) clearTimeout(timeoutId);
        if (options?.abortSignal) options.abortSignal.removeEventListener("abort", onAbort);
        if (settled) return;
        settled = true;
        resolvePromise(result);
      };

      try {
        child = Bun.spawn({
          cmd: [shellConfig.value.shell, ...shellConfig.value.args, command],
          cwd,
          env: getShellEnv(this.shellEnv, options?.env),
          stdin: "ignore",
          stdout: "pipe",
          stderr: "pipe",
        });
      } catch (error) {
        const cause = toError(error);
        settle(err(new ExecutionError("spawn_error", cause.message, cause)));
        return;
      }

      timeoutId =
        typeof options?.timeout === "number"
          ? setTimeout(() => {
              timedOut = true;
              if (child?.pid) {
                killProcessTree(child.pid);
              }
            }, options.timeout * 1000)
          : undefined;

      if (options?.abortSignal) {
        if (options.abortSignal.aborted) {
          onAbort();
        } else {
          options.abortSignal.addEventListener("abort", onAbort, { once: true });
        }
      }

      const consumeStream = async (
        stream: ReadableStream<Uint8Array>,
        onChunk: ((chunk: string) => void) | undefined,
      ): Promise<string> => {
        const decoder = new TextDecoder();
        const reader = stream.getReader();
        let output = "";
        try {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            const chunk = decoder.decode(value, { stream: true });
            output += chunk;
            try {
              onChunk?.(chunk);
            } catch (error) {
              const cause = toError(error);
              callbackError = new ExecutionError("callback_error", cause.message, cause);
              onAbort();
              break;
            }
          }
          const tail = decoder.decode();
          if (tail) {
            output += tail;
            onChunk?.(tail);
          }
          return output;
        } finally {
          reader.releaseLock();
        }
      };

      Promise.all([
        consumeStream(child.stdout, options?.onStdout),
        consumeStream(child.stderr, options?.onStderr),
        child.exited,
      ])
        .then(([stdoutValue, stderrValue, code]) => {
          stdout = stdoutValue;
          stderr = stderrValue;
          if (callbackError) {
            settle(err(callbackError));
            return;
          }
          if (timedOut) {
            settle(err(new ExecutionError("timeout", `timeout:${options?.timeout}`)));
            return;
          }
          if (options?.abortSignal?.aborted) {
            settle(err(new ExecutionError("aborted", "aborted")));
            return;
          }
          settle(ok({ stdout, stderr, exitCode: code ?? 0 }));
        })
        .catch((error) => {
          const cause = toError(error);
          settle(err(new ExecutionError("spawn_error", cause.message, cause)));
        });
    });
  }

  async readTextFile(path: string, abortSignal?: AbortSignal): Promise<Result<string, FileError>> {
    const resolved = resolvePath(this.cwd, path);
    const aborted = abortResult<string>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      const text = await Bun.file(resolved).text();
      const afterReadAbort = abortResult<string>(abortSignal, resolved);
      if (afterReadAbort) return afterReadAbort;
      return ok(text);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async readTextLines(
    path: string,
    options?: { maxLines?: number; abortSignal?: AbortSignal },
  ): Promise<Result<string[], FileError>> {
    const resolved = resolvePath(this.cwd, path);
    const aborted = abortResult<string[]>(options?.abortSignal, resolved);
    if (aborted) return aborted;
    if (options?.maxLines !== undefined && options.maxLines <= 0) return ok([]);
    try {
      const text = await Bun.file(resolved).text();
      const lines =
        options?.maxLines === undefined
          ? text.split(/\r?\n/)
          : text.split(/\r?\n/, options.maxLines);
      const afterReadAbort = abortResult<string[]>(options?.abortSignal, resolved);
      if (afterReadAbort) return afterReadAbort;
      return ok(lines);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async readBinaryFile(
    path: string,
    abortSignal?: AbortSignal,
  ): Promise<Result<Uint8Array, FileError>> {
    const resolved = resolvePath(this.cwd, path);
    const aborted = abortResult<Uint8Array>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      const bytes = await Bun.file(resolved).bytes();
      const afterReadAbort = abortResult<Uint8Array>(abortSignal, resolved);
      if (afterReadAbort) return afterReadAbort;
      return ok(bytes);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async writeFile(
    path: string,
    content: string | Uint8Array,
    abortSignal?: AbortSignal,
  ): Promise<Result<void, FileError>> {
    const resolved = resolvePath(this.cwd, path);
    const aborted = abortResult<void>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      await mkdirp(resolveFsPath(resolved, ".."));
      const afterMkdirAbort = abortResult<void>(abortSignal, resolved);
      if (afterMkdirAbort) return afterMkdirAbort;
      await Bun.write(resolved, content);
      return ok(undefined);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async appendFile(path: string, content: string | Uint8Array): Promise<Result<void, FileError>> {
    const resolved = resolvePath(this.cwd, path);
    try {
      await mkdirp(resolveFsPath(resolved, ".."));
      const existing = (await Bun.file(resolved).exists())
        ? await Bun.file(resolved).bytes()
        : new Uint8Array();
      const next =
        typeof content === "string"
          ? new Blob([existing, content])
          : new Blob([existing, content.slice()]);
      await Bun.write(resolved, next);
      return ok(undefined);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async fileInfo(path: string): Promise<Result<FileInfo, FileError>> {
    const resolved = resolvePath(this.cwd, path);
    try {
      return fileInfoFromStats(resolved, await Bun.file(resolved).stat());
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async listDir(path: string, abortSignal?: AbortSignal): Promise<Result<FileInfo[], FileError>> {
    const resolved = resolvePath(this.cwd, path);
    const aborted = abortResult<FileInfo[]>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      const glob = new Bun.Glob("*");
      const infos: FileInfo[] = [];
      for await (const name of glob.scan({ cwd: resolved, onlyFiles: false, dot: true })) {
        const loopAbort = abortResult<FileInfo[]>(abortSignal, resolved);
        if (loopAbort) return loopAbort;
        const entryPath = resolveFsPath(resolved, name);
        try {
          const info = fileInfoFromStats(entryPath, await Bun.file(entryPath).stat());
          if (info.ok) infos.push(info.value);
        } catch (error) {
          return err(toFileError(error, entryPath));
        }
      }
      return ok(infos);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async canonicalPath(path: string): Promise<Result<string, FileError>> {
    const resolved = resolvePath(this.cwd, path);
    try {
      return ok(await realPath(resolved));
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async exists(path: string): Promise<Result<boolean, FileError>> {
    const result = await this.fileInfo(path);
    if (result.ok) return ok(true);
    if (result.error.code === "not_found") return ok(false);
    return err(result.error);
  }

  async createDir(
    path: string,
    _options?: { recursive?: boolean },
  ): Promise<Result<void, FileError>> {
    const resolved = resolvePath(this.cwd, path);
    try {
      await mkdirp(resolved);
      return ok(undefined);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async remove(
    path: string,
    options?: { recursive?: boolean; force?: boolean },
  ): Promise<Result<void, FileError>> {
    const resolved = resolvePath(this.cwd, path);
    try {
      await removePath(resolved, {
        recursive: options?.recursive ?? false,
        force: options?.force ?? false,
      });
      return ok(undefined);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async createTempDir(prefix: string = "tmp-"): Promise<Result<string, FileError>> {
    try {
      return ok(await makeTempDir(prefix));
    } catch (error) {
      return err(toFileError(error));
    }
  }

  async createTempFile(options?: {
    prefix?: string;
    suffix?: string;
  }): Promise<Result<string, FileError>> {
    const dir = await this.createTempDir("tmp-");
    if (!dir.ok) return dir;
    const filePath = joinPath(
      dir.value,
      `${options?.prefix ?? ""}${crypto.randomUUID()}${options?.suffix ?? ""}`,
    );
    try {
      await Bun.write(filePath, "");
      return ok(filePath);
    } catch (error) {
      return err(toFileError(error, filePath));
    }
  }

  async cleanup(): Promise<void> {
    // Nothing to clean up for the local Bun implementation.
  }
}

export { BunExecutionEnv as NodeExecutionEnv };
