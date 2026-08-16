// Prompt assembly view: frozen semantic blocks, cache plan bar, tool catalog,
// per-model-step provider cache usage, and raw JSON. Pure derivation plus DOM
// rendering; native vertical scroll, no JS on scroll.

import { $, esc, short, fmtCount, copyText } from "./format.js";

export const KIND_LABEL = {
  instruction: "Instruction",
  context: "Context",
  catalog: "Catalog",
  environment: "Environment",
};
export const AUTHORITY_LABEL = {
  platform: "Platform",
  operator: "Operator",
  agent: "Agent",
  project: "Project",
  user: "User",
  none: "None",
};
export const TRUST_LABEL = {
  trusted: "Trusted",
  workspaceControlled: "Workspace",
  untrusted: "Untrusted",
};
export const SCOPE_LABEL = {
  globalStable: "GlobalStable",
  operatorStable: "OperatorStable",
  agentStable: "AgentStable",
  catalogStable: "CatalogStable",
  resourceSnapshot: "ResourceSnapshot",
  runDynamic: "RunDynamic",
  noCache: "NoCache",
};

const kebab = (s) => String(s).replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

// Pure derivation: run detail -> structured prompt view state.
export function derivePrompt(run) {
  const assembly = run?.assembly;
  if (!assembly) return null;
  const blocks = assembly.prompt?.blocks || [];
  const totalChars = blocks.reduce((n, b) => n + (b.content?.length || 0), 0);
  const segments = (assembly.prompt?.cachePlan?.prefixSegments || []).map((seg) => {
    const segBlocks = blocks.filter((b) => (seg.blockDigests || []).includes(b.contentDigest));
    const chars = segBlocks.reduce((n, b) => n + (b.content?.length || 0), 0);
    return { ...seg, blocks: segBlocks, chars };
  });
  const tools = assembly.toolCatalog?.tools || [];
  const sources = assembly.toolCatalog?.sources || [];
  return {
    assembly,
    blocks,
    totalChars,
    segments,
    tools,
    sources,
    raw: JSON.stringify(assembly, null, 2),
  };
}

