// ============================================================================
// Commands surface unit tests — verify commands emit clean SurfaceRequests
// without preferredMount / targetSlot.
// ============================================================================

import { describe, expect, it } from "vitest";
import { createBuiltinCommands } from "../src/commands/builtin-commands.js";
import type { SurfaceRequest } from "../src/surfaces/types.js";

/**
 * Create a dummy deps function that captures the first openSurface call.
 */
function captureDeps() {
  let captured: SurfaceRequest | null = null;
  const deps = () => ({
    openSurface: (req: SurfaceRequest) => {
      captured = req;
      return "test-surface-id";
    },
    closeSurface: (_id?: string) => {},
    notify: (_msg: string, _severity?: string) => {},
    getState: () => ({
      model: { current: { id: "x", provider: "x" }, availableModels: [] },
      session: { sessionId: "s1", cwd: "/tmp" },
      stream: { status: "idle" },
      surfaces: [],
    }),
    executeCommand: (_cmdId: string, _args?: string) => {},
    shutdown: () => {},
    abort: () => {},
    host: {} as any,
    dispatch: (_event: any) => {},
    switchModel: (_modelId: string, _provider: string) => true,
  });
  return { deps, getCaptured: () => captured };
}

describe("commands surface requests", () => {
  it("/model surface request has no preferredMount or targetSlot", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps);
    const cmd = cmds.find((c) => c.id === "piko.model.select");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect((req as any).preferredMount).toBeUndefined();
    expect((req as any).targetSlot).toBeUndefined();
    expect(req!.role).toBe("selector");
    expect(req!.contentSize).toBe("medium");
    expect(req!.data).toEqual({ type: "model", filter: undefined });
  });

  it("/settings surface request has no preferredMount", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps);
    const cmd = cmds.find((c) => c.id === "piko.settings.open");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect((req as any).preferredMount).toBeUndefined();
    expect((req as any).targetSlot).toBeUndefined();
    expect(req!.role).toBe("menu");
    expect(req!.contentSize).toBe("medium");
    expect(req!.data).toEqual({ type: "settings" });
  });

  it("/login role is form", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps);
    const cmd = cmds.find((c) => c.id === "piko.auth.login");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.role).toBe("form");
    expect(req!.requiresSecretInput).toBe(true);
  });

  it("/resume contentSize is large", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps);
    const cmd = cmds.find((c) => c.id === "piko.session.resume");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect((req as any).preferredMount).toBeUndefined();
    expect(req!.role).toBe("selector");
    expect(req!.contentSize).toBe("large");
  });

  it("/thinking surface request has no preferredMount", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps);
    const cmd = cmds.find((c) => c.id === "piko.thinking.select");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect((req as any).preferredMount).toBeUndefined();
    expect(req!.role).toBe("selector");
    expect(req!.contentSize).toBe("small");
  });

  it("/hotkeys surface request has no preferredMount", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps);
    const cmd = cmds.find((c) => c.id === "piko.help.hotkeys");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect((req as any).preferredMount).toBeUndefined();
    expect(req!.role).toBe("menu");
  });

  it("/help surface request has no preferredMount", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps);
    const cmd = cmds.find((c) => c.id === "piko.help.show");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect((req as any).preferredMount).toBeUndefined();
    expect(req!.role).toBe("menu");
  });

  it("/tree surface request has no preferredMount", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps);
    const cmd = cmds.find((c) => c.id === "piko.session.tree");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect((req as any).preferredMount).toBeUndefined();
    expect(req!.role).toBe("menu");
    expect(req!.contentSize).toBe("large");
  });
});
