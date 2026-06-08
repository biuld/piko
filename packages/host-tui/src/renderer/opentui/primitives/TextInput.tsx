// ============================================================================
// TextInput — single-line text input with keyboard handling.
//
// Pure presentational aside from internal keyboard state.
// Used for login, session-import, session-rename panels.
// ============================================================================

import { createSignal, onCleanup, onMount } from "solid-js";
import type { PanelRuntime } from "../../../panels/panel-runtime.js";
import type { TuiController } from "../../../runtime/tui-controller.js";

export interface TextInputProps {
  label: string;
  placeholder?: string;
  controller: TuiController;
  surfaceId: string;
  runtime: PanelRuntime;
  onConfirm: (text: string) => void;
}

export function TextInput(props: TextInputProps) {
  const [text, setText] = createSignal("");

  onMount(() => {
    props.controller.setSurfaceController(props.surfaceId, {
      handleKey(event) {
        if (event.name === "enter" || event.name === "return") {
          return { type: "confirm", value: text() };
        }
        if (event.name === "backspace" || event.name === "delete") {
          setText((t) => t.slice(0, -1));
          return { type: "handled" };
        }
        if (event.ctrl || event.meta || event.alt) {
          return { type: "unhandled" };
        }
        if (event.char && event.char.length === 1 && !event.name.startsWith("f")) {
          setText((t) => t + event.char);
          return { type: "handled" };
        }
        return { type: "unhandled" };
      },
      onConfirm(val?: any) {
        if (typeof val === "string") {
          props.onConfirm(val);
        }
        props.runtime.dispatch({ type: "cancel" });
      },
    });
  });

  onCleanup(() => props.controller.setSurfaceController(props.surfaceId, null));

  return (
    <box flexDirection="column" padding={1}>
      <text>{props.label}</text>
      <text>{text() || props.placeholder || ""}</text>
    </box>
  );
}
