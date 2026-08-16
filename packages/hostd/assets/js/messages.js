// Chronological message list. Native vertical scroll; incremental append on
// live refresh so scroll position and expansion survive.

import { $, esc, fmtTs, ROLE_LABEL, textOfMessage } from "./format.js";

export function createMessages({ onSelect }) {
  const box = $("messages");
  let rendered = 0;
  const expanded = new Set();

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
    append(state);
  }

  // Incremental: only append messages beyond what is already rendered.
  function append(state) {
    const messages = state.messages || [];
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
