import {
  type Component,
  Loader,
  type LoaderIndicatorOptions,
  type TUI,
} from "@earendil-works/pi-tui";
import { getTheme } from "../theme.js";

/**
 * Animated spinner component, wrapping pi-tui's Loader.
 * Shows during streaming to indicate the engine is working.
 */
export class Spinner implements Component {
  private loader: Loader | null = null;
  private _active = false;
  private _message = "Thinking...";
  private tui: TUI | null = null;
  private indicatorConfig?: LoaderIndicatorOptions;

  /** Bind to the TUI (required before start) */
  bind(tui: TUI): void {
    this.tui = tui;
  }

  get active(): boolean {
    return this._active;
  }

  /** Start the spinner animation */
  start(message?: string): void {
    if (message) this._message = message;
    if (this._active) return;
    this._active = true;

    if (this.tui) {
      const t = getTheme();
      this.loader = new Loader(
        this.tui,
        (s) => t.fg("accent", s),
        (s) => t.fg("muted", s),
        this._message,
        this.indicatorConfig,
      );
      this.loader.start();
    }
  }

  /** Stop the spinner animation */
  stop(): void {
    this._active = false;
    if (this.loader) {
      this.loader.stop();
      this.loader = null;
    }
  }

  /** Update the message shown beside the spinner */
  setMessage(message: string): void {
    this._message = message;
    if (this.loader) {
      this.loader.setMessage(message);
    }
  }

  /** Configure frames and interval */
  setIndicator(config?: LoaderIndicatorOptions): void {
    this.indicatorConfig = config;
    if (this.loader) {
      this.loader.setIndicator(config);
    }
  }

  invalidate(): void {
    // Loader handles its own invalidation
  }

  render(_width: number): string[] {
    if (!this._active || !this.loader) return [];
    return this.loader.render(_width);
  }
}
