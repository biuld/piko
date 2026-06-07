// ============================================================================
// TuiController — wires all UX runtime subsystems together
// ============================================================================

import type { PikoHost } from "piko-host-runtime";
import {
  CombinedAutocompleteProvider,
  FileAutocompleteProvider,
  SlashCommandAutocompleteProvider,
} from "../autocomplete/index.js";
import type { AutocompleteItem, AutocompleteSuggestions } from "../autocomplete/types.js";
import { createBuiltinCommands } from "../commands/builtin-commands.js";
import { CommandRegistry } from "../commands/command-registry.js";
import type { EditorAutocompleteController } from "../editor/editor-autocomplete-controller.js";
import { FocusManager } from "../focus/focus-manager.js";
import { InputRouter } from "../focus/input-router.js";
import { normalizeKeyEvent } from "../focus/key-normalize.js";
import type { KeyEvent } from "../focus/types.js";
import { KeymapManager } from "../keymap/keymap-manager.js";
import { NotificationCenter } from "../notifications/notification-center.js";
import { traceSurfaceClose, traceSurfaceOpen } from "../renderer/opentui/instrumentation.js";
import type { TuiStore } from "../renderer/opentui/store.js";
import { type SurfaceKeyResult, SurfaceManager } from "../surfaces/index.js";
import type { PanelSurfaceRequest } from "../surfaces/types.js";
import { ScrollController } from "../timeline/scroll-controller.js";

export class TuiController {
  readonly keymap: KeymapManager;
  readonly commands: CommandRegistry;
  readonly notifications: NotificationCenter;
  readonly focus: FocusManager;
  readonly input: InputRouter;
  readonly surfaces: SurfaceManager;
  readonly scroll: ScrollController;
  readonly slashProvider: SlashCommandAutocompleteProvider;
  readonly autocomplete: CombinedAutocompleteProvider;
  readonly store: TuiStore;
  private surfaceControllers: Map<
    string,
    {
      handleKey: (e: KeyEvent) => SurfaceKeyResult;
      onConfirm?: (value?: any) => void;
      onSubmit?: (value?: any) => void;
    }
  > = new Map();
  private _host: PikoHost;
  /** EditorAutocompleteController reference for global Esc guard (not in store). */
  private _autocompleteController: EditorAutocompleteController | null = null;
  /** Accessor for current editor text (for double-ESC detection). */
  private editorTextAccessor?: () => string;
  /** Timestamp of last Escape press (for double-ESC detection). */
  private lastEscapeTime = 0;

