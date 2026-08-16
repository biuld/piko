// Session list and run strip renderers (static DOM lists).

import { $, esc, short, fmtTs, fmtDur, terminalBadge, agentShort } from "./format.js";

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
