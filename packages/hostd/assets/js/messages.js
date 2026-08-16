// Chronological message list. Native vertical scroll; incremental append on
// live refresh so scroll position and expansion survive.

import { $, esc, fmtTs, ROLE_LABEL, textOfMessage, fmtCount, fmtDur, fmtCost, cacheRatio } from "./format.js";

export function createMessages({ onSelect }) {
  const box = $("messages");
  let rendered = 0;
  const expanded = new Set();
  let stepsByMessage = {};

  function indexSteps(state) {
    stepsByMessage = {};
    for (const record of state.run?.records || []) {
      if (record.type !== "model_step" || !record.messageId) continue;
      const existing = stepsByMessage[record.messageId];
      if (!existing || (record.finishedAt && !existing.finishedAt)) {
        stepsByMessage[record.messageId] = record;
      }
    }
  }

  function callStrip(step) {
    const details = document.createElement("details");
    details.className = "call-strip";
    const summary = document.createElement("summary");
    const u = step.usage;
    const ratio = cacheRatio(u);
    const parts = [];
    if (u) {
      parts.push(`in ${fmtCount(u.input)}`);
      parts.push(`cache ${fmtCount(u.cacheRead)}${ratio === null ? "" : ` (${(ratio * 100).toFixed(0)}%)`}`);
      parts.push(`write ${fmtCount(u.cacheWrite)}`);
      parts.push(`out ${fmtCount(u.output)}`);
      parts.push(`cost ${fmtCost(u)}`);
    } else {
      parts.push("no usage reported");
    }
    if (step.retries?.length) parts.push(`retries ${step.retries.length}`);
    if (step.fallback) parts.push("fallback");
    summary.textContent =
      `model call · ${step.provider}/${step.model} · ${fmtDur(step.startedAt, step.finishedAt)}`;
    const body = document.createElement("div");
    body.className = "call-strip-body";
    body.textContent = parts.join(" · ");
    summary.addEventListener("click", (event) => event.stopPropagation());
    details.appendChild(summary);
    details.appendChild(body);
    return details;
  }

  function buildCard(m, index) {
    const role = ROLE_LABEL[m.role] || "message";
    const text = textOfMessage(m);
    const card = document.createElement("div");
    card.className = `msg ${role}${expanded.has(index) ? " expanded" : ""}`;
    card.dataset.index = index;
    const preview = text.length > 160 ? text.slice(0, 160) + "…" : text;
    card.innerHTML = `<span class="role">${esc(role)}</span>` +
      `<span class="time">${fmtTs(m.timestamp)}</span>` +
      `<div class="preview">${esc(preview)}</div>` +
      `<div class="full">${esc(text)}</div>` +
      `<div class="hint">click to ${text.length > 160 ? "expand/collapse" : "select"}</div>`;
    const step = m.messageId ? stepsByMessage[m.messageId] : null;
    if (step) card.appendChild(callStrip(step));
    card.onclick = () => {
      card.classList.toggle("expanded");
      if (card.classList.contains("expanded")) expanded.add(index);
      else expanded.delete(index);
      onSelect(index);
    };
    return card;
  }

  function render(state) {
    box.innerHTML = "";
    expanded.clear();
    rendered = 0;
    indexSteps(state);
    append(state);
  }

  // Incremental: only append messages beyond what is already rendered.
  function append(state) {
    const messages = state.messages || [];
    indexSteps(state);
    if (!messages.length) {
      if (!rendered) box.innerHTML = '<p class="muted">no messages in this run</p>';
      return;
    }
    if (messages.length < rendered) {
      render(state);
      return;
    }
    if (box.querySelector("p.muted")) box.innerHTML = "";
    for (let i = rendered; i < messages.length; i++) {
      box.appendChild(buildCard(messages[i], i));
      rendered++;
    }
  }

  function highlight(index) {
    document.querySelectorAll("#messages .msg.selected").forEach((n) => n.classList.remove("selected"));
    const card = box.querySelector(`.msg[data-index="${index}"]`);
    if (card) {
      card.classList.add("selected");
      card.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  }

  return { render, append, highlight };
}