  constructor(host: PikoHost, store: TuiStore, _shutdown: () => void) {
    this.store = store;
    this._host = host;

    // Initialize subsystems
    this.keymap = new KeymapManager();
    this.notifications = new NotificationCenter();
    this.focus = new FocusManager();
    this.input = new InputRouter({
      focus: this.focus,
      getState: () => this.store.state(),
      appFallback: (event) => this.handleAppFallbackKey(event),
    });
    this.surfaces = new SurfaceManager();
    this.scroll = new ScrollController();
    this.commands = new CommandRegistry();
    this.slashProvider = new SlashCommandAutocompleteProvider(this.commands);
    this.autocomplete = new CombinedAutocompleteProvider([
      { id: "slash", provider: this.slashProvider },
      { id: "file", provider: new FileAutocompleteProvider(store.state().session.cwd) },
    ]);

    // Load keybinding overrides from config files
    this.keymap.loadFromFiles(store.state().session.cwd);
    // Report only global app-level conflicts at startup (context-specific bindings
    // like tui.select.up / tui.timeline.up are expected to share keys)
    const conflicts = this.keymap.detectConflicts("global");
    if (conflicts.length > 0) {
      queueMicrotask(() => {
        for (const c of conflicts) {
          this.notifications.notify({
            message: `Keybinding conflict: ${c.id1} and ${c.id2} both bound to ${c.key}`,
            severity: "warning",
            source: "runtime",
          });
        }
      });
    }

    // Register built-in commands
    const deps = () => ({
      openPanel: (req: PanelSurfaceRequest) => this.openPanel(req),
      closeSurface: (id?: string) => this.closeSurface(id),
      notify: (msg: string, severity?: string) =>
        this.notifications.notify({
          message: msg,
          severity: severity as any,
          source: "command",
        }),
      getState: () => this.store.state(),
      executeCommand: (cmdId: string, args?: string) =>
        this.commands.execute(cmdId, this.createCommandContext(), args),
      shutdown: () => this.shutdown(),
      abort: () => this.abort(),
      host,
      dispatch: (event: any) => this.store.dispatch(event),
      switchModel: (modelId: string, provider: string) => {
        const svc = (this as any)._actionSvc;
        if (svc?.switchModel) return svc.switchModel(modelId, provider);
        return false;
      },
      modelRegistry: (this as any)._actionSvc?.modelRegistry,
    });

    this.commands.registerAll(createBuiltinCommands(deps));

    // Wire focus state accessor for interceptor matching
    this.focus.setStateAccessor(() => store.state());

    // Sync focus state changes to store
    this.focus.onChange((focusState) => {
      store.dispatch({
        type: "focus_changed",
        activeOwnerId: focusState.activeOwnerId,
        region: focusState.region,
      });
    });

    // Register editor focus owner (timeline scroll interceptor only;
    // autocomplete keys are handled locally by Editor.)
    this.focus.registerOwner({
      id: "editor",
      region: "editor",
      priority: 0,
      interceptors: [
        // Timeline scroll interceptor (PageUp/PageDown/End when no blocking surface)
        {
          id: "editor.timeline-scroll",
          priority: 50,
          match: (_event, state) => {
            if (!state) return false;
            if (
              state.surfaces?.some((s: any) =>
                "blocking" in s ? s.blocking : s.inputPolicy !== "passive",
              )
            )
              return false;
            return ["pageup", "pagedown", "end"].includes(_event.name);
          },
          handle: (event, state) => {
            if (event.name === "pageup") {
              const seq = state._scrollSeq + 1;
              store.setState((s: any) => ({
                ...s,
                _scrollSeq: seq,
                scrollCommand: { dir: "pageUp", seq },
              }));
              return { handled: true };
            }
            if (event.name === "pagedown") {
              const seq = state._scrollSeq + 1;
              store.setState((s: any) => ({
                ...s,
                _scrollSeq: seq,
                scrollCommand: { dir: "pageDown", seq },
              }));
              return { handled: true };
            }
            if (event.name === "end") {
              store.dispatch({ type: "timeline_jump_latest" });
              const seq = state._scrollSeq + 1;
              store.setState((s: any) => ({
                ...s,
                _scrollSeq: seq,
                scrollCommand: { dir: "jumpLatest", seq },
              }));
              return { handled: true };
            }
            return { handled: false };
          },
        },
      ],
    });

    // Wire notification events to store
    this.notifications.onEvent((event) => {
      if (event.type === "notification_added") {
        store.dispatch({ type: "notification_added", notification: event.notification });
      } else if (event.type === "notification_cleared") {
        store.dispatch({ type: "notification_cleared", id: event.id });
      } else if (event.type === "notification_read") {
        store.dispatch({ type: "notification_read", id: event.id });
      }
    });

    // Wire surface events to store + instrumentation
    this.surfaces.onEvent((event) => {
      if (event.type === "surface_opened") {
        const role = "panel";
        const mount = event.surface.placement === "full" ? "replace-slot" : "insert-between";
        traceSurfaceOpen(event.surface.id, role, mount);
        store.dispatch({ type: "surface_opened", surface: event.surface });
      } else if (event.type === "surface_closed") {
        traceSurfaceClose(event.surfaceId);
        store.dispatch({ type: "surface_closed", surfaceId: event.surfaceId });
      }
    });

    // Set global key handler for Esc: surface → autocomplete → stream abort
    this.focus.setGlobalHandler((event: KeyEvent) => {
      if (event.name !== "escape") return false;

      const activeSurfaces = this.surfaces.getAllSurfaces();

      if (activeSurfaces.length > 0) {
        const topSurface = activeSurfaces.reduce((top, surface) =>
          surface.zIndex > top.zIndex ? surface : top,
        );
        this.closeSurface(topSurface.id);
        return true;
      }

      // 2. Cancel autocomplete if visible
      const acCtrl = this._autocompleteController;
      if (acCtrl?.state.visible) {
        acCtrl.cancel();
        return true;
      }

      // 3. Interrupt running stream
      const currentState = store.state();
      if (currentState.stream.status === "running") {
        this.abort();
        return true;
      }

      // 4. Double-escape with empty editor → show tree / fork (pi-compatible)
      const editorText = this.editorTextAccessor?.() ?? "";
      if (!editorText.trim()) {
        const settingsManager = (this._host as any)?.getSettingsManager?.();
        const action = settingsManager?.getDoubleEscapeAction?.() ?? "tree";
        if (action !== "none") {
          const now = Date.now();
          if (now - this.lastEscapeTime < 500) {
            if (action === "tree") {
              this.commands.execute("piko.session.tree", this.createCommandContext());
            } else {
              this.commands.execute("piko.session.fork", this.createCommandContext());
            }
            this.lastEscapeTime = 0;
          } else {
            this.lastEscapeTime = now;
          }
        }
        return true;
      }

      return false;
    });
  }

