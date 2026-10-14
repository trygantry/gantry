import { state } from '../state.js';
import { updateBlockerField } from '../api.js';

// ── Column id map ─────────────────────────────────────────────────────────────
const COL_ID = {
  "open":        "col-open",
  "in_progress": "col-progress",
  "blocked":     "col-blocked",
  "resolved":    "col-resolved"
};

function getCol(status) {
  return document.getElementById(COL_ID[status] || "col-open");
}

function colKeyFromEl(el) {
  const colEl = el?.closest?.(".kanban-column");
  if (!colEl) return null;
  return Object.entries(COL_ID).find(([, id]) => id === colEl.id)?.[0] ?? null;
}

// ── Pointer-based drag state (works inside Tauri / WKWebView) ────────────────
const drag = {
  active:      false,
  cardEl:      null,
  blocker:     null,
  clone:       null,
  fromColKey:  null,
  overColKey:  null,
  offsetX:     0,
  offsetY:     0,
};

function onMouseMove(e) {
  if (!drag.active) return;

  // Move clone with cursor
  drag.clone.style.left = `${e.clientX - drag.offsetX}px`;
  drag.clone.style.top  = `${e.clientY - drag.offsetY}px`;

  // Detect column under cursor (hide clone briefly so elementFromPoint sees through it)
  drag.clone.style.visibility = "hidden";
  const below = document.elementFromPoint(e.clientX, e.clientY);
  drag.clone.style.visibility = "";

  const newKey = colKeyFromEl(below);
  if (newKey !== drag.overColKey) {
    document.querySelectorAll(".kanban-column.drag-over")
      .forEach(c => c.classList.remove("drag-over"));
    if (newKey) document.getElementById(COL_ID[newKey])?.classList.add("drag-over");
    drag.overColKey = newKey;
  }
}

function onMouseUp(e) {
  if (!drag.active) return;
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("mouseup",   onMouseUp);

  drag.clone.remove();
  drag.clone = null;
  drag.cardEl.classList.remove("dragging");
  document.querySelectorAll(".kanban-column.drag-over")
    .forEach(c => c.classList.remove("drag-over"));

  const targetKey = drag.overColKey;
  const card      = drag.cardEl;
  const blocker   = drag.blocker;
  const fromKey   = drag.fromColKey;

  drag.active = drag.cardEl = drag.blocker = drag.overColKey = drag.fromColKey = null;

  if (!targetKey || targetKey === fromKey) return;

  // Move card into new column
  const newCol = getCol(targetKey);
  if (newCol) {
    newCol.appendChild(card);
    // Landing animation
    card.classList.remove("landing");
    void card.offsetWidth;
    card.classList.add("landing");
    card.addEventListener("animationend", () => card.classList.remove("landing"), { once: true });
    // Sync status dropdown on the card
    const statusSel = card.querySelector("select.blocker-field-select:not([class*='severity'])");
    if (statusSel) statusSel.value = targetKey;
  }

  updateColumnCounts();
  updateBlockerField(blocker.path, "status", targetKey).catch(console.error);
}

// ── Public renderer ───────────────────────────────────────────────────────────
export function renderBlockerBoard() {
  if (!document.getElementById("col-open")) return;

  // Clear non-header children
  Object.values(COL_ID).forEach(colId => {
    const col = document.getElementById(colId);
    if (!col) return;
    Array.from(col.children).forEach(child => {
      if (!child.classList.contains("column-header")) col.removeChild(child);
    });
  });

  state.cachedIndex.blockers.forEach(blocker => {
    const card = buildCard(blocker);
    getCol(blocker.status || "open")?.appendChild(card);
  });

  updateColumnCounts();
}

