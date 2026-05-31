import type { Component, OverlayOptions, TUI } from "@earendil-works/pi-tui";
import { getSelectListTheme } from "../theme.js";
import { showConfirmDialog, showInputDialog, showSelectDialog } from "./dialogs.js";
import type {
  EditorFactory,
  ExtensionEvent,
  ExtensionEventHandler,
  ExtensionHostDeps,
  FooterFactory,
  PikoExtensionAPI,
  PikoExtensionFactory,
  PikoExtensionUI,
  RegisteredCommand,
  RegisteredTool,
  WidgetContent,
  WidgetPlacement,
} from "./types.js";

export class ExtensionHost {
  private deps: ExtensionHostDeps;
  private extensions: Array<{ commands: RegisteredCommand[] }> = [];
  public commands: RegisteredCommand[] = [];
  public tools: RegisteredTool[] = [];
  private eventHandlers = new Map<string, ExtensionEventHandler[]>();

  constructor(deps: ExtensionHostDeps) {
    this.deps = deps;
  }

  private createUI(): PikoExtensionUI {
    const {
      tui,
      theme,
      setWidgetSlot,
      setStatusSlot,
      setFooterFactory,
      setEditorFactory,
      setWorkingIndicatorConfig,
      setEditorText: _setEditorText,
      getEditorText: _getEditorText,
      addChatMessage,
    } = this.deps;

    return {
      theme,

      custom<T>(
        factory: (
          tui: TUI,
          theme: import("../theme.js").Theme,
          done: (result: T) => void,
        ) => Component,
        options?: { overlay?: boolean; overlayOptions?: OverlayOptions },
      ): Promise<T> {
        return new Promise<T>((resolve) => {
          let overlayHandle: { hide(): void } | undefined;
          const component = factory(tui, theme, (result) => {
            overlayHandle?.hide();
            resolve(result);
          });
          overlayHandle = tui.showOverlay(
            {
              render: (w: number) => component.render(w),
              invalidate: () => component.invalidate?.(),
              handleInput: (data: string) => component.handleInput?.(data),
            },
            options?.overlayOptions ?? { anchor: "center", width: "80%", maxHeight: "60%" },
          );
        });
      },

      setWidget(key, content, options) {
        setWidgetSlot(key, content, options?.placement ?? "aboveEditor");
      },

      setStatus(key, text) {
        setStatusSlot(key, text);
      },

      setFooter(factory) {
        setFooterFactory(factory);
      },

      setEditorComponent(factory) {
        setEditorFactory(factory);
      },

      setWorkingIndicator(config) {
        setWorkingIndicatorConfig(config);
      },

      notify(message, level) {
        const prefix =
          level === "error"
            ? theme.fg("error", "⚠")
            : level === "warning"
              ? theme.fg("warning", "⚠")
              : theme.fg("muted", "ℹ");
        addChatMessage("system", `${prefix} ${message}`);
      },

      setEditorText(text) {
        _setEditorText(text);
      },

      getEditorText() {
        return _getEditorText();
      },

      select(title, items, options) {
        return showSelectDialog(
          tui,
          theme,
          title,
          items,
          getSelectListTheme,
          options?.overlayOptions,
        );
      },

      confirm(title, message, options) {
        return showConfirmDialog(
          tui,
          theme,
          title,
          message,
          getSelectListTheme,
          options?.overlayOptions,
        );
      },

      input(title, placeholder, options) {
        return showInputDialog(tui, theme, title, placeholder, options?.overlayOptions);
      },
    };
  }

  async load(factory: PikoExtensionFactory): Promise<void> {
    const cmds: RegisteredCommand[] = [];
    const ui = this.createUI();

    const eventHandlers = this.eventHandlers;
    const registeredTools: RegisteredTool[] = [];

    const api: PikoExtensionAPI = {
      ui,
      registerCommand(value, label, description, handler) {
        cmds.push({ value, label, description, handler: (args, ctx) => handler(args, ctx) });
      },
      registerTool(tool: RegisteredTool) {
        registeredTools.push(tool);
      },
      on(eventType: ExtensionEvent["type"], handler: ExtensionEventHandler) {
        const handlers = eventHandlers.get(eventType) ?? [];
        handlers.push(handler);
        eventHandlers.set(eventType, handlers);
      },
    };

    await factory(api);
    this.extensions.push({ commands: cmds });
    this.commands.push(...cmds);
    this.tools.push(...registeredTools);
  }

  async loadAll(factories: PikoExtensionFactory[]): Promise<void> {
    for (const factory of factories) {
      await this.load(factory);
    }
  }

  bindRuntime(deps: {
    setEditorText: (text: string) => void;
    getEditorText: () => string;
    addChatMessage: (role: string, text: string) => void;
    setFooterFactory: (factory: FooterFactory | undefined) => void;
    setEditorFactory: (factory: EditorFactory | undefined) => void;
    setWidgetSlot: (
      key: string,
      content: WidgetContent | undefined,
      placement: WidgetPlacement,
    ) => void;
    setStatusSlot: (key: string, text: string | undefined) => void;
    setWorkingIndicatorConfig: (
      config?: import("@earendil-works/pi-tui").LoaderIndicatorOptions,
    ) => void;
  }): void {
    Object.assign(this.deps, deps);
  }

  findCommand(input: string): RegisteredCommand | undefined {
    const parts = input.split(/\s+/);
    const cmd = parts[0].toLowerCase();
    return this.commands.find((c) => c.value === cmd);
  }

  /** Dispatch an event to all registered handlers. */
  dispatchEvent(event: ExtensionEvent): void {
    const handlers = this.eventHandlers.get(event.type);
    if (!handlers) return;
    for (const handler of handlers) {
      try {
        void handler(event);
      } catch {
        // Don't let extension errors crash the host
      }
    }
  }
}