  /**
   * Route keyboard events through focus → keymap.
   * Focus runs first (global handler → interceptors → owner).
   * Keymap is the fallback for non-focused keybindings.
   */
  handleKey(event: KeyEvent): boolean {
    const normalizedEvent = normalizeKeyEvent(event);
    if (!normalizedEvent) return false;
    return this.input.dispatch(normalizedEvent);
  }

  private handleAppFallbackKey(event: KeyEvent): boolean {
    const state = this.store.state();
    const isStreamRunning = state.stream.status === "running";

    const bindingId = this.keymap.findBinding(
      event.name,
      event.ctrl ?? false,
      event.shift ?? false,
      event.alt ?? false,
      event.meta ?? false,
    );

    if (bindingId) {
      if (this.keymap.requiresIdle(bindingId) && isStreamRunning) {
        this.notifications.notify({
          message: "Command unavailable while running",
          severity: "warning",
        });
        return true;
      }

      const cmd = this.commands.findByKeybinding(bindingId);
      if (cmd) {
        const ctx = this.createCommandContext();
        cmd.run(ctx);
        return true;
      }
    }

    return false;
  }

  /**
   * Handle interrupt (Escape during streaming).
   */
  handleInterrupt(): void {
    const actionSvc = (this as any)._actionSvc;
    if (actionSvc?.abortRun) {
      actionSvc.abortRun();
    }
    this.notifications.notify({ message: "Interrupted", severity: "info" });
  }

  openPanel(request: PanelSurfaceRequest): string {
    const id = this.surfaces.openPanel(request);
    this.focus.registerOwner({
      id,
      region: "surface",
      priority: 10,
      handleKey: (event) => {
        const sc = this.surfaceControllers.get(id);
        if (sc) {
          const result = sc.handleKey(event);
          switch (result.type) {
            case "handled":
              return { handled: true };
            case "close":
              this.closeSurface(id);
              return { handled: true };
            case "confirm":
              if (sc.onConfirm) sc.onConfirm(result.value);
              else this.closeSurface(id);
              return { handled: true };
            case "submit":
              if (sc.onSubmit) sc.onSubmit(result.value);
              else this.closeSurface(id);
              return { handled: true };
          }
        }
        if (event.name === "escape") {
          this.closeSurface(id);
          return { handled: true };
        }
        return { handled: false };
      },
    });
    if (request.inputPolicy !== "passive") {
      this.focus.pushFocus(id, "surface", "editor");
    }
    return id;
  }

  /**
   * Close a surface (or all surfaces).
   */
  closeSurface(id?: string): void {
    if (id) {
      this.surfaces.close(id);
      this.focus.closeSurface(id);
      this.focus.unregisterOwner(id);
    } else {
      const all = this.surfaces.getAllSurfaces();
      for (const s of all) {
        this.surfaces.close(s.id);
        this.focus.closeSurface(s.id);
        this.focus.unregisterOwner(s.id);
      }
    }
  }

  /**
   * Execute a slash command.
   */
  async executeSlash(text: string): Promise<boolean> {
    const ctx = this.createCommandContext();
    const found = await this.commands.executeSlash(text, ctx);
    if (!found) {
      this.notifications.notify({
        message: `Unknown command: ${text}`,
        severity: "error",
        source: "command",
      });
    }
    return found;
  }

  /**
   * Submit user prompt.
   */
  submitPrompt(text: string): void {
    const trimmed = text.trim();
    if (!trimmed) return;

    // Delegate to action service
    const actionSvc = (this as any)._actionSvc;
    if (actionSvc?.submitPrompt) {
      actionSvc.submitPrompt(trimmed);
    }
  }

