import { state } from '../state.js';
import { parseTextBoldFormatting, stripComments, getOwnerInitials, getGateDates } from '../utils.js';
import { toggleChecklistItem, getWaivers } from '../api.js';
import { canWriteChecklistFile, showToast, syncOpenEditorTab, statusForChecked } from '../checklist.js';
import { renderEvidenceSummary } from './evidence.js';
import { normalizeIndexPayload } from '../contract.js';

export function renderGoNoGo() {
  renderGoNoGoRecommendation();
  renderGoNoGoGates();
  renderGoNoGoSignoffs();
  renderGoNoGoBlockers();
  renderGoNoGoEvidence();
  renderGoNoGoWaivers();
}

function renderGoNoGoRecommendation() {
  const { cachedIndex } = state;
  const el = document.getElementById("gonogo-recommendation");
  if (!el) return;

  const readiness = cachedIndex.readiness || "At Risk";
  const reasons = cachedIndex.readiness_reasons || [];
  const cls = readiness === "Go" ? "go" : readiness === "No-Go" ? "no-go" : "at-risk";
  const icon = readiness === "Go" ? "[GO]" : readiness === "No-Go" ? "[NO-GO]" : "[AT RISK]";
  const title = readiness === "Go" ? "Go for Launch" : readiness === "No-Go" ? "No-Go — Launch Blocked" : "At Risk — Review Required";

  el.className = `gonogo-recommendation ${cls}`;
  el.textContent = "";

  const iconEl = document.createElement("div");
  iconEl.className = "gonogo-rec-icon";

  let iconColor = "var(--text-secondary)";
  let iconBg = "rgba(255, 255, 255, 0.05)";
  let iconBorder = "var(--border-glass)";
  if (readiness === "Go") {
    iconColor = "var(--color-go)";
    iconBg = "rgba(16, 185, 129, 0.1)";
    iconBorder = "rgba(16, 185, 129, 0.2)";
  } else if (readiness === "No-Go") {
    iconColor = "var(--color-blocker)";
    iconBg = "rgba(239, 68, 68, 0.1)";
    iconBorder = "rgba(239, 68, 68, 0.2)";
  } else {
    iconColor = "var(--color-pending)";
    iconBg = "rgba(245, 158, 11, 0.1)";
    iconBorder = "rgba(245, 158, 11, 0.2)";
  }
  iconEl.style.cssText = `font-family: var(--font-mono); font-size: 14px; font-weight: 700; padding: 6px 12px; border-radius: 4px; color: ${iconColor}; background: ${iconBg}; border: 1px solid ${iconBorder}; text-align: center;`;
  iconEl.textContent = icon;

  const content = document.createElement("div");
  content.className = "gonogo-rec-content";

  const titleEl = document.createElement("div");
  titleEl.className = "gonogo-rec-title";
  titleEl.textContent = title;

  const subtitleEl = document.createElement("div");
  subtitleEl.className = "gonogo-rec-subtitle";
  subtitleEl.style.display = "flex";
  subtitleEl.style.flexDirection = "column";
  subtitleEl.style.gap = "6px";
  subtitleEl.style.marginTop = "8px";

  if (reasons.length === 0) {
    const line = document.createElement("div");
    line.textContent = "All systems nominal.";
    subtitleEl.appendChild(line);
  } else {
    reasons.forEach(reason => {
      const line = document.createElement("div");
      line.style.cssText = "font-size:12px; line-height:1.5; color:rgba(255,255,255,0.7);";
      const cleanReason = stripComments(reason);
      parseTextBoldFormatting(line, cleanReason);
      subtitleEl.appendChild(line);
    });
  }

  content.appendChild(titleEl);
  content.appendChild(subtitleEl);
  el.appendChild(iconEl);
  el.appendChild(content);
}

