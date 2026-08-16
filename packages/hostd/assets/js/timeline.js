// Canvas timeline component. One component owns structure, draw, hit-test,
// tooltip, ruler, and scroll redraw. No DOM nodes per brick; layout constants
// come from CSS custom properties via format.tokens().

import { $, fmtTs, tokens, TRACK_ORDER, ROLE_LABEL, textOfMessage } from "./format.js";

// Pure derivation: run detail -> global slot axis + per-track groups.
export function deriveTimeline(run) {
  const items = [];
  const messages = run.messages || [];
  let lastTime = 0;
  messages.forEach((m, index) => {
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
  const tracks = [...new Set(indexed.map((i) => i.kind))].sort((a, b) => {
    const ia = TRACK_ORDER.indexOf(a);
    const ib = TRACK_ORDER.indexOf(b);
    return (ia === -1 ? 99 : ia) - (ib === -1 ? 99 : ib);
  });
  const trackItems = tracks.map((kind) => indexed.filter((i) => i.kind === kind));
  return { timelineItems: indexed, tracks, trackItems };
}

export function createTimeline({ onSelectMessage }) {
  const container = $("timeline");
  let scrollEl = null;
  let canvas = null;
  let tooltip = null;
  let raf = 0;
  let current = null;

  // Build structure once per run; draw() is cheap and rerun on scroll/refresh.
  function render(state) {
    current = state;
    const prevScrollLeft = scrollEl?.scrollLeft || 0;
    container.innerHTML = "";
    scrollEl = null;
    canvas = null;
    tooltip = null;
    if (!state.timelineItems.length) {
      container.innerHTML = '<p class="muted" style="padding:12px">no messages in this run</p>';
      return;
    }
    const t = tokens();
    const viewWidth = container.clientWidth || 800;
    const contentWidth = Math.max(viewWidth - t.labelW, t.padX * 2 + state.timelineItems.length * t.slotW);

    // Left frozen column: ruler spacer + one label row per track.
    const labels = document.createElement("div");
    labels.id = "timeline-labels";
    const rulerSpacer = document.createElement("div");
    rulerSpacer.className = "timeline-ruler-spacer";
    labels.appendChild(rulerSpacer);
    for (const kind of state.tracks) {
      const row = document.createElement("div");
      row.className = "track-label";
      row.textContent = kind;
      labels.appendChild(row);
    }
    const bottomSpacer = document.createElement("div");
    bottomSpacer.className = "timeline-bottom-spacer";
    labels.appendChild(bottomSpacer);

    // Right column: pinned viewport-sized canvas + a spacer that provides the
    // native horizontal scrollbar and wheel range.
    const scroll = document.createElement("div");
    scroll.id = "timeline-scroll";
    const cv = document.createElement("canvas");
    cv.id = "timeline-canvas";
    const spacer = document.createElement("div");
    spacer.id = "timeline-spacer";
    spacer.style.width = `${contentWidth}px`;
    const tip = document.createElement("div");
    tip.id = "timeline-tooltip";
    scroll.appendChild(cv);
    scroll.appendChild(spacer);
    scroll.appendChild(tip);
    container.appendChild(labels);
    container.appendChild(scroll);

    const height = t.rulerH + state.tracks.length * t.trackH + t.padBottom;
    cv.style.width = `${scroll.clientWidth || 800}px`;
    cv.style.height = `${height}px`;

    scroll.scrollLeft = prevScrollLeft;
    scroll.addEventListener("scroll", scheduleDraw, { passive: true });
    cv.addEventListener("click", (e) => {
      const hit = hitTest(e);
      if (hit && hit.ref.kind === "message") onSelectMessage(hit.ref.index);
    });
    cv.addEventListener("mousemove", (e) => showTooltip(e));
    cv.addEventListener("mouseleave", () => {
      if (tooltip) tooltip.style.display = "none";
    });
    scrollEl = scroll;
    canvas = cv;
    tooltip = tip;
    draw();
  }

  // Redraw only (live refresh, selection change).
  function update(state) {
    current = state;
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
