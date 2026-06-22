// ============================================================================
// AssistantMessageView — render assistant messages with markdown, thinking,
// and error states. Pi-aligned rendering.
//
// Pi pattern:
//   - Text blocks rendered as Markdown with fg=text.primary
//   - Thinking blocks rendered in italic thinkingText color (hideable)
//   - Error/aborted messages shown in error color
//   - Spacer between visible content blocks
// ============================================================================

import { TextAttributes } from "@opentui/core";
import { Index, Match, Show, Switch } from "solid-js";
import type { TimelineItem } from "../../../timeline/types.js";
import { useLayout } from "../layout-context.js";
import { useTheme } from "../theme-context.js";
import { getVisibleAssistantBlocks } from "./assistant-blocks.js";
import { MarkdownContent } from "./MarkdownContent.js";

export interface AssistantMessageViewProps {
  item: TimelineItem;
}

export function AssistantMessageView(props: AssistantMessageViewProps) {
  const theme = useTheme();
  const layout = useLayout();

  const hasText = () => Boolean(props.item.text && props.item.text.trim().length > 0);
  const hasThinking = () =>
    Boolean(props.item.thinkingText && props.item.thinkingText.trim().length > 0);
  const hideThinking = () => props.item.hideThinking ?? layout.hideThinking ?? false;
  const isError = () => props.item.isError ?? false;
  const errorMessage = () => props.item.errorMessage;
  const isStreaming = () => props.item.isStreaming ?? false;

  const visibleBlocks = () => {
    return getVisibleAssistantBlocks(props.item);
  };

  const hasVisibleBlocks = () => visibleBlocks().length > 0;

  return (
    <Show
      when={props.item.content && props.item.content.length > 0}
      fallback={
        <Show
          when={hasText() || hasThinking() || isError()}
          fallback={
            <Show when={isStreaming()}>
              <box paddingLeft={1} paddingRight={1}>
                <text fg={theme.color("text.muted")}>...</text>
              </box>
            </Show>
          }
        >
          <box flexDirection="column" paddingLeft={1} paddingRight={1}>
            <box height={1} />
            {/* Legacy Thinking block — rendered before text, in italic thinkingText color */}
            <Show when={hasThinking() && !hideThinking()}>
              <box paddingTop={hasText() ? 1 : 0} paddingBottom={hasText() ? 1 : 0}>
                <text fg={theme.color("thinking.text")} attributes={TextAttributes.ITALIC}>
                  {props.item.thinkingText!.trim()}
                </text>
              </box>
            </Show>

            {/* Legacy Hidden thinking label */}
            <Show when={hasThinking() && hideThinking()}>
              <box paddingTop={1}>
                <text fg={theme.color("thinking.hiddenLabel")} attributes={TextAttributes.ITALIC}>
                  Thinking...
                </text>
              </box>
            </Show>

            {/* Legacy Main text content — rendered as Markdown */}
            <Show when={hasText()}>
              <MarkdownContent
                content={props.item.text!.trim()}
                fg={theme.color("text.primary")}
                streaming={isStreaming()}
                conceal={true}
              />
            </Show>

            {/* Legacy Error / aborted message */}
            <Show when={isError() && errorMessage()}>
              <box paddingTop={hasText() || hasThinking() ? 1 : 0}>
                <text fg={theme.color("text.error")}>{errorMessage()}</text>
              </box>
            </Show>
          </box>
        </Show>
      }
    >
      <Show
        when={hasVisibleBlocks() || (isError() && errorMessage())}
        fallback={
          <Show when={isStreaming() && props.item.content && props.item.content.length === 0}>
            <box paddingLeft={1} paddingRight={1}>
              <text fg={theme.color("text.muted")}>...</text>
            </box>
          </Show>
        }
      >
        <box flexDirection="column" paddingLeft={1} paddingRight={1}>
          <box height={1} />
          <Index each={visibleBlocks()}>
            {(block, index) => (
              <Switch>
                <Match when={block().type === "thinking"}>
                  <Show
                    when={hideThinking()}
                    fallback={
                      <box
                        paddingTop={index > 0 ? 1 : 0}
                        paddingBottom={index < visibleBlocks().length - 1 ? 1 : 0}
                      >
                        <text fg={theme.color("thinking.text")} attributes={TextAttributes.ITALIC}>
                          {String((block() as any).thinking || "").trim()}
                        </text>
                      </box>
                    }
                  >
                    <box
                      paddingTop={index > 0 ? 1 : 0}
                      paddingBottom={index < visibleBlocks().length - 1 ? 1 : 0}
                    >
                      <text
                        fg={theme.color("thinking.hiddenLabel")}
                        attributes={TextAttributes.ITALIC}
                      >
                        Thinking...
                      </text>
                    </box>
                  </Show>
                </Match>
                <Match when={block().type === "text"}>
                  <MarkdownContent
                    content={String((block() as any).text || "").trim()}
                    fg={theme.color("text.primary")}
                    streaming={isStreaming()}
                    conceal={true}
                  />
                </Match>
              </Switch>
            )}
          </Index>
          {/* Error / aborted message */}
          <Show when={isError() && errorMessage()}>
            <box paddingTop={hasVisibleBlocks() ? 1 : 0}>
              <text fg={theme.color("text.error")}>{errorMessage()}</text>
            </box>
          </Show>
        </box>
      </Show>
    </Show>
  );
}
