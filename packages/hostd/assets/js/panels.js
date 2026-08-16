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

// One-line run-level usage summary, derived from model-step records. Hidden
// when the run has no steps. Per-call detail lives on message cards.
export function renderRunStats(state) {
  const el = $("run-stats");
  const steps = (state.run?.records || []).filter((r) => r.type === "model_step");
  if (!steps.length) {
    el.classList.add("hidden");
    return;
  }
  const totals = steps.reduce(
    (acc, step) => {
      const usage = step.usage;
      if (!usage) return acc;
      acc.input += usage.input || 0;
      acc.cacheRead += usage.cacheRead || 0;
      acc.cacheWrite += usage.cacheWrite || 0;
      acc.output += usage.output || 0;
      const entry = usage.cost?.entries?.[0];
      if (entry) {
        acc.cost += Number(entry.total ?? 0);
        acc.currency ||= entry.currency || "";
      }
      return acc;
    },
    { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, cost: 0, currency: "" }
  );
  const hit = totals.input > 0 ? (totals.cacheRead / totals.input) * 100 : null;
  el.classList.remove("hidden");
  el.textContent =
    `${steps.length} model step${steps.length === 1 ? "" : "s"} · ` +
    `${fmtCount(totals.input)} input · ` +
    `${fmtCount(totals.cacheRead)} cached${hit === null ? "" : ` (${hit.toFixed(0)}%)`} · ` +
    `${fmtCount(totals.cacheWrite)} written · ` +
    `${fmtCount(totals.output)} output` +
    (totals.cost > 0 ? ` · ${totals.currency}${totals.cost.toFixed(4)}` : "");
}
