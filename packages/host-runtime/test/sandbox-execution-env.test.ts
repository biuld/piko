import { afterEach, describe, expect, test } from "bun:test";
import { chmod, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { SandboxExecutionEnv } from "../src/session/sandbox-execution-env.js";

const dirs: string[] = [];

async function fixture(): Promise<{ root: string; binary: string }> {
  const root = await mkdtemp(join(tmpdir(), "piko-sandbox-env-"));
  dirs.push(root);
  const binary = join(root, "fake-sandbox");
  await Bun.write(
    binary,
    `#!/bin/sh
case "$3" in
  check-path)
    case "$7" in *denied*) echo "access denied" >&2; exit 126;; esac
    exit 0
    ;;
  exec)
    shift 6
    /bin/sh -c "$1"
    ;;
  write)
    /bin/cat > "$7"
    ;;
esac
echo "bad invocation: $*" >&2
exit 2
`,
  );
  await chmod(binary, 0o755);
  await Bun.write(join(root, "policy.json"), "{}");
  return { root, binary };
}

afterEach(async () => {
  await Promise.all(dirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

describe("SandboxExecutionEnv", () => {
  test("executes commands through the configured supervisor", async () => {
    const { root, binary } = await fixture();
    const env = new SandboxExecutionEnv({
      cwd: root,
      policyPath: "policy.json",
      binaryPath: binary,
    });
    const result = await env.exec("printf sandboxed");
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.value.stdout).toBe("sandboxed");
  });

  test("streams output and reports timeouts", async () => {
    const { root, binary } = await fixture();
    const env = new SandboxExecutionEnv({
      cwd: root,
      policyPath: "policy.json",
      binaryPath: binary,
    });
    const chunks: string[] = [];
    const streamed = await env.exec("printf first; sleep 0.1; printf second", {
      onStdout: (chunk) => chunks.push(chunk),
    });
    expect(streamed.ok).toBe(true);
    expect(chunks.join("")).toBe("firstsecond");

    const timedOut = await env.exec("sleep 2", { timeout: 0.05 });
    expect(timedOut.ok).toBe(false);
    if (!timedOut.ok) expect(timedOut.error.code).toBe("timeout");
  });

  test("maps supervisor policy rejections to permission_denied", async () => {
    const { root, binary } = await fixture();
    const env = new SandboxExecutionEnv({
      cwd: root,
      policyPath: "policy.json",
      binaryPath: binary,
    });
    const result = await env.readTextFile("denied.txt");
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.code).toBe("permission_denied");
  });

  test("authorizes filesystem writes before touching the target", async () => {
    const { root, binary } = await fixture();
    const env = new SandboxExecutionEnv({
      cwd: root,
      policyPath: "policy.json",
      binaryPath: binary,
    });
    const result = await env.writeFile("allowed.txt", "value");
    expect(result.ok).toBe(true);
    expect(await Bun.file(join(root, "allowed.txt")).text()).toBe("value");
  });
});