  /**
   * Create a command context for the current state.
   */
  private createCommandContext() {
    return {
      openPanel: (req: PanelSurfaceRequest) => this.openPanel(req),
      closeSurface: (id?: string) => this.closeSurface(id),
      notify: (msg: string, severity?: string) =>
        this.notifications.notify({
          message: msg,
          severity: severity as any,
          source: "command",
        }),
      getState: () => this.store.state(),
      executeCommand: (cmdId: string, args?: string) =>
        this.commands.execute(cmdId, this.createCommandContext(), args),
      shutdown: () => this.shutdown(),
      abort: () => this.abort(),
      host: this._host,
      dispatch: (event: any) => this.store.dispatch(event),
      switchModel: (modelId: string, provider: string) => {
        const svc = (this as any)._actionSvc;
        if (svc?.switchModel) return svc.switchModel(modelId, provider);
        return false;
      },
    };
  }

  /**
   * Set the EditorAutocompleteController reference (for global Esc guard).
   * Called by Editor on mount; cleared on unmount.
   */
  setAutocompleteController(ctrl: EditorAutocompleteController | null): void {
    this._autocompleteController = ctrl;
  }

  /**
   * Set the editor text accessor for double-ESC detection.
   * Called by Editor on mount; cleared on unmount.
   */
  setEditorTextAccessor(fn: (() => string) | null): void {
    this.editorTextAccessor = fn ?? undefined;
  }

  /**
   * Set the editor-local autocomplete key handler.
   * Called by Editor on mount; cleared on unmount.
   * Keys are intercepted in handleKey() BEFORE focus routing.
   */
  setAutocompleteKeyHandler(handler: ((event: KeyEvent) => boolean) | null): void {
    this.input.setEditorChildHandler(handler);
  }

  /**
   * Synchronous autocomplete fallback for slash commands.
   * Used as instant response while the async provider loads.
   */
  getAutocomplete(input: string): AutocompleteItem[] {
    // Use the slash provider synchronously for navigation count
    // (the provider API is async but the CommandRegistry queries are sync)
    return this.commands
      .listSlashCommands()
      .filter((cmd) => {
        const prefix = input.trimStart().toLowerCase();
        if (!prefix.startsWith("/")) return false;
        if (cmd.name.toLowerCase().startsWith(prefix)) return true;
        return cmd.aliases?.some((a) => a.toLowerCase().startsWith(prefix)) ?? false;
      })
      .map((cmd) => ({
        value: cmd.name,
        label: cmd.name,
        providerId: "slash",
        description: `${cmd.description}${
          cmd.aliases?.length ? ` (${cmd.aliases.join(", ")})` : ""
        }`,
      }));
  }

  /**
   * Get autocomplete suggestions asynchronously using the full provider chain.
   */
  async getAutocompleteAsync(
    input: string,
    cursor: number,
    signal?: AbortSignal,
  ): Promise<AutocompleteSuggestions | null> {
    return this.autocomplete.getSuggestions(input, cursor, {
      force: false,
      signal: signal ?? new AbortController().signal,
    });
  }

  setSurfaceController(
    surfaceId: string,
    ctrl: {
      handleKey: (e: KeyEvent) => SurfaceKeyResult;
      onConfirm?: (value?: any) => void;
      onSubmit?: (value?: any) => void;
    } | null,
  ): void {
    if (ctrl) {
      this.surfaceControllers.set(surfaceId, ctrl);
    } else {
      this.surfaceControllers.delete(surfaceId);
    }
  }

  /**
   * Wire the action service reference for stream operations.
   */
  setActionService(svc: any): void {
    (this as any)._actionSvc = svc;
    // Wire notification callback so ActionService can produce notifications
    if (svc && typeof svc === "object") {
      svc.onNotify = (message: string, severity?: string) => {
        this.notifications.notify({
          message,
          severity: severity as any,
          source: "stream",
        });
      };
    }
  }

  /**
   * Shutdown the application (exit).
   */
  shutdown(): void {
    const svc = (this as any)._actionSvc as { shutdown?: () => void } | undefined;
    if (svc?.shutdown) {
      svc.shutdown();
    } else {
      process.exit(0);
    }
  }

  /**
   * Abort the current stream.
   */
  abort(): void {
    const svc = (this as any)._actionSvc as { abortRun?: () => void } | undefined;
    if (svc?.abortRun) {
      svc.abortRun();
    }
  }
}
