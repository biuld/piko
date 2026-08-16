// Shared helpers and design tokens (single source: CSS custom properties).

export const $ = (id) => document.getElementById(id);
export const SESSION_KEY = "piko.trajectory.session";
export const RUN_KEY = "piko.trajectory.run";
export const ROLE_LABEL = {
  prompt: "prompt",
  step: "model step",
  user: "user",
  assistant: "assistant",
  toolCall: "toolCall",
  toolResult: "toolResult",
  context: "context",
  system: "system",
  terminal: "terminal",
};
export const TRACK_ORDER = ["prompt", "step", "context", "user", "assistant", "toolCall", "toolResult", "system", "terminal"];

let tokenCache = null;
export function tokens() {
  if (tokenCache) return tokenCache;
  const cs = getComputedStyle(document.documentElement);
  const num = (name, fallback) => {
    const v = parseFloat(cs.getPropertyValue(name));
    return Number.isFinite(v) ? v : fallback;
  };
  const alpha = { toolCall: 0.6, system: 0.65 };
  const names = ["prompt", "step", "context", "user", "assistant", "toolCall", "toolResult", "system", "terminal"];
  const roleColors = {};
  for (const name of names) {
    const hex = cs.getPropertyValue(`--role-${name}`).trim();
    const m = /^#?([0-9a-f]{6})$/i.exec(hex);
    if (!m) {
      roleColors[name] = "rgba(128,128,128,0.5)";
      continue;
    }
    const n = parseInt(m[1], 16);
    roleColors[name] = `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha[name] ?? 0.55})`;
  }
  tokenCache = {
    roleColors,
    labelW: num("--label-w", 92),
    trackH: num("--track-h", 34),
    rulerH: num("--ruler-h", 18),
    slotW: num("--slot-w", 24),
    brickW: num("--brick-w", 22),
    padX: num("--pad-x", 8),
    padBottom: num("--pad-bottom", 12),
    radius: num("--radius", 4),
  };
  return tokenCache;
}

export function esc(text) {
  return String(text).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  }[c]));
}

export function short(id) {
  return String(id || "").slice(0, 12);
}

export function fmtTs(ms) {
  if (!ms) return "—";
  return new Date(ms).toLocaleTimeString([], { hour12: false });
}

export function terminalLabel(t) {
  return t === "completed" ? "completed" : t === "failed" ? "failed" : t === "cancelled" ? "cancelled" : "running";
}

export function terminalBadge(t) {
  const kind = terminalLabel(t);
  return `<span class="badge ${kind}">${kind}</span>`;
}

export function fmtDur(start, end) {
  if (!start || !end || end <= start) return "—";
  const total = Math.round((end - start) / 1000);
  if (total < 60) return `${total}s`;
  const m = Math.floor(total / 60);
  const s = total % 60;
  return s ? `${m}m${s}s` : `${m}m`;
}

export function fmtCount(n) {
  const value = Number(n) || 0;
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}m`;
  if (value >= 10_000) return `${(value / 1000).toFixed(0)}k`;
  if (value >= 1_000) return `${(value / 1000).toFixed(1)}k`;
  return String(value);
}

export async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Clipboard may be unavailable in restricted contexts; the button is a
    // convenience, not a contract.
  }
}

export function fmtCost(usage) {
  const entries = usage?.cost?.entries || [];
  if (!entries.length) return "—";
  const entry = entries[0];
  return `${entry.currency || ""}${Number(entry.total ?? 0).toFixed(4)}`;
}

export function cacheRatio(usage) {
  if (!usage || !usage.input) return null;
  return (usage.cacheRead || 0) / usage.input;
}

export function agentShort(id, sessionId) {
  if (id === `agent_${sessionId}_root`) return "";
  return String(id || "").replace(/^agent_/, "").slice(0, 8);
}

export function textOfMessage(m) {
  const blocks = (arr) => (arr || []).map((b) => {
    if (b.type === "text") return b.text;
    if (b.type === "thinking") return "[thinking]";
    return "";
  }).filter(Boolean).join(" ");
  if (m.role === "assistant") return blocks(m.content);
  if (m.role === "user") return typeof m.content === "string" ? m.content : blocks(m.content);
  if (m.role === "toolCall") return `${m.name}(${JSON.stringify(m.arguments ?? {})})`;
  if (m.role === "toolResult") return blocks(m.content) || (m.isError ? "error" : "[result]");
  if (m.role === "context") return typeof m.content === "string" ? m.content : blocks(m.content);
  return JSON.stringify(m).slice(0, 200);
}
