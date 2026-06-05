// ============================================================================
// Key normalization — renderer key events to runtime KeyEvent
// ============================================================================

import type { KeyEvent } from "./types.js";

export interface RawKeyEvent {
  name?: string;
  sequence?: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  option?: boolean;
  meta?: boolean;
  super?: boolean;
  hyper?: boolean;
  char?: string;
}

export function normalizeKeyName(name?: string, sequence?: string): string {
  const raw = name || (sequence === "\x1b" || sequence === "\u001b" ? "escape" : "");
  const normalized = raw.toLowerCase();
  if (normalized === "arrowup" || normalized === "arrow_up") return "up";
  if (normalized === "arrowdown" || normalized === "arrow_down") return "down";
  if (normalized === "arrowleft" || normalized === "arrow_left") return "left";
  if (normalized === "arrowright" || normalized === "arrow_right") return "right";
  if (normalized === "enter") return "return";
  if (normalized === "esc") return "escape";
  return normalized;
}

export function normalizeKeyEvent(raw: RawKeyEvent): KeyEvent | null {
  const name = normalizeKeyName(raw.name, raw.sequence);
  if (!name) return null;

  const ctrl = raw.ctrl ?? false;
  const meta = raw.meta ?? false;
  const char =
    raw.char ??
    (!ctrl &&
    !meta &&
    !raw.super &&
    !raw.hyper &&
    raw.sequence &&
    raw.sequence.length === 1 &&
    raw.sequence >= " "
      ? raw.sequence
      : undefined);

  return {
    name,
    ctrl,
    shift: raw.shift ?? false,
    alt: raw.option ?? raw.alt ?? false,
    meta,
    char,
  };
}
