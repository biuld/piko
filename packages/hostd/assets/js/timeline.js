// Canvas timeline component. One component owns structure, draw, hit-test,
// tooltip, ruler, and scroll redraw. No DOM nodes per brick; layout constants
// come from CSS custom properties via format.tokens().

import { $, fmtTs, tokens, ROLE_LABEL, terminalLabel, textOfMessage } from "./format.js";

// Pure derivation: run detail + display message stream -> global slot axis +
// per-track groups. `messageItems` is the store's display list (assembly card
// + messages); all message/step refs index into it so selection never drifts.
export function deriveTimeline(run, messageItems) {
  const items = [];
  const messages = messageItems || run?.messages || [];
  if (run.assembly) {
    const assemblyIndex = messages.findIndex((m) => m && m.role === "assembly");
    items.push({
      id: "prompt",
      kind: "prompt",
      label: "prompt assembled",
      time: run.assembly.recordedAt || 0,
      ref: { kind: "message", index: assemblyIndex >= 0 ? assemblyIndex : 0 },
    });
  }
  let lastTime = 0;
  messages.forEach((m, index) => {
    if (m.role === "assembly") return;
    // Old journals may lack timestamps on ToolResult; fall back to the
    // previous message's time so they stay in journal order.
    const time = m.timestamp || lastTime;
    if (m.timestamp) lastTime = m.timestamp;
    items.push({
      id: `m${index}`,
      kind: ROLE_LABEL[m.role] || "message",
      label: textOfMessage(m),
      time,
      ref: { kind: "message", index },
    });
  });
  // Model steps emit a start and a finish record; keep one brick per step and
  // prefer the finish record so usage/duration/timing are available.
  const steps = new Map();
  for (const record of run.records || []) {
    if (record.type !== "model_step") continue;
    const key = record.messageId || record.stepId;
    const existing = steps.get(key);
    if (!existing || (record.finishedAt && !existing.finishedAt)) steps.set(key, record);
  }
  for (const record of steps.values()) {
    const messageIndex = record.messageId
      ? messages.findIndex((m) => m.messageId === record.messageId)
      : -1;
    items.push({
      id: `s${record.stepId}`,
      kind: "step",
      label: `${record.provider || ""}/${record.model || "model"}`,
      time: record.startedAt || 0,
      ref: { kind: "step", index: messageIndex >= 0 ? messageIndex : null },
    });
  }
  for (const record of run.records || []) {
    if (record.type === "system_notification") {
      items.push({
        id: `n${items.length}`,
        kind: "system",
        label: `${record.kind}: ${record.summary}`,
        time: record.recordedAt || 0,
        ref: { kind: "record", index: run.records.indexOf(record) },
      });
    }
  }
  // Terminal record: the running → completed/failed/cancelled transition is
  // pushed live via SSE, so the brick appears as soon as the run finishes.
  for (const record of run.records || []) {
    if (record.type === "terminal") {
      items.push({
        id: `t${items.length}`,
        kind: "terminal",
        label: terminalLabel(record.kind) + (record.reason ? `: ${record.reason}` : ""),
        time: record.finishedAt || 0,
        ref: { kind: "record", index: run.records.indexOf(record) },
      });
    }
  }
  const indexed = items.map((item, seq) => ({ ...item, seq }));
  indexed.sort((a, b) => (a.time || 0) - (b.time || 0) || a.seq - b.seq);
  const sameInstant = new Map(); // `${kind}:${time}` -> count seen
  indexed.forEach((item, slot) => {
    const key = `${item.kind}:${item.time || 0}`;
    const seen = sameInstant.get(key) || 0;
    sameInstant.set(key, seen + 1);
    item.slot = slot;
    item.nudge = seen * 3; // tiny in-track offset for exact same-ms commits
  });
  // Tracks follow first-appearance order: `indexed` is time-sorted, so each
  // new kind lands below the kinds that appeared before it — a track that
  // first shows up mid-stream naturally appends at the bottom.
  const tracks = [...new Set(indexed.map((i) => i.kind))];
  const trackItems = tracks.map((kind) => indexed.filter((i) => i.kind === kind));
  return { timelineItems: indexed, tracks, trackItems };
}

