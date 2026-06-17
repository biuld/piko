// ============================================================================
// Commands panel unit tests — verify commands emit clean PanelRequests
// ============================================================================

import { describe, expect, it } from "bun:test";
import { createBuiltinCommands } from "../src/commands/builtin-commands/index.js";
import type { PanelSurfaceRequest } from "../src/surfaces/types.js";

/**
 * Create a dummy deps function that captures the first openPanel call.
 */
function captureDeps() {
  let captured: PanelSurfaceRequest | null = null;
  const deps = () => ({
    openSurface: (_req: any) => {
      return "test-surface-id";
    },
    openPanel: (req: PanelSurfaceRequest) => {
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

describe("commands panel requests", () => {
  it("/model emits partial panel", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps as any);
    const cmd = cmds.find((c) => c.id === "piko.model.select");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.placement).toBe("partial");
    expect(req!.panel.stack[0].body.type).toBe("model-picker");
  });

  it("/settings emits partial panel", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps as any);
    const cmd = cmds.find((c) => c.id === "piko.settings.open");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.placement).toBe("partial");
    expect(req!.panel.stack[0].body.type).toBe("settings");
  });

  it("/login emits partial capture panel with provider picker if no provider specified", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps as any);
    const cmd = cmds.find((c) => c.id === "piko.auth.login");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.placement).toBe("partial");
    expect(req!.inputPolicy).toBe("capture");
    expect(req!.panel.stack[0].body.type).toBe("provider-picker");
  });

  it("/login <provider> emits partial capture panel directly to login form", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps as any);
    const cmd = cmds.find((c) => c.id === "piko.auth.login");
    expect(cmd).toBeDefined();
    cmd!.run({} as any, "openai");
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.placement).toBe("partial");
    expect(req!.panel.stack[0].body.type).toBe("login");
    expect(req!.panel.stack[0].body.payload.provider).toBe("openai");
  });

  it("/resume emits full panel", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps as any);
    const cmd = cmds.find((c) => c.id === "piko.session.resume");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.placement).toBe("full");
    expect(req!.panel.stack[0].body.type).toBe("session-resume");
  });

  it("/thinking emits partial panel", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps as any);
    const cmd = cmds.find((c) => c.id === "piko.thinking.select");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.placement).toBe("partial");
    expect(req!.panel.stack[0].body.type).toBe("thinking-picker");
  });

  it("/hotkeys emits partial panel", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps as any);
    const cmd = cmds.find((c) => c.id === "piko.help.hotkeys");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.placement).toBe("partial");
    expect(req!.panel.stack[0].body.type).toBe("hotkeys");
  });

  it("/help emits partial panel", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps as any);
    const cmd = cmds.find((c) => c.id === "piko.help.show");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.placement).toBe("partial");
    expect(req!.panel.stack[0].body.type).toBe("help");
  });

  it("/tree emits full panel", () => {
    const { deps, getCaptured } = captureDeps();
    const cmds = createBuiltinCommands(deps as any);
    const cmd = cmds.find((c) => c.id === "piko.session.tree");
    expect(cmd).toBeDefined();
    cmd!.run({} as any);
    const req = getCaptured();
    expect(req).not.toBeNull();
    expect(req!.placement).toBe("full");
    expect(req!.panel.stack[0].body.type).toBe("session-tree");
  });
});
