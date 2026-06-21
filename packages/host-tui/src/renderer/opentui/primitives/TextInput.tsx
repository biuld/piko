// ============================================================================
// TextInput — single-line text input using OpenTUI's native textarea element.
//
// Natively supports typing, deletion, cursor movement, and pasting.
// Used for login, session-import, and session-rename panels.
// ============================================================================

import type { TextareaRenderable } from "@opentui/core";
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
  let textareaRef: TextareaRenderable | undefined;
  const [text, setText] = createSignal("");

  onMount(() => {
    props.controller.setSurfaceController(props.surfaceId, {
      handleKey(event) {
        if (event.name === "escape") {
          props.runtime.dispatch({ type: "cancel" });
          return { type: "handled" };
        }
        return { type: "unhandled" };
      },
      onConfirm(val?: any) {
        const valueToSubmit = typeof val === "string" ? val : (textareaRef?.plainText ?? text());
        props.onConfirm(valueToSubmit);
        props.runtime.dispatch({ type: "cancel" });
      },
    });
  });

  onCleanup(() => props.controller.setSurfaceController(props.surfaceId, null));

  return (
    <box flexDirection="column" padding={1}>
      <text>{props.label}</text>
      <box margin={1}>
        <textarea
          ref={(el: TextareaRenderable) => {
            textareaRef = el;
          }}
          focused={true}
          placeholder={props.placeholder || "Enter value..."}
          onContentChange={
            ((val: any) => {
              const textValue = typeof val === "string" ? val : (textareaRef?.plainText ?? "");
              setText(textValue);
            }) as any
          }
          onSubmit={
            ((val: any) => {
              const textValue = typeof val === "string" ? val : (textareaRef?.plainText ?? text());
              props.onConfirm(textValue);
              props.runtime.dispatch({ type: "cancel" });
            }) as any
          }
          keyBindings={[
            { name: "return", action: "submit" },
            { name: "kpenter", action: "submit" },
            { name: "linefeed", action: "submit" },
          ]}
        />
      </box>
    </box>
  );
}
