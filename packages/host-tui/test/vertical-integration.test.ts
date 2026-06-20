import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import {
  type FauxProviderRegistration,
  fauxAssistantMessage,
  registerFauxProvider,
} from "@earendil-works/pi-ai";
import { testRender } from "@opentui/solid";
import * as fs from "fs/promises";
import { tmpdir } from "os";
import { join } from "path";
import { createHostConfig, PikoHost, SessionManager } from "piko-host-runtime";
import { createModelCaller } from "piko-orchestrator";
import type { Message, Model, ModelProviderConfig } from "piko-orchestrator-protocol";
import { createComponent } from "solid-js";
import { App } from "../src/renderer/opentui/App.js";
import { ActionService } from "../src/renderer/opentui/action-service.js";
import { createDefaultStore } from "../src/renderer/opentui/store.js";
import { TuiController } from "../src/runtime/tui-controller.js";

const PROVIDER = "faux";
const API = "openai-completions";
const MODEL_ID = "faux-host-model";

let faux: FauxProviderRegistration;
const originalHome = process.env.HOME;

const renderers: Array<{ destroy(): void }> = [];

beforeAll(() => {
  faux = registerFauxProvider({
    api: API,
    provider: PROVIDER,
    models: [{ id: MODEL_ID }],
  });
});

afterAll(() => {
  faux?.unregister();
  process.env.HOME = originalHome;
  for (const renderer of renderers.splice(0)) renderer.destroy();
});

function buildTestModel(): Model<string> {
  return {
    id: MODEL_ID,
    name: "Faux Host Model",
    api: API,
    provider: PROVIDER,
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128000,
    maxTokens: 16384,
  };
}

describe("Vertical integration navigation and branch", () => {
  it("runs the full session tree navigation and branch sequence", async () => {
    // 1. Setup temporary HOME and CWD
    const home = await fs.mkdtemp(join(tmpdir(), "piko-integration-home-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-integration-cwd-"));

    // 2. Create user → assistant session entries in real SessionManager
    const sessionManager = await SessionManager.create(cwd);
    const userMsg: Message = {
      role: "user",
      content: "Original user text message",
      timestamp: Date.now(),
    };
    const assistantMsg: Message = {
      role: "assistant",
      content: [{ type: "text", text: "Assistant response" }],
      timestamp: Date.now() + 1,
    } as any;
    await sessionManager.saveMessages(MODEL_ID, [userMsg, assistantMsg]);

    const allEntries = await sessionManager.getEntries();
    const userEntry = allEntries.find((e) => e.type === "message" && e.message.role === "user");
    expect(userEntry).toBeDefined();

    // 3. Initialize real host and settings
    const settingsManager = {
      getHideThinkingBlock: () => false,
      getTheme: () => "dark",
      onChange: () => {},
    } as any;

    const hostConfig = createHostConfig(buildTestModel(), undefined, { allowToolCalls: false });
    const host = PikoHost.fromSessionManager(createModelCaller(), hostConfig, sessionManager);
    await host.restoreFromSession();

    const store = createDefaultStore(buildTestModel(), {} as ModelProviderConfig, cwd, {
      hideThinking: false,
      theme: "dark",
    });

    // Wires up components
    const actionSvcObj = new ActionService(host, store, settingsManager, {} as any, () => {});

    const controller = new TuiController(host, store, () => {});
    controller.setActionService(actionSvcObj);

    // Initial load of session data
    const initialEntries = await host.loadBranchEntries();
    const { entriesToTranscript } = await import("../src/timeline/entries-to-transcript.js");
    store.dispatch({
      type: "session_resumed",
      sessionId: host.sessionFile ?? "",
      sessionName: undefined,
      transcript: entriesToTranscript(initialEntries),
    });

    // 4. Mount the App
    const setup = await testRender(
      () =>
        createComponent(App, {
          store,
          host,
          controller,
          actionSvc: actionSvcObj,
          options: {
            settingsManager,
            modelRegistry: {} as any,
          } as any,
          shutdown: () => {},
        }),
      { width: 80, height: 24 },
    );
    renderers.push(setup.renderer);
    await setup.flush();

    // Verify timeline initially contains user and assistant message
    expect(store.state().timeline.items).toHaveLength(2);

    // 5. Open the session tree using the controller
    controller.executeSlash("/tree");
    await setup.waitFor(() => (controller as any).surfaceControllers.size > 0);

    const surfaceId = Array.from((controller as any).surfaceControllers.keys())[0] as string;
    const treeCtrl = (controller as any).surfaceControllers.get(surfaceId);
    expect(treeCtrl).toBeDefined();

    // 6. Navigate up to select the user entry (current selection is the assistant message)
    treeCtrl.handleKey({ name: "up" });
    await setup.flush();

    // Confirm navigation
    treeCtrl.onConfirm();

    // 7. Wait for the surface to close (meaning navigation succeeded)
    await setup.waitFor(() => (controller as any).surfaceControllers.size === 0);

    // 8. Assertions on navigated state
    // - Persisted JSONL leaf target equals the user's parent (null/root)
    expect(await sessionManager.getLeafId()).toBeNull();
    // - Surface is closed in Store, SurfaceManager, and FocusManager
    expect(store.state().surfaces).toHaveLength(0);
    expect(controller.surfaces.getAllSurfaces()).toHaveLength(0);
    expect(controller.focus.getState().activeOwnerId).toBe("editor");
    expect(controller.focus.getState().stack).toEqual(["editor"]);
    // - messageCount is updated to match the navigated timeline
    expect(store.state().session.messageCount).toBe(0);
    // - Timeline is empty (since parent of root user message is null)
    expect(store.state().timeline.items).toHaveLength(0);
    // - Real textarea contains the selected user text
    expect(store.state().input.draft).toBe("Original user text message");
    expect(setup.captureCharFrame()).toContain("Original user text message");

    // 9. Submit the restored text
    faux.setResponses([fauxAssistantMessage("New branch response")]);

    // Simulate submitting the prompt
    await actionSvcObj.submitPrompt("Original user text message");

    // Wait for the stream to finish and become idle
    await setup.waitFor(() => store.state().stream.status === "idle");
    await setup.flush();

    // 10. Assert a new pi-compatible user branch was created
    const finalEntries = await sessionManager.getEntries();
    const userMessages = finalEntries.filter(
      (e) => e.type === "message" && e.message.role === "user",
    );
    expect(userMessages).toHaveLength(2); // Original user message + the new branched user message

    const _leaves = finalEntries.filter((e) => e.type === "leaf");
    // Ensure the new leaf points to the newly created assistant message
    expect(await sessionManager.getLeafId()).not.toBeNull();
  });
});
