import type {
  Component,
  EditorComponent,
  LoaderIndicatorOptions,
  OverlayOptions,
  SelectItem,
  TUI,
} from "@earendil-works/pi-tui";
import type { Theme } from "../theme.js";

// ============================================================================
// Basic types
// ============================================================================

export type NotifyLevel = "info" | "warning" | "error";
export type WidgetPlacement = "aboveEditor" | "belowEditor";

export type WidgetContent =
  | string[]
  | ((tui: TUI, theme: Theme) => Component & { dispose?(): void });

export interface WidgetOptions {
  placement?: WidgetPlacement;
}

export type { LoaderIndicatorOptions as WorkingIndicatorConfig } from "@earendil-works/pi-tui";

export type FooterFactory = (tui: TUI, theme: Theme) => Component & { dispose?(): void };
export type EditorFactory = (tui: TUI, theme: Theme) => EditorComponent;

// ============================================================================
// Extension API interfaces
// ============================================================================

export interface PikoExtensionUI {
  custom<T>(
    factory: (tui: TUI, theme: Theme, done: (result: T) => void) => Component,
    options?: { overlay?: boolean; overlayOptions?: OverlayOptions },
  ): Promise<T>;
  setWidget(key: string, content: WidgetContent | undefined, options?: WidgetOptions): void;
  setStatus(key: string, text: string | undefined): void;
  setFooter(factory: FooterFactory | undefined): void;
  setEditorComponent(factory: EditorFactory | undefined): void;
  setWorkingIndicator(config?: LoaderIndicatorOptions): void;
  notify(message: string, level?: NotifyLevel): void;
  setEditorText(text: string): void;
  getEditorText(): string;
  select(
    title: string,
    items: SelectItem[],
    options?: { overlayOptions?: OverlayOptions },
  ): Promise<string | undefined>;
  confirm(
    title: string,
    message: string,
    options?: { overlayOptions?: OverlayOptions },
  ): Promise<boolean>;
  input(
    title: string,
    placeholder?: string,
    options?: { overlayOptions?: OverlayOptions },
  ): Promise<string | undefined>;
  readonly theme: Theme;
}

export interface RegisteredCommand {
  value: string;
  label: string;
  description: string;
  handler: (args: string, ctx: PikoExtensionUI) => void | Promise<void>;
}

export interface PikoExtensionAPI {
  ui: PikoExtensionUI;
  registerCommand(
    value: string,
    label: string,
    description: string,
    handler: (args: string, ctx: PikoExtensionUI) => void | Promise<void>,
  ): void;
}

export type PikoExtensionFactory = (api: PikoExtensionAPI) => void | Promise<void>;

// ============================================================================
// Extension host deps
// ============================================================================

export interface ExtensionHostDeps {
  tui: TUI;
  theme: Theme;
  setEditorText: (text: string) => void;
  getEditorText: () => string;
  addChatMessage: (role: string, text: string) => void;
  requestRender: () => void;
  setFooterFactory: (factory: FooterFactory | undefined) => void;
  setEditorFactory: (factory: EditorFactory | undefined) => void;
  setWidgetSlot: (
    key: string,
    content: WidgetContent | undefined,
    placement: WidgetPlacement,
  ) => void;
  setStatusSlot: (key: string, text: string | undefined) => void;
  setWorkingIndicatorConfig: (config?: LoaderIndicatorOptions) => void;
}