function renderGoNoGoGates() {
  const { cachedIndex } = state;
  const container = document.getElementById("gonogo-gates-list");
  if (!container) return;
  container.textContent = "";

  const gates = cachedIndex.go_no_go_gates || [];
  if (gates.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state text-muted";
    empty.textContent = "No gates configured in go_no_go.md.";
    container.appendChild(empty);
    return;
  }

  gates.forEach(gate => {
    const itemEl = document.createElement("div");
    itemEl.className = "checklist-item";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.className = "checklist-checkbox";
    checkbox.checked = gate.status === "checked";

    const content = document.createElement("div");
    content.className = "checklist-content";
    content.style.display = "flex";
    content.style.justifyContent = "space-between";
    content.style.alignItems = "center";
    content.style.width = "100%";

    const textEl = document.createElement("div");
    textEl.className = "checklist-text";
    parseTextBoldFormatting(textEl, stripComments(gate.name));

    const metaEl = document.createElement("div");
    metaEl.style.cssText = "display:flex;align-items:center;gap:8px;flex-shrink:0;margin-left:12px;";

    if (gate.owner) {
      const ownerEl = document.createElement("span");
      ownerEl.className = "gate-owner text-muted";
      ownerEl.textContent = gate.owner;
      metaEl.appendChild(ownerEl);
    }

    const statusLabel = gate.status === "checked" ? "Passed" : gate.status === "partial" ? "In Progress" : "Pending";
    const statusClass = gate.status === "checked" ? "completed" : gate.status === "partial" ? "at-risk" : "pending";
    const pill = document.createElement("span");
    pill.className = `status-pill ${statusClass}`;
    pill.textContent = statusLabel;
    metaEl.appendChild(pill);

    checkbox.addEventListener("change", async () => {
      const checked = checkbox.checked;
      const previousStatus = gate.status;
      if (typeof gate.line_index !== "number") {
        checkbox.checked = previousStatus === "checked";
        showToast("This gate is missing line metadata. Refresh the index before toggling.");
        return;
      }
      if (!canWriteChecklistFile("go_no_go.md")) {
        checkbox.checked = previousStatus === "checked";
        return;
      }

      const next = statusForChecked(checked);
      gate.status = next.status;
      pill.className = `status-pill ${next.className}`;
      pill.textContent = next.label;
      checkbox.disabled = true;
      renderGoNoGoRecommendation();
      renderGoNoGoSignoffs();
      renderGoNoGoEvidence();

      try {
        await toggleChecklistItem("go_no_go.md", gate.line_index, checked);
        await syncOpenEditorTab("go_no_go.md");
        if (window.refreshUI) await window.refreshUI();
      } catch (err) {
        gate.status = previousStatus;
        checkbox.checked = previousStatus === "checked";
        const rollback = statusForChecked(previousStatus === "checked");
        pill.className = `status-pill ${previousStatus === "partial" ? "at-risk" : rollback.className}`;
        pill.textContent = previousStatus === "partial" ? "In Progress" : rollback.label;
        showToast(`Could not update go_no_go.md: ${err.message || err}`, "error");
        renderGoNoGoRecommendation();
        renderGoNoGoSignoffs();
        renderGoNoGoEvidence();
      } finally {
        checkbox.disabled = false;
      }
    });

    content.appendChild(textEl);
    content.appendChild(metaEl);
    itemEl.appendChild(checkbox);
    itemEl.appendChild(content);
    container.appendChild(itemEl);
  });
}

function renderGoNoGoSignoffs() {
  const { cachedIndex } = state;
  const container = document.getElementById("gonogo-signoffs-table");
  if (!container) return;
  container.textContent = "";

  const signoffs = cachedIndex.signoffs || [];
  if (signoffs.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state text-muted";
    empty.textContent = "No sign-offs configured.";
    container.appendChild(empty);
    return;
  }

  signoffs.forEach(s => {
    const row = document.createElement("div");
    row.className = "approval-row-item";

    const left = document.createElement("div");
    left.className = "approval-stakeholder";

    const roleEl = document.createElement("span");
    roleEl.className = "approval-role";
    roleEl.textContent = s.role;

    const nameEl = document.createElement("span");
    nameEl.className = "approval-name";
    nameEl.textContent = s.name;

    left.appendChild(roleEl);
    left.appendChild(nameEl);

    const statusClass = s.status === "Approved" ? "completed" : s.status === "In Progress" ? "at-risk" : "pending";
    const pill = document.createElement("span");
    pill.className = `status-pill ${statusClass}`;
    pill.textContent = s.status;

    row.appendChild(left);
    row.appendChild(pill);

    if (s.date) {
      const dateEl = document.createElement("span");
      dateEl.className = "approval-date text-muted";
      dateEl.textContent = s.date;
      row.appendChild(dateEl);
    }

    container.appendChild(row);
  });
}

