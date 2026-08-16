// Chronological message list. Native vertical scroll; incremental append on
// live refresh so scroll position and expansion survive.

import { $, esc, fmtTs, ROLE_LABEL, textOfMessage, fmtCount, fmtDur, fmtCost, cacheRatio } from "./format.js";
import { derivePrompt, assemblySummary, createPrompt } from "./prompt.js";

// Pure derivation: the run's display stream = assembly card (if recorded) +
// committed messages, ordered by timestamp (stable; assembly precedes the
// run's input commit). Every index downstream refers to this list.
export function deriveMessageItems(run) {
  const messages = run?.messages || [];
  const items = [];
  if (run?.assembly) {
    items.push({
      role: "assembly",
      timestamp: run.assembly.recordedAt || 0,
      assembly: run.assembly,
    });
  }
  items.push(...messages);
  const withSeq = items.map((m, i) => ({ m, i }));
  withSeq.sort((a, b) => (a.m.timestamp || 0) - (b.m.timestamp || 0) || a.i - b.i);
  return withSeq.map((x) => x.m);
}

export function createMessages({ onSelect }) {
  const box = $("messages");
  let rendered = 0;
  const expanded = new Set();
  const assemblyViews = new Map();
  let assemblySeen = false;
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

  function buildAssemblyCard(m, index) {
    const derived = derivePrompt({ assembly: m.assembly });
    const card = document.createElement("div");
    card.className = `msg assembly${expanded.has(index) ? " expanded" : ""}`;
    card.dataset.index = index;
    card.innerHTML =
      `<span class="role">prompt assembly</span>` +
      `<span class="time">${fmtTs(m.timestamp)}</span>` +
      `<div class="preview">${esc(assemblySummary(derived))}</div>` +
      `<div class="full prompt-body"></div>` +
      `<div class="hint">click to expand/select</div>`;
    const full = card.querySelector(".full");
    // Nested prompt controls (block cards, chips, copy, tool entries) manage
    // their own clicks; they must not collapse the assembly card.
    full.addEventListener("click", (event) => event.stopPropagation());
    card.onclick = () => {
      const isExpanded = card.classList.toggle("expanded");
      if (isExpanded) expanded.add(index);
      else expanded.delete(index);
      if (isExpanded) {
        let view = assemblyViews.get(index);
        if (!view) {
          view = createPrompt(full);
          assemblyViews.set(index, view);
        }
        view.render({ run: { assembly: m.assembly } });
      }
      onSelect(index);
    };
    return card;
  }

  function buildCard(m, index) {
    if (m.role === "assembly") return buildAssemblyCard(m, index);
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
    assemblyViews.clear();
    rendered = 0;
    indexSteps(state);
    assemblySeen = (state.messages?.[0]?.role) === "assembly";
    append(state);
  }

  // Incremental: only append messages beyond what is already rendered.
  function append(state) {
    const messages = state.messages || [];
    indexSteps(state);
    const hasAssembly = messages[0]?.role === "assembly";
    if (hasAssembly !== assemblySeen) {
      // Assembly appearance/disappearance is a head-of-list change, not a
      // tail append; rebuild the card list once (D-50 invariant 4).
      render(state);
      return;
    }
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
