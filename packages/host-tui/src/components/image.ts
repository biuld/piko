/**
 * Terminal image display component — renders images via Kitty graphics protocol
 * or ITerm2 inline image protocol.
 *
 * Falls back to a text placeholder if the terminal doesn't support images.
 */

import type { Component } from "@earendil-works/pi-tui";
import { getTheme } from "../theme.js";

// ============================================================================
// Types
// ============================================================================

export interface ImageDisplayOptions {
  path: string;
  /** Base64-encoded image data (alternative to path). */
  data?: string;
  /** MIME type (required if data is provided). */
  mimeType?: string;
  /** Max display width in cells. Default: 40 */
  maxWidth?: number;
  /** Max display height in rows. Default: 20 */
  maxHeight?: number;
}

// ============================================================================
// Kitty graphics protocol
// ============================================================================

function kittyImageCommand(options: {
  data: string;
  mimeType: string;
  maxWidth: number;
  maxHeight: number;
}): string {
  // Kitty graphics protocol: \x1b_Gkey=val,key=val;base64data\x1b\\
  const chunk = Buffer.from(options.data, "base64");
  const base64 = chunk.toString("base64");

  const params = [
    "a=T", // transmit (not query)
    "f=100", // format: 100 = base64
    `s=${options.maxWidth}`,
    `v=${options.maxHeight}`,
    "c=1", // columns
    "r=1", // rows
  ];

  return `\x1b_G${params.join(",")};${base64}\x1b\\`;
}

// ============================================================================
// ITerm2 inline image protocol
// ============================================================================

function itermImageCommand(options: {
  data: string;
  mimeType: string;
  maxWidth: number;
  maxHeight: number;
}): string {
  const chunk = Buffer.from(options.data, "base64");
  const base64 = chunk.toString("base64");

  return `\x1b]1337;File=inline=1;width=${options.maxWidth}px;height=${options.maxHeight}px:${base64}\x07`;
}

// ============================================================================
// Image component
// ============================================================================

export class ImageComponent implements Component {
  private options: ImageDisplayOptions;
  private rendered = false;

  constructor(options: ImageDisplayOptions) {
    this.options = options;
  }

  invalidate(): void {
    this.rendered = false;
  }

  render(width: number): string[] {
    if (this.rendered) return [];
    this.rendered = true;

    const t = getTheme();
    const maxW = Math.min(this.options.maxWidth ?? 40, width);
    const maxH = this.options.maxHeight ?? 20;

    // Try to load image data from path if not provided
    let data = this.options.data;
    if (!data && this.options.path) {
      try {
        const { readFileSync } = require("node:fs") as typeof import("node:fs");
        const buf = readFileSync(this.options.path);
        data = buf.toString("base64");
      } catch {
        return [t.fg("muted", `[Image: ${this.options.path}]`)];
      }
    }

    if (!data) {
      return [t.fg("muted", `[Image: ${this.options.path}]`)];
    }

    const mimeType = this.options.mimeType ?? "image/png";

    // Try Kitty protocol first, then ITerm2
    const kittySeq = kittyImageCommand({ data, mimeType, maxWidth: maxW, maxHeight: maxH });
    const itermSeq = itermImageCommand({
      data,
      mimeType,
      maxWidth: maxW * 10,
      maxHeight: maxH * 20,
    });

    // Send both — terminal will pick the one it understands
    // We return as part of the render output
    return [
      `${kittySeq}${itermSeq}`,
      t.fg("dim", `[Image: ${this.options.path} (${maxW}×${maxH})]`),
    ];
  }
}
