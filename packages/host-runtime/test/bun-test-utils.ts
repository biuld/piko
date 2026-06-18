import { mkdirp } from "../src/utils/bun-fs.js";
import { joinPath } from "../src/utils/bun-path.js";

function tmpdir(): string {
  return Bun.env.TMPDIR ?? Bun.env.TEMP ?? Bun.env.TMP ?? "/tmp";
}

function assertCommand(
  result: { exitCode: number | null; stderr?: string | Uint8Array },
  command: string,
): void {
  if (result.exitCode === 0) return;
  const message = result.stderr
    ? typeof result.stderr === "string"
      ? result.stderr.trim()
      : new TextDecoder().decode(result.stderr).trim()
    : "";
  throw new Error(message || `Command failed: ${command}`);
}

function writeFileSync(path: string, contents: string, _encoding?: string): void {
  const result = Bun.spawnSync(["tee", path], {
    stdin: new TextEncoder().encode(contents),
    stdout: "ignore",
    stderr: "pipe",
  });
  assertCommand(result, `tee ${path}`);
}

function readFileSync(path: string, _encoding?: string): string {
  const result = Bun.spawnSync(["cat", path], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  assertCommand(result, `cat ${path}`);
  return new TextDecoder().decode(result.stdout);
}

function mkdirSync(path: string, _options?: { recursive?: boolean }): void {
  const result = Bun.spawnSync(["mkdir", "-p", path], {
    stdin: "ignore",
    stdout: "ignore",
    stderr: "pipe",
  });
  assertCommand(result, `mkdir -p ${path}`);
}

function rmSync(path: string, _options?: { recursive?: boolean; force?: boolean }): void {
  const result = Bun.spawnSync(["rm", "-rf", path], {
    stdin: "ignore",
    stdout: "ignore",
    stderr: "pipe",
  });
  assertCommand(result, `rm -rf ${path}`);
}

function mkdtempSync(prefix: string): string {
  const dir = `${prefix}${crypto.randomUUID()}`;
  mkdirSync(dir, { recursive: true });
  return dir;
}

async function mkdtemp(prefix: string): Promise<string> {
  const dir = `${prefix}${crypto.randomUUID()}`;
  await mkdirp(dir);
  return dir;
}

async function writeFile(path: string, contents: string, _encoding?: string): Promise<void> {
  await Bun.write(path, contents);
}

async function mkdir(path: string, _options?: { recursive?: boolean }): Promise<void> {
  await mkdirp(path);
}

export function execSync(command: string, options?: { cwd?: string; stdio?: "ignore" }): void {
  const result = Bun.spawnSync(["/bin/sh", "-c", command], {
    cwd: options?.cwd,
    stdin: "ignore",
    stdout: options?.stdio === "ignore" ? "ignore" : "pipe",
    stderr: options?.stdio === "ignore" ? "ignore" : "pipe",
  });
  assertCommand(result, command);
}

export const fs = {
  mkdir,
  mkdirSync,
  mkdtemp,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFile,
  writeFileSync,
};

export { joinPath as join, tmpdir };
