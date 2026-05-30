import type { Component } from "@earendil-works/pi-tui";

const SPINNER_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

export class Spinner implements Component {
  private frameIndex = 0;
  private label: string;
  private _active = false;
  private timer?: ReturnType<typeof setInterval>;

  constructor(label = "Thinking...") {
    this.label = label;
  }

  get active(): boolean {
    return this._active;
  }

  start(): void {
    if (this._active) return;
    this._active = true;
    this.frameIndex = 0;
    this.timer = setInterval(() => {
      this.frameIndex = (this.frameIndex + 1) % SPINNER_FRAMES.length;
    }, 80);
  }

  stop(): void {
    this._active = false;
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = undefined;
    }
  }

  setLabel(label: string): void {
    this.label = label;
  }

  invalidate(): void {}

  render(_width: number): string[] {
    if (!this._active) return [];
    const frame = SPINNER_FRAMES[this.frameIndex];
    return [`${frame} ${this.label}`];
  }
}
