// ============================================================================
// TuiController — wires all UX runtime subsystems together
// ============================================================================

import { createBuiltinCommands } from "../commands/builtin-commands.js";
import { CommandRegistry } from "../commands/command-registry.js";
import { SlashCommandProvider } from "../commands/slash-command-provider.js";
import type { AutocompleteItem } from "../commands/types.js";
import { FocusManager } from "../focus/focus-manager.js";
import type { KeyEvent } from "../focus/types.js";
import { KeymapManager } from "../keymap/keymap-manager.js";
import { NotificationCenter } from "../notifications/notification-center.js";
import type { TuiStore } from "../renderer/opentui/store.js";
import { SurfaceManager } from "../surfaces/surface-manager.js";
import type { SurfaceRequest } from "../surfaces/types.js";
import { ScrollController } from "../timeline/scroll-controller.js";

export class TuiController {
  readonly keymap: KeymapManager;
  readonly commands: CommandRegistry;
  readonly notifications: NotificationCenter;
  readonly focus: FocusManager;
  readonly surfaces: SurfaceManager;
  readonly scroll: ScrollController;
  readonly slashProvider: SlashCommandProvider;
  readonly store: TuiStore;
  private surfaceControllers: Map<string, { handleKey: (e: KeyEvent) => boolean }> = new Map();

  constructor(_host: unknown, store: TuiStore, _shutdown: () => void) {
    this.store = store;

    // Initialize subsystems
    this.keymap = new KeymapManager();
    this.notifications = new NotificationCenter();
    this.focus = new FocusManager();
    this.surfaces = new SurfaceManager();
    this.scroll = new ScrollController();
    this.commands = new CommandRegistry();
    this.slashProvider = new SlashCommandProvider(this.commands);

    // Register built-in commands
    const deps = () => ({
      openSurface: (req: SurfaceRequest) => this.openSurface(req),
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
    });

    this.commands.registerAll(createBuiltinCommands(deps));

    // Wire focus state accessor for interceptor matching
    this.focus.setStateAccessor(() => store.state());

    // Register autocomplete interceptor on the editor focus owner
    this.focus.registerOwner({
      id: "editor",
      region: "editor",
      priority: 0,
      interceptors: [
        {
          id: "editor.slash-autocomplete",
          priority: 100,
          match: (_event, state) => state?.autocomplete?.active === true,
          handle: (event, _state) => {
            if (event.name === "up") {
              store.dispatch({ type: "autocomplete_navigate", delta: -1 });
              return { handled: true };
            }
            if (event.name === "down") {
              store.dispatch({ type: "autocomplete_navigate", delta: 1 });
              return { handled: true };
            }
            if (event.name === "tab") {
              // Tab accepts the selected completion
              store.dispatch({ type: "autocomplete_accept" });
              return { handled: true };
            }
            if (event.name === "escape") {
              store.dispatch({ type: "autocomplete_active", active: false });
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

    // Wire surface events to store
    this.surfaces.onEvent((event) => {
      if (event.type === "surface_opened") {
        store.dispatch({ type: "surface_opened", surface: event.surface });
      } else if (event.type === "surface_closed") {
        store.dispatch({ type: "surface_closed", surfaceId: event.surfaceId });
      }
    });

    // Set global key handler for interrupt / surface close
    this.focus.setGlobalHandler((event: KeyEvent) => {
      if (event.name === "escape") {
        const state = store.state();
        // If there are active surfaces, Esc pops focus (closes top surface)
        if (state.surfaces.length > 0 && !state.autocomplete?.active) {
          const topSurface = state.surfaces[state.surfaces.length - 1];
          this.closeSurface(topSurface.id);
          return true;
        }
        // Interrupt running stream
        if (state.stream.status === "running") {
          this.abort();
          return true;
        }
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
    // Try focus first (global handler, interceptors, owner)
    if (this.focus.handleKey(event)) return true;

    const state = this.store.state();

    // If a blocking surface is active, don't fall through to keymap.
    // Only Esc (global handler) and surface-intercepted keys should work.
    if (state.surfaces.some((s) => s.blocking)) {
      return false;
    }

    // Fallback: keymap → command
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

  /**
   * Open a surface from a command request.
   */
  openSurface(request: SurfaceRequest): string {
    const state = this.store.state();
    const context = this.surfaces.getContext(
      state.layout.viewport.width,
      state.layout.viewport.height,
      state.stream.status === "running",
    );
    const id = this.surfaces.open(request, context);
    const surface = this.surfaces.getSurface(id);
    if (surface && surface.interactionOwner === "self") {
      this.focus.registerOwner({
        id,
        region: "surface",
        priority: 10,
        handleKey: (event) => {
          // Delegate to surface-specific controller (e.g. SelectorController)
          const sc = this.surfaceControllers.get(id);
          if (sc?.handleKey(event)) return { handled: true };
          // Default: Esc closes
          if (event.name === "escape") {
            this.closeSurface(id);
            return { handled: true };
          }
          return { handled: false };
        },
      });
      if (surface.blocking) {
        this.focus.pushFocus(id, "surface", "editor");
      }
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
      openSurface: (req: SurfaceRequest) => this.openSurface(req),
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
    };
  }

  /**
   * Get autocomplete suggestions for current input.
   */
  getAutocomplete(input: string): AutocompleteItem[] {
    return this.slashProvider.getSuggestions(input);
  }

  /**
   * Set a surface's interaction controller (e.g., SelectorController).
   * Called by the surface component on mount. Cleared on close.
   */
  setSurfaceController(
    surfaceId: string,
    ctrl: { handleKey: (e: KeyEvent) => boolean } | null,
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