export function createTimeline({ onSelectMessage }) {
  const container = $("timeline");
  let scrollEl = null;
  let canvas = null;
  let tooltip = null;
  let labels = null;
  let spacerEl = null;
  let raf = 0;
  let current = null;
  let layoutKey = "";

  // Recompute content geometry (spacer width, canvas height, track labels)
  // and apply follow-scroll. `pin` true forces the right edge onto the
  // newest activity (used on run selection and when the user is already at
  // the end); without it the current scroll position is preserved.
  function layout(state, { pin = null } = {}) {
    if (!scrollEl || !canvas || !labels || !spacerEl) return;
    const t = tokens();
    const atEnd = scrollEl.scrollLeft + scrollEl.clientWidth >= scrollEl.scrollWidth - 2;
    const shouldPin = pin != null ? pin : atEnd;
    const viewWidth = container.clientWidth || 800;
    const contentWidth = Math.max(
      viewWidth - t.labelW,
      t.padX * 2 + state.timelineItems.length * t.slotW
    );
    const height = t.rulerH + state.tracks.length * t.trackH + t.padBottom;
    // Track labels grow/change with the track set (live refresh can add a
    // track — e.g. the first tool call — mid-stream). Insert in order before
    // the bottom spacer; the ruler spacer stays on top.
    labels.querySelectorAll(".track-label").forEach((row) => row.remove());
    const bottomSpacer = labels.querySelector(".timeline-bottom-spacer");
    for (const kind of state.tracks) {
      const row = document.createElement("div");
      row.className = "track-label";
      row.textContent = kind === "step" ? "model step" : kind;
      labels.insertBefore(row, bottomSpacer);
    }
    spacerEl.style.width = `${contentWidth}px`;
    canvas.style.width = `${scrollEl.clientWidth || 800}px`;
    canvas.style.height = `${height}px`;
    if (shouldPin) {
      scrollEl.scrollLeft = scrollEl.scrollWidth;
    }
  }

  // Build structure once per run; draw() is cheap and rerun on scroll/refresh.
  function render(state) {
    current = state;
    container.innerHTML = "";
    scrollEl = null;
    canvas = null;
    tooltip = null;
    labels = null;
    spacerEl = null;
    layoutKey = "";
    if (!state.timelineItems.length) {
      container.innerHTML = '<p class="muted" style="padding:12px">no messages in this run</p>';
      return;
    }

    // Left frozen column: ruler spacer + one label row per track.
    const labelsEl = document.createElement("div");
    labelsEl.id = "timeline-labels";
    const rulerSpacer = document.createElement("div");
    rulerSpacer.className = "timeline-ruler-spacer";
    labelsEl.appendChild(rulerSpacer);
    const bottomSpacer = document.createElement("div");
    bottomSpacer.className = "timeline-bottom-spacer";
    labelsEl.appendChild(bottomSpacer);

    // Right column: pinned viewport-sized canvas + a spacer that provides the
    // native horizontal scrollbar and wheel range.
    const scroll = document.createElement("div");
    scroll.id = "timeline-scroll";
    const cv = document.createElement("canvas");
    cv.id = "timeline-canvas";
    const spacer = document.createElement("div");
    spacer.id = "timeline-spacer";
    const tip = document.createElement("div");
    tip.id = "timeline-tooltip";
    scroll.appendChild(cv);
    scroll.appendChild(spacer);
    scroll.appendChild(tip);
    container.appendChild(labelsEl);
    container.appendChild(scroll);

    scroll.addEventListener("scroll", scheduleDraw, { passive: true });
    cv.addEventListener("click", (e) => {
      const hit = hitTest(e);
      if (hit && hit.ref.kind === "message") onSelectMessage(hit.ref.index);
      else if (hit && hit.ref.kind === "step" && hit.ref.index != null) {
        onSelectMessage(hit.ref.index);
      }
    });
    cv.addEventListener("mousemove", (e) => showTooltip(e));
    cv.addEventListener("mouseleave", () => {
      if (tooltip) tooltip.style.display = "none";
    });
    scrollEl = scroll;
    canvas = cv;
    tooltip = tip;
    labels = labelsEl;
    spacerEl = spacer;
    // Land on the newest activity; a live run then follows as records stream
    // in.
    layout(state, { pin: true });
    draw();
  }

  // Redraw only (live refresh, selection change); relayout when the content
  // structure grew (new items or new tracks) so the scroll range and track
  // labels keep up. The right edge follows the newest activity only when the
  // user was already pinned there.
  function update(state) {
    current = state;
    const key = `${state.timelineItems.length}|${state.tracks.join(",")}`;
    if (key !== layoutKey) {
      layout(state);
      layoutKey = key;
    }
    draw();
  }

  function scheduleDraw() {
    if (raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      draw();
    });
  }

  function draw() {
    if (!canvas || !scrollEl || !current) return;
    const t = tokens();
    const w = canvas.clientWidth || 800;
    const h = canvas.clientHeight || 0;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }
    const ctx = canvas.getContext("2d");
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const left = scrollEl.scrollLeft;
    const { timelineItems, tracks, trackItems, selectedMessage } = current;

    // Ruler band: ticks + timestamps at content positions (scrolls with the
    // content), density-adaptive stride.
    ctx.fillStyle = "rgba(128,128,128,0.08)";
    ctx.fillRect(0, 0, w, t.rulerH);
    ctx.strokeStyle = "rgba(128,128,128,0.25)";
    ctx.beginPath();
    ctx.moveTo(0, t.rulerH - 0.5);
    ctx.lineTo(w, t.rulerH - 0.5);
    ctx.stroke();
    ctx.font = "11px ui-monospace, SFMono-Regular, Menlo, monospace";
    ctx.textBaseline = "middle";
    const stride = Math.max(1, Math.ceil((t.padX * 2 + timelineItems.length * t.slotW) / 900));
    let lastLabelX = -999;
    for (let i = 0; i < timelineItems.length; i += stride) {
      const item = timelineItems[i];
      const x = t.padX + item.slot * t.slotW - left;
      if (x < -80 || x > w + 80) continue;
      ctx.strokeStyle = "rgba(128,128,128,0.35)";
      ctx.beginPath();
      ctx.moveTo(x, t.rulerH - 5);
      ctx.lineTo(x, t.rulerH);
      ctx.stroke();
      if (x - lastLabelX >= 70) {
        ctx.fillStyle = "#888";
        ctx.fillText(fmtTs(item.time), x + 3, t.rulerH / 2);
        lastLabelX = x;
      }
    }

    // Bricks: only the visible window is rasterized per frame.
    trackItems.forEach((group, ti) => {
      if (!group.length) return;
      const kind = tracks[ti];
      const y = t.rulerH + ti * t.trackH;
      ctx.fillStyle = t.roleColors[kind] || "rgba(128,128,128,0.5)";
      for (const item of group) {
        const x = t.padX + item.slot * t.slotW + item.nudge - left;
        if (x < -t.brickW || x > w) continue;
        ctx.beginPath();
        if (ctx.roundRect) ctx.roundRect(x, y + 8, t.brickW, 18, t.radius);
        else ctx.rect(x, y + 8, t.brickW, 18);
        ctx.fill();
        if (item.ref.kind === "message" && item.ref.index === selectedMessage) {
          ctx.strokeStyle = "#fff";
          ctx.lineWidth = 2;
          ctx.stroke();
        }
      }
    });
  }

  function hitTest(e) {
    if (!scrollEl || !current) return null;
    const t = tokens();
    const x = e.offsetX + scrollEl.scrollLeft - t.padX;
    const ti = Math.floor((e.offsetY - t.rulerH) / t.trackH);
    if (ti < 0 || ti >= current.trackItems.length) return null;
    for (const item of current.trackItems[ti]) {
      const ix = item.slot * t.slotW + item.nudge;
      if (Math.abs(x - ix) <= t.brickW / 2 + 2) return item;
    }
    return null;
  }

  function showTooltip(e) {
    if (!tooltip) return;
    const hit = hitTest(e);
    if (!hit) {
      tooltip.style.display = "none";
      return;
    }
    tooltip.textContent = `${fmtTs(hit.time)} — ${hit.label}`;
    tooltip.style.display = "block";
    tooltip.style.left = `${e.offsetX + 14}px`;
    tooltip.style.top = `${e.offsetY + 14}px`;
  }

  return { render, update };
}