// ── Card builder ──────────────────────────────────────────────────────────────
function buildCard(blocker) {
  const card = document.createElement("div");
  card.className = "kanban-card";

  // ── Drag grip — pinned to top-right corner ───────────────────────────────
  const grip = document.createElement("div");
  grip.className = "card-drag-grip";
  grip.title = "Drag to move";
  grip.innerHTML = `
    <svg width="20" height="14" viewBox="0 0 20 14" fill="currentColor">
      <circle cx="3"  cy="2" r="1.6"/><circle cx="10" cy="2" r="1.6"/><circle cx="17" cy="2" r="1.6"/>
      <circle cx="3"  cy="7" r="1.6"/><circle cx="10" cy="7" r="1.6"/><circle cx="17" cy="7" r="1.6"/>
      <circle cx="3"  cy="12" r="1.6"/><circle cx="10" cy="12" r="1.6"/><circle cx="17" cy="12" r="1.6"/>
    </svg>
  `;

  grip.addEventListener("mousedown", (e) => {
    e.preventDefault();
    const rect = card.getBoundingClientRect();

    drag.active     = true;
    drag.cardEl     = card;
    drag.blocker    = blocker;
    drag.fromColKey = colKeyFromEl(card);
    drag.offsetX    = e.clientX - rect.left;
    drag.offsetY    = e.clientY - rect.top;

    // Floating clone that follows the cursor
    const clone = card.cloneNode(true);
    clone.className = "kanban-card drag-clone";
    clone.style.cssText = [
      `position:fixed`,
      `width:${rect.width}px`,
      `left:${rect.left}px`,
      `top:${rect.top}px`,
      `pointer-events:none`,
      `z-index:9999`,
      `opacity:0.92`,
      `transform:rotate(1.5deg) scale(1.03)`,
      `box-shadow:0 20px 50px rgba(0,0,0,0.6)`,
      `transition:transform 0.1s ease,box-shadow 0.1s ease`,
    ].join(";");
    document.body.appendChild(clone);
    drag.clone = clone;

    card.classList.add("dragging");

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup",   onMouseUp);
  });

  // ── Severity dropdown ─────────────────────────────────────────────────────
  const sevEl = document.createElement("select");
  sevEl.className = `blocker-field-select severity-${(blocker.severity || "p1").toLowerCase()}`;
  ["P0", "P1", "P2"].forEach(opt => {
    const o = document.createElement("option");
    o.value = opt;
    o.textContent = opt;
    if (opt === blocker.severity) o.selected = true;
    sevEl.appendChild(o);
  });
  sevEl.addEventListener("change", async (e) => {
    e.stopPropagation();
    sevEl.className = `blocker-field-select severity-${e.target.value.toLowerCase()}`;
    await updateBlockerField(blocker.path, "severity", e.target.value);
  });
  sevEl.addEventListener("mousedown", (e) => e.stopPropagation()); // don't start drag

  // ── Status dropdown (moves card on change) ────────────────────────────────
  const statusEl = document.createElement("select");
  statusEl.className = "blocker-field-select";
  statusEl.style.marginLeft = "8px";
  ["open", "in_progress", "blocked", "resolved"].forEach(opt => {
    const o = document.createElement("option");
    o.value = opt;
    o.textContent = opt.replace("_", " ");
    if (opt === blocker.status) o.selected = true;
    statusEl.appendChild(o);
  });
  statusEl.addEventListener("change", async (e) => {
    e.stopPropagation();
    const newStatus = e.target.value;
    const newCol = getCol(newStatus);
    if (newCol) {
      newCol.appendChild(card);
      card.classList.remove("sliding-in");
      void card.offsetWidth;
      card.classList.add("sliding-in");
      card.addEventListener("animationend", () => card.classList.remove("sliding-in"), { once: true });
    }
    updateColumnCounts();
    await updateBlockerField(blocker.path, "status", newStatus);
  });
  statusEl.addEventListener("mousedown", (e) => e.stopPropagation()); // don't start drag

  // ── Controls row (dropdowns only — grip is a corner overlay) ─────────────
  const controlsRow = document.createElement("div");
  controlsRow.className = "card-controls-row";
  controlsRow.appendChild(sevEl);
  controlsRow.appendChild(statusEl);
  if (blocker.stale) {
    const t = document.createElement("span");
    t.className = "card-stale-tag stale-tag";
    t.textContent = "stale";
    controlsRow.appendChild(t);
  }

  // ── Title ─────────────────────────────────────────────────────────────────
  const titleEl = document.createElement("div");
  titleEl.className = "card-title-text";
  titleEl.textContent = blocker.title;
  titleEl.title = "Click to open in editor";
  titleEl.addEventListener("click", () => {
    if (window.openFileInEditor) window.openFileInEditor(blocker.path);
  });

  // ── Source file chips ─────────────────────────────────────────────────────
  const sourceFiles = blocker.source_files || [];
  let sourceRow = null;
  if (sourceFiles.length > 0) {
    sourceRow = document.createElement("div");
    sourceRow.className = "card-source-row";
    sourceFiles.forEach(f => {
      const chip = document.createElement("span");
      chip.className = "file-chip";
      chip.textContent = f.split("/").pop();
      chip.title = f;
      sourceRow.appendChild(chip);
    });
  }

  // ── Meta ──────────────────────────────────────────────────────────────────
  const metaEl = document.createElement("div");
  metaEl.className = "card-meta";

  const idSpan = document.createElement("span");
  idSpan.className = "card-id";
  idSpan.textContent = blocker.id;

  const ownerSpan = document.createElement("span");
  ownerSpan.className = "card-assignee";
  ownerSpan.textContent = blocker.owner || "—";

  metaEl.appendChild(idSpan);
  metaEl.appendChild(ownerSpan);

  card.appendChild(grip);        // corner overlay — must be first in DOM so z-index stacks correctly
  card.appendChild(controlsRow);
  card.appendChild(titleEl);
  if (sourceRow) card.appendChild(sourceRow);
  card.appendChild(metaEl);

  return card;
}

// ── Column badge counts ───────────────────────────────────────────────────────
function updateColumnCounts() {
  Object.entries(COL_ID).forEach(([key, colId]) => {
    const badgeId = `badge-${key === "in_progress" ? "progress" : key}`;
    const col   = document.getElementById(colId);
    const badge = document.getElementById(badgeId);
    if (!col || !badge) return;
    badge.textContent = Array.from(col.children)
      .filter(c => !c.classList.contains("column-header")).length;
  });
}
