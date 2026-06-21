import { joinPath, resolvePath } from "./bun-path.js";

function run(command: string[]): void {
  const result = Bun.spawnSync(command, {
    stdin: "ignore",
    stdout: "ignore",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) {
    const message = new TextDecoder().decode(result.stderr).trim();
    throw new Error(message || `Command failed: ${command.join(" ")}`);
  }
}

export async function mkdirp(path: string): Promise<void> {
  if (process.platform === "win32") {
    run(["cmd", "/c", "mkdir", path]);
    return;
  }
  run(["mkdir", "-p", path]);
}

export async function removePath(
  path: string,
  options?: { recursive?: boolean; force?: boolean },
): Promise<void> {
  if (process.platform === "win32") {
    const args = options?.recursive
      ? ["cmd", "/c", "rmdir", "/s", "/q", path]
      : ["cmd", "/c", "del", "/q", path];
    run(args);
    return;
  }
  const args = ["rm"];
  if (options?.recursive) args.push("-r");
  if (options?.force) args.push("-f");
  args.push(path);
  run(args);
}

export async function makeTempDir(prefix: string): Promise<string> {
  const root = Bun.env.TMPDIR ?? Bun.env.TEMP ?? Bun.env.TMP ?? "/tmp";
  for (let attempt = 0; attempt < 16; attempt++) {
    const dir = joinPath(root, `${prefix}${crypto.randomUUID()}`);
    if (await Bun.file(dir).exists()) continue;
    await mkdirp(dir);
    return dir;
  }
  throw new Error("Failed to create temporary directory");
}

export async function realPath(path: string): Promise<string> {
  if (process.platform === "win32") return resolvePath(path);
  const result = Bun.spawnSync(["realpath", path], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) {
    const message = new TextDecoder().decode(result.stderr).trim();
    throw new Error(message || `Failed to resolve real path: ${path}`);
  }
  return new TextDecoder().decode(result.stdout).trim();
}