function renderGoNoGoBlockers() {
  const { cachedIndex } = state;
  const container = document.getElementById("gonogo-blocking-issues");
  if (!container) return;
  container.textContent = "";

  const active = (cachedIndex.blockers || []).filter(b => b.status !== "resolved");
  if (active.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state text-go";
    empty.textContent = "✓ No active blockers. Clear for launch.";
    container.appendChild(empty);
    return;
  }

  active.forEach(b => {
    const item = document.createElement("div");
    item.className = "dashboard-blocker-item";
    item.style.cursor = "pointer";

    const header = document.createElement("div");
    header.className = "blocker-item-header";

    const idSpan = document.createElement("span");
    idSpan.className = "blocker-item-id";
    idSpan.textContent = b.id;

    const sevSpan = document.createElement("span");
    sevSpan.className = `blocker-item-severity ${b.severity.toLowerCase()}`;
    sevSpan.textContent = b.severity;

    header.appendChild(idSpan);
    header.appendChild(sevSpan);

    const titleDiv = document.createElement("div");
    titleDiv.className = "blocker-item-title";
    titleDiv.textContent = b.title;

    item.appendChild(header);
    item.appendChild(titleDiv);
    item.addEventListener("click", () => window.openFileInEditor(b.path));
    container.appendChild(item);
  });
}

function renderGoNoGoEvidence() {
  const container = document.getElementById("gonogo-evidence");
  if (!container) return;
  renderEvidenceSummary(container, state.cachedIndex);
}

export async function renderGoNoGoWaivers() {
  const container = document.getElementById("gonogo-waivers-list");
  if (!container) return;
  container.textContent = "";

  let active = [];
  try {
    const list = await getWaivers();
    active = (list || []).filter(w => w.status === "active");
  } catch (err) {
    console.warn("Failed to fetch waivers for Go/No-Go room:", err);
  }

  if (active.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state text-muted";
    empty.style.cssText = "font-size: 12px; padding: 16px; text-align: center;";
    empty.textContent = "No active waivers.";
    container.appendChild(empty);
    return;
  }

  active.forEach(w => {
    const itemEl = document.createElement("div");
    itemEl.className = "checklist-item";
    itemEl.style.cssText = "display: flex; flex-direction: column; gap: 6px; padding: 14px; margin-bottom: 8px;";

    const header = document.createElement("div");
    header.style.cssText = "display: flex; justify-content: space-between; align-items: center; width: 100%;";

    const titleEl = document.createElement("span");
    titleEl.style.cssText = "font-size: 13.5px; font-weight: 600; color: #a5b4fc;";
    titleEl.textContent = `🛡 ${w.id} (${w.target_type})`;

    const approverEl = document.createElement("span");
    approverEl.className = "status-pill completed";
    approverEl.style.cssText = "font-size: 10px; background: rgba(59, 130, 246, 0.1); border-color: rgba(59, 130, 246, 0.2); color: var(--color-primary);";
    approverEl.textContent = `Approved by: ${w.approved_by || 'Unknown'}`;

    header.appendChild(titleEl);
    header.appendChild(approverEl);

    const body = document.createElement("div");
    body.style.cssText = "font-size: 12.5px; color: var(--text-primary); line-height: 1.4; margin-top: 2px;";
    body.textContent = w.reason;

    const footer = document.createElement("div");
    footer.style.cssText = "display: flex; justify-content: space-between; font-size: 11px; color: var(--text-muted); margin-top: 4px; border-top: 1px solid rgba(255, 255, 255, 0.05); padding-top: 6px;";

    const expiresEl = document.createElement("span");
    expiresEl.textContent = `Expires: ${w.expires_at || 'Never'}`;

    let targetDetail = "";
    if (w.target_id) targetDetail = `ID: ${w.target_id}`;
    else if (w.target_path) targetDetail = `Path: ${w.target_path}`;

    const targetEl = document.createElement("span");
    targetEl.style.fontFamily = "var(--font-mono)";
    targetEl.textContent = targetDetail;

    footer.appendChild(expiresEl);
    footer.appendChild(targetEl);

    itemEl.appendChild(header);
    itemEl.appendChild(body);
    itemEl.appendChild(footer);

    container.appendChild(itemEl);
  });
}
