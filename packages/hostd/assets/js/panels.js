// Session list and run strip renderers (static DOM lists).

import { $, esc, short, fmtTs, fmtDur, fmtCount, terminalBadge, agentShort } from "./format.js";

export function renderSessions(state, actions) {
  $("session-count").textContent = `(${state.sessions.length})`;
  const box = $("sessions");
  box.innerHTML = "";
  for (const id of state.sessions) {
    const el = document.createElement("div");
    el.className = "session" + (id === state.selectedSession ? " active" : "");
    el.innerHTML = `<div>${esc(id)}</div>`;
    el.onclick = () => actions.selectSession(id);
    box.appendChild(el);
  }
}

export function renderRunStrip(state, actions) {
  const strip = $("runs-strip");
  strip.innerHTML = "";
  if (!state.runs.length) {
    strip.innerHTML = '<span class="muted">no runs recorded</span>';
    return;
  }
  for (const run of state.runs) {
    const el = document.createElement("div");
    el.className = "run-chip" + (run.runId === state.selectedRun ? " active" : "");
    const parts = [
      fmtTs(run.startedAt),
      fmtDur(run.startedAt, run.finishedAt),
      `${run.messageCount ?? 0} msgs`,
      `${run.toolCallCount} tools`,
    ];
    if (run.childRunCount > 0) parts.push(`${run.childRunCount} children`);
    if (run.stepCount > 0) parts.push(`${run.stepCount} steps`);
    if (run.usage?.cacheHitRatio != null) {
      parts.push(`${(run.usage.cacheHitRatio * 100).toFixed(0)}% cached`);
    }
    if (run.droppedRecords > 0) parts.push(`<span class="dropped">${run.droppedRecords} dropped</span>`);
    const agent = agentShort(run.agentInstanceId, state.selectedSession);
    el.innerHTML = `<div>${terminalBadge(run.terminal)} <span class="muted">${esc(short(run.runId))}</span>` +
      (agent ? ` <span class="muted">· ${esc(agent)}</span>` : "") + `</div>` +
      `<div class="muted">${parts.join(" · ")}</div>`;
    el.title = run.runId;
    el.onclick = () => actions.selectRun(run.runId);
    strip.appendChild(el);
  }
}

// One-line run-level usage summary. Hostd computes the rollup
// (`TrajectoryRunSummary.usage`); the viewer only formats it. Hidden when the
// run has no usage. Per-call detail lives on message cards.
export function renderRunStats(state) {
  const el = $("run-stats");
  const summary = state.run?.summary;
  const usage = summary?.usage;
  if (!usage) {
    el.classList.add("hidden");
    return;
  }
  const steps = summary.stepCount ?? 0;
  const hit = usage.cacheHitRatio == null ? null : usage.cacheHitRatio * 100;
  const cost = usage.cost?.[0];
  el.classList.remove("hidden");
  el.textContent =
    `${steps} model step${steps === 1 ? "" : "s"} · ` +
    `${fmtCount(usage.input)} input · ` +
    `${fmtCount(usage.cacheRead)} cached${hit === null ? "" : ` (${hit.toFixed(0)}%)`} · ` +
    `${fmtCount(usage.cacheWrite)} written · ` +
    `${fmtCount(usage.output)} output` +
    (cost && cost.total > 0 ? ` · ${cost.currency}${cost.total.toFixed(4)}` : "");
}
