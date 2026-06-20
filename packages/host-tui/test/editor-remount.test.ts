import { afterEach, describe, expect, it } from "bun:test";
import { testRender } from "@opentui/solid";
import type { Model, ModelProviderConfig } from "piko-orchestrator-protocol";
import { createComponent } from "solid-js";
import { Editor } from "../src/renderer/opentui/Editor.js";
import { createDefaultStore } from "../src/renderer/opentui/store.js";
import { TuiController } from "../src/runtime/tui-controller.js";

const renderers: Array<{ destroy(): void }> = [];

afterEach(() => {
  for (const renderer of renderers.splice(0)) renderer.destroy();
});

function createController(): TuiController {
  const model = {
    id: "test-model",
    name: "Test Model",
    provider: "test",
  } as Model<string>;
  const provider = {} as ModelProviderConfig;
  const store = createDefaultStore(model, provider, process.cwd());
  return new TuiController({} as never, store, () => {});
}

describe("Editor remount", () => {
  it("restores pending tree-navigation text into the mounted textarea", async () => {
    const controller = createController();
    controller.setEditorText("Recovered user entry");

    const setup = await testRender(
      () =>
        createComponent(Editor, {
          actionSvc: {} as never,
          controller,
          disabled: false,
        }),
      { width: 80, height: 12 },
    );
    renderers.push(setup.renderer);
    await setup.flush();

    expect(controller.getEditorText()).toBe("Recovered user entry");
    expect(setup.captureCharFrame()).toContain("Recovered user entry");
  });
});