export function createPrompt() {
  const view = $("prompt-view");
  const ui = { filter: {}, expanded: new Set(), highlightScope: null };
  let current = null;

  function matches(block) {
    for (const [key, value] of Object.entries(ui.filter)) {
      if (value && block[key] !== value) return false;
    }
    return true;
  }

  function chipGroup(label, key, blocks) {
    const counts = {};
    for (const b of blocks) counts[b[key]] = (counts[b[key]] || 0) + 1;
    const row = document.createElement("div");
    row.className = "chips";
    const addChip = (value, text) => {
      const chip = document.createElement("button");
      chip.className = "chip" + (ui.filter[key] === value ? " active" : "");
      chip.textContent = `${text} (${counts[value] || 0})`;
      chip.onclick = () => {
        ui.filter[key] = ui.filter[key] === value ? null : value;
        render();
      };
      row.appendChild(chip);
    };
    for (const value of Object.keys(counts)) addChip(value, value);
    const labelEl = document.createElement("span");
    labelEl.className = "muted small";
    labelEl.textContent = label;
    row.prepend(labelEl);
    return row;
  }

  function header(d) {
    const a = d.assembly;
    const sec = document.createElement("section");
    sec.className = "prompt-section";
    const digests = [
      ["source", a.promptDigest],
      ["semantic prefix", a.prompt?.cachePlan?.semanticPrefixDigest],
      ["catalog", a.toolCatalog?.digest],
    ].filter(([, v]) => v);
    const head = document.createElement("div");
    head.className = "prompt-head-row";
    head.innerHTML =
      `<strong>prompt assembly</strong>` +
      `<span class="muted">v${esc(a.assemblyVersion)}</span>` +
      `<span class="muted">${fmtCount(d.totalChars)} chars · ${d.blocks.length} blocks · ${d.tools.length} tools</span>` +
      `<span class="muted">cache: ${esc(a.prompt?.cachePlan?.policy || "providerDefault")}</span>`;
    sec.appendChild(head);
    const digestRow = document.createElement("div");
    digestRow.className = "digest-row";
    for (const [label, value] of digests) {
      const chip = document.createElement("span");
      chip.className = "digest-chip";
      chip.innerHTML = `${esc(label)} <code>${esc(short(value))}</code>`;
      const copy = document.createElement("button");
      copy.className = "ghost";
      copy.textContent = "copy";
      copy.dataset.copy = value;
      chip.appendChild(copy);
      digestRow.appendChild(chip);
    }
    sec.appendChild(digestRow);
    const note = document.createElement("p");
    note.className = "muted note";
    note.textContent =
      "Blocks are frozen at run start; transcript context/user messages are injected separately and are not part of the frozen prompt.";
    sec.appendChild(note);
    return sec;
  }

  function cacheBar(d) {
    const sec = document.createElement("section");
    sec.className = "prompt-section";
    const title = document.createElement("h4");
    title.textContent = "cache plan";
    sec.appendChild(title);
    const bar = document.createElement("div");
    bar.className = "cache-bar";
    if (!d.segments.length) {
      bar.innerHTML = '<span class="muted">no stable prefix segments</span>';
    } else {
      for (const seg of d.segments) {
        const el = document.createElement("div");
        el.className = `cache-seg scope-${kebab(seg.scope)}` +
          (ui.highlightScope === seg.scope ? " active" : "");
        el.style.width = `${Math.max(1, (seg.chars / Math.max(1, d.totalChars)) * 100)}%`;
        el.title = `${SCOPE_LABEL[seg.scope] || seg.scope} · ${fmtCount(seg.chars)} chars · ${seg.segmentDigest}`;
        el.onclick = () => {
          ui.highlightScope = ui.highlightScope === seg.scope ? null : seg.scope;
          render();
          if (ui.highlightScope) {
            const first = view.querySelector(".block-card.highlighted");
            first?.scrollIntoView({ behavior: "smooth", block: "nearest" });
          }
        };
        bar.appendChild(el);
      }
    }
    sec.appendChild(bar);
    const legend = document.createElement("div");
    legend.className = "cache-legend";
    legend.innerHTML = d.segments
      .map((s) => `<span class="legend-item"><i class="dot scope-${kebab(s.scope)}"></i>${SCOPE_LABEL[s.scope] || s.scope}</span>`)
      .join("");
    sec.appendChild(legend);
    const copy = document.createElement("button");
    copy.className = "ghost";
    copy.textContent = "copy segment digests";
    copy.dataset.copy = d.segments.map((s) => `${s.scope}:${s.segmentDigest}`).join("\n");
    sec.appendChild(copy);
    return sec;
  }

  function blockCard(block) {
    const card = document.createElement("div");
    card.className = "block-card" + (ui.expanded.has(block.id) ? " expanded" : "");
    if (ui.highlightScope && block.cacheScope === ui.highlightScope) {
      card.classList.add("highlighted");
    }
    const source = block.source || {};
    const version = source.version ? `@${source.version}` : "";
    const meta = document.createElement("div");
    meta.className = "block-meta";
    meta.textContent =
      `${source.kind}:${source.locator}${version} · ${fmtCount(block.content?.length || 0)} chars · ${short(block.contentDigest)}`;
    const head = document.createElement("div");
    head.className = "block-head";
    head.innerHTML =
      `<span class="badge kind">${KIND_LABEL[block.kind] || block.kind}</span>` +
      `<span class="badge auth">${AUTHORITY_LABEL[block.authority] || block.authority}</span>` +
      `<span class="badge trust">${TRUST_LABEL[block.trust] || block.trust}</span>` +
      `<span class="badge scope scope-${kebab(block.cacheScope)}">${SCOPE_LABEL[block.cacheScope] || block.cacheScope}</span>`;
    const body = document.createElement("div");
    body.className = "block-body";
    const content = document.createElement("div");
    content.className = "block-content";
    content.textContent = block.content || "";
    const copy = document.createElement("button");
    copy.className = "ghost";
    copy.textContent = "copy";
    copy.dataset.copy = block.content || "";
    body.appendChild(content);
    body.appendChild(copy);
    card.appendChild(head);
    card.appendChild(meta);
    card.appendChild(body);
    card.onclick = (event) => {
      if (event.target.dataset.copy) return;
      if (ui.expanded.has(block.id)) ui.expanded.delete(block.id);
      else ui.expanded.add(block.id);
      card.classList.toggle("expanded");
    };
    return card;
  }

  function blockList(d) {
    const sec = document.createElement("section");
    sec.className = "prompt-section";
    const title = document.createElement("h4");
    title.innerHTML = `blocks <span class="muted">(${d.blocks.length})</span>`;
    sec.appendChild(title);
    sec.appendChild(chipGroup("kind", "kind", d.blocks));
    sec.appendChild(chipGroup("authority", "authority", d.blocks));
    sec.appendChild(chipGroup("trust", "trust", d.blocks));
    const list = document.createElement("div");
    for (const block of d.blocks.filter(matches)) list.appendChild(blockCard(block));
    if (!list.children.length) {
      list.innerHTML = '<p class="muted">no blocks match the filter</p>';
    }
    sec.appendChild(list);
    return sec;
  }

  function toolCatalog(d) {
    const sec = document.createElement("section");
    sec.className = "prompt-section";
    const title = document.createElement("h4");
    title.innerHTML = `tool catalog <span class="muted">(${d.tools.length})</span>`;
    sec.appendChild(title);
    const sources = document.createElement("div");
    sources.className = "muted small";
    sources.textContent = d.sources.length
      ? `contributors: ${d.sources.map((s) => `${s.kind}:${s.locator}`).join(", ")}`
      : "no contributor metadata";
    sec.appendChild(sources);
    for (const tool of d.tools) {
      const det = document.createElement("details");
      det.className = "tool-card";
      const provenance = tool.provenance || {};
      const summary = document.createElement("summary");
      summary.innerHTML =
        `<strong>${esc(tool.name)}</strong> <span class="muted">${esc(tool.version)} · ${esc(provenance.kind)}:${esc(provenance.locator)}</span>`;
      det.appendChild(summary);
      if (tool.description) {
        const desc = document.createElement("p");
        desc.textContent = tool.description;
        det.appendChild(desc);
      }
      const schema = document.createElement("pre");
      schema.textContent = JSON.stringify(tool.inputSchema ?? {}, null, 2);
      det.appendChild(schema);
      sec.appendChild(det);
    }
    return sec;
  }

  function rawJson(d) {
    const sec = document.createElement("section");
    sec.className = "prompt-section";
    const det = document.createElement("details");
    det.className = "raw-json";
    const summary = document.createElement("summary");
    summary.textContent = "raw JSON";
    const pre = document.createElement("pre");
    pre.textContent = d.raw;
    det.appendChild(summary);
    det.appendChild(pre);
    sec.appendChild(det);
    const copy = document.createElement("button");
    copy.className = "ghost";
    copy.textContent = "copy";
    copy.dataset.copy = d.raw;
    sec.appendChild(copy);
    return sec;
  }

  function render() {
    const state = current;
    if (!state) return;
    const top = view.scrollTop;
    const derived = derivePrompt(state.run);
    view.innerHTML = "";
    if (!derived) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = "no assembly recorded for this run";
      view.appendChild(empty);
      return;
    }
    view.appendChild(header(derived));
    view.appendChild(cacheBar(derived));
    view.appendChild(blockList(derived));
    view.appendChild(toolCatalog(derived));
    view.appendChild(rawJson(derived));
    view.scrollTop = top;
  }

  view.addEventListener("click", (event) => {
    const target = event.target.closest("[data-copy]");
    if (target) copyText(target.dataset.copy);
  });

  return {
    render(state) {
      current = state;
      render();
    },
    update(state) {
      current = state;
      if (!view.classList.contains("hidden")) render();
    },
  };
}
