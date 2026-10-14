import { state, setCachedIndex } from '../state.js';
import { parseTextBoldFormatting, stripComments, getOwnerInitials, getGateDates } from '../utils.js';
import { fetchRecentChanges, hasTauri, fetchLaunchIndex, toggleChecklistItem, getCIStatus, loadFileContent, saveFileContent, getWaivers, getReleaseManifest, getTrends } from '../api.js';
import { normalizeIndexPayload, getReadinessMeta } from '../contract.js';
import { renderEvidenceSummary } from './evidence.js';
import { renderScorecard } from './scorecard.js';
import { renderReleaseProgression } from './progression.js';
import { canWriteChecklistFile, showToast, syncOpenEditorTab, statusForChecked } from '../checklist.js';

export async function renderDashboard() {
  // Render release progression strip
  const progressionStripEl = document.getElementById("dashboard-progression-strip");
  if (progressionStripEl) {
    renderReleaseProgression(progressionStripEl, state.cachedIndex);
  }

  // Render scorecard
  const scorecardContainerEl = document.getElementById("dashboard-scorecard-container");
  if (scorecardContainerEl) {
    renderScorecard(scorecardContainerEl, state.cachedIndex);
  }

  renderQAProgress();
  const blockersListEl = document.getElementById("dashboard-blockers-list");
  if (blockersListEl) {
    renderEvidenceSummary(blockersListEl, state.cachedIndex);
  }
  renderNextActions();
  renderDashboardBlockerCounts();
  renderDashboardApprovals();
  await renderDashboardActivity();
  await updateCiStatusWidget();
  renderUptimeAndReadiness();
  renderGatesTable();
  renderTelemetryStatus();

  // New local-first capabilities UI integration
  await renderDashboardWaivers();
  await renderDashboardReleaseManifest();
  await renderDashboardTrends();
}

export function renderTelemetryStatus() {
  const { cachedIndex } = state;
  const stabilityEl = document.getElementById("sentry-stability");
  const badgeEl = document.getElementById("sentry-stability-badge");
  const targetEl = document.getElementById("sentry-target-label");
  const labelEl = document.getElementById("sentry-label");

  if (!stabilityEl || !badgeEl || !targetEl) return;

  const activeEnv = state.activeEnv || cachedIndex.active_environment || "production";
  const envRes = cachedIndex.environment_results ? cachedIndex.environment_results[activeEnv] : null;

  const status = (cachedIndex.telemetry_status || "not_required").toLowerCase();
  const telemetry = cachedIndex.observed_telemetry || null;

  const minCrashRate = envRes && envRes.profile ? (envRes.profile.minCrashFreeRate ?? envRes.profile.min_crash_free_rate) : null;

  // Update crash free rate observed value
  if (telemetry && typeof telemetry.crash_free_rate === "number") {
    stabilityEl.textContent = `${(telemetry.crash_free_rate).toFixed(2)}%`;
    stabilityEl.className = "stat-val";

    // Highlight crash free rate if below target
    if (typeof minCrashRate === "number" && telemetry.crash_free_rate < minCrashRate) {
      stabilityEl.classList.add("alert");
    }
  } else {
    stabilityEl.textContent = "—";
    stabilityEl.className = "stat-val";
  }

  // Update badge and colors
  badgeEl.style.cssText = ""; // clear custom styles first
  if (status === "fresh") {
    badgeEl.textContent = "FRESH";
    badgeEl.className = "status-pill completed";
  } else if (status === "stale") {
    badgeEl.textContent = "STALE";
    badgeEl.className = "status-pill at-risk";
  } else if (status === "missing") {
    badgeEl.textContent = "MISSING";
    badgeEl.className = "status-pill pending";
    badgeEl.style.cssText = "color: var(--color-blocker); border-color: rgba(239, 68, 68, 0.2); background: rgba(239, 68, 68, 0.05); font-size: 8px; margin-left: 6px;";
  } else if (status === "invalid") {
    badgeEl.textContent = "INVALID";
    badgeEl.className = "status-pill pending";
    badgeEl.style.cssText = "color: var(--color-blocker); border-color: rgba(239, 68, 68, 0.2); background: rgba(239, 68, 68, 0.05); font-size: 8px; margin-left: 6px;";
  } else {
    badgeEl.textContent = "NOT REQUIRED";
    badgeEl.className = "status-pill pending";
  }

  // Update target rate label
  if (typeof minCrashRate === "number") {
    targetEl.textContent = `Target: ≥${(minCrashRate).toFixed(2)}%`;
  } else {
    targetEl.textContent = "Target: Not Required";
  }

  // Update Sentry source label
  if (labelEl) {
    labelEl.textContent = telemetry && telemetry.source
      ? `Sentry: ${telemetry.source}`
      : "Sentry Crash-Free";
  }
}

export function renderQAProgress() {
  const { cachedIndex } = state;
  const total = cachedIndex.qa_total;
  const completed = cachedIndex.qa_completed;
  const pct = total > 0 ? Math.round((completed / total) * 100) : 100;

  const qaProgressText = document.getElementById("qa-progress-text");
  const qaProgressPct = document.getElementById("qa-progress-pct");
  const qaProgressBar = document.getElementById("qa-progress-bar");

  if (!qaProgressText || !qaProgressPct || !qaProgressBar) return;

  qaProgressText.textContent = `${completed} / ${total}`;
  qaProgressPct.textContent = `${pct}%`;
  qaProgressBar.style.width = `${pct}%`;

  qaProgressBar.className = "progress-bar-fill";
  if (pct >= 100) {
    qaProgressBar.classList.add("complete");
    qaProgressText.style.color = "var(--color-go)";
  } else if (pct >= 50) {
    qaProgressBar.classList.add("at-risk");
    qaProgressText.style.color = "var(--color-pending)";
  } else {
    qaProgressText.style.color = "var(--color-blocker)";
  }
}

async function addWaiver(targetType, targetId, targetPath, targetCode) {
  const reason = prompt(`Enter reason for waiving this ${targetType}:`, "Temporary exemption for launch alignment");
  if (reason === null) return; // cancelled
  if (reason.trim() === "") {
    alert("Waiver reason cannot be empty.");
    return;
  }

  try {
    let waiversContent = "[]";
    try {
      waiversContent = await loadFileContent("waivers.json");
      if (!waiversContent || waiversContent.trim() === "") {
        waiversContent = "[]";
      }
    } catch (e) {
      console.log("waivers.json not found, initializing empty array");
    }

    let waivers = [];
    try {
      waivers = JSON.parse(waiversContent);
    } catch (e) {
      console.error("Failed to parse waivers.json", e);
      waivers = [];
    }

    const user = document.getElementById("profile-name")?.textContent || "@user";
    const dateToday = new Date().toISOString().split("T")[0];
    const expiresDate = new Date(Date.now() + 14 * 24 * 3600 * 1000).toISOString().split("T")[0]; // 14 days

    const newWaiver = {
      id: "WAV-" + Math.random().toString(36).substring(2, 8).toUpperCase(),
      target_type: targetType,
      target_id: targetId || null,
      target_path: targetPath || null,
      target_code: targetCode || null,
      reason: reason.trim(),
      approved_by: user.trim(),
      created_at: dateToday,
      expires_at: expiresDate,
      status: "active"
    };

    waivers.push(newWaiver);
    await saveFileContent("waivers.json", JSON.stringify(waivers, null, 2));

    if (window.refreshUI) {
      await window.refreshUI();
    }
  } catch (err) {
    alert(`Failed to save waiver: ${err.message || err}`);
  }
}

export function renderNextActions() {
  const { cachedIndex } = state;
  const list = document.getElementById("next-actions-list");
  if (!list) return;

  list.textContent = "";
  const actions = [];

  // 1. P0 blockers
  if (cachedIndex.blockers) {
    cachedIndex.blockers.forEach(b => {
      if (b.severity === "P0" && b.status !== "resolved" && !b.waived) {
        actions.push({
          icon: "P0",
          text: `Resolve P0 Blocker: ${b.title}`,
          meta: b.id,
          priority: "high",
          type: "blocker",
          id: b.id,
          path: b.path,
          action: () => window.openFileInEditor(b.path)
        });
      }
    });
  }

  // 2. Failed gates
  if (cachedIndex.go_no_go_gates) {
    cachedIndex.go_no_go_gates.forEach(g => {
      if (g.status !== "checked" && !g.waived) {
        let cleanOwner = g.owner || "Unassigned";
        if (cleanOwner.startsWith("owner:")) {
          cleanOwner = cleanOwner.substring(6);
        }
        actions.push({
          icon: "GATE",
          text: `Clear Gate: ${g.name}`,
          meta: cleanOwner,
          priority: "medium",
          type: "gate",
          name: g.name,
          lineIndex: g.line_index,
          action: () => window.switchView("gonogo")
        });
      }
    });
  }

  // 3. Validation errors
  if (cachedIndex.validation_errors) {
    cachedIndex.validation_errors.forEach(err => {
      if (!err.waived) {
        actions.push({
          icon: "VAL",
          text: `Fix Validation: ${err.message}`,
          meta: err.code || "SCHEMA",
          priority: err.severity === "error" ? "high" : "low",
          type: "validation",
          file: err.file,
          code: err.code || "SCHEMA",
          action: () => window.openFileInEditor(err.file)
        });
      }
    });
  }

  // 4. Broken links
  if (cachedIndex.broken_links) {
    cachedIndex.broken_links.forEach(err => {
      if (!err.waived) {
        actions.push({
          icon: "LINK",
          text: `Fix Broken Link: ${err.message}`,
          meta: "LINK",
          priority: "medium",
          type: "validation",
          file: err.file,
          code: "BROKEN_LINK",
          action: () => window.openFileInEditor(err.file)
        });
      }
    });
  }

  // 5. Stale docs
  if (cachedIndex.stale_docs) {
    cachedIndex.stale_docs.forEach(docPath => {
      const filename = docPath.split("/").pop();
      const isWaived = (cachedIndex.validation_errors || []).some(err => err.file === docPath && err.code === "STALE_DOC" && err.waived);
      if (!isWaived) {
        actions.push({
          icon: "STALE",
          text: `Re-verify Stale Doc: ${filename}`,
          meta: "STALE",
          priority: "low",
          type: "validation",
          file: docPath,
          code: "STALE_DOC",
          action: () => window.openFileInEditor(docPath)
        });
      }
    });
  }

  // 6. Incomplete QA suites
  if (cachedIndex.qa_suites) {
    cachedIndex.qa_suites.forEach(suite => {
      if (suite.completed_tests < suite.total_tests && !suite.waived) {
        let cleanOwner = suite.owner || "Unassigned";
        if (cleanOwner.startsWith("owner:")) {
          cleanOwner = cleanOwner.substring(6);
        }
        actions.push({
          icon: "QA",
          text: `Complete QA Suite: ${suite.title}`,
          meta: `${suite.completed_tests}/${suite.total_tests}`,
          priority: "medium",
          type: "qa",
          id: suite.id || suite.path,
          path: suite.path,
          action: () => {
            state.selectedQASuitePath = suite.path;
            window.switchView("qa");
          }
        });
      }
    });
  }

  // 7. P1 blockers
  if (cachedIndex.blockers) {
    cachedIndex.blockers.forEach(b => {
      if (b.severity === "P1" && b.status !== "resolved" && !b.waived) {
        actions.push({
          icon: "P1",
          text: `Resolve Blocker: ${b.title}`,
          meta: b.id,
          priority: "medium",
          type: "blocker",
          id: b.id,
          path: b.path,
          action: () => window.openFileInEditor(b.path)
        });
      }
    });
  }

  const displayActions = actions.slice(0, 5);

  if (displayActions.length === 0) {
    const el = document.createElement("div");
    el.className = "action-item";
    const iconEl = document.createElement("span");
    iconEl.className = "action-icon";
    iconEl.style.cssText = "font-family: var(--font-mono); font-size: 9px; font-weight: 700; padding: 2px 6px; border-radius: 2px; background: rgba(16, 185, 129, 0.1); border: 1px solid rgba(16, 185, 129, 0.2); color: var(--color-go); text-align: center; min-width: 48px; flex-shrink: 0;";
    iconEl.textContent = "SHIP";
    const textEl = document.createElement("span");
    textEl.className = "action-text";
    textEl.textContent = "All launch gates are clear — ready to ship!";
    el.appendChild(iconEl);
    el.appendChild(textEl);
    list.appendChild(el);
    return;
  }

  displayActions.forEach(item => {
    const el = document.createElement("div");
    el.className = "action-item";

    const iconEl = document.createElement("span");
    iconEl.className = "action-icon";

    // Determine color coding based on action priority / icon
    let bg = "rgba(255, 255, 255, 0.05)";
    let border = "1px solid var(--border-glass)";
    let color = "var(--text-secondary)";

    if (item.priority === "high") {
      bg = "rgba(239, 68, 68, 0.1)";
      border = "1px solid rgba(239, 68, 68, 0.2)";
      color = "var(--color-blocker)";
    } else if (item.priority === "medium") {
      if (item.icon === "GATE" || item.icon === "LINK") {
        bg = "rgba(59, 130, 246, 0.1)";
        border = "1px solid rgba(59, 130, 246, 0.2)";
        color = "var(--color-primary)";
      } else {
        bg = "rgba(245, 158, 11, 0.1)";
        border = "1px solid rgba(245, 158, 11, 0.2)";
        color = "var(--color-pending)";
      }
    } else if (item.priority === "low") {
      bg = "rgba(245, 158, 11, 0.06)";
      border = "1px solid rgba(245, 158, 11, 0.15)";
      color = "var(--color-pending)";
    }

    iconEl.style.cssText = `font-family: var(--font-mono); font-size: 9px; font-weight: 700; padding: 2px 6px; border-radius: 2px; text-align: center; min-width: 48px; flex-shrink: 0; background: ${bg}; border: ${border}; color: ${color};`;
    iconEl.textContent = item.icon;

    const textEl = document.createElement("span");
    textEl.className = "action-text";
    textEl.style.cssText = "flex:1; margin-right:8px; line-height:1.3;";

    // Strip comments (e.g. owner tag comments) and parse markdown bold tags
    const cleanText = stripComments(item.text);
    parseTextBoldFormatting(textEl, cleanText);

    const controlsEl = document.createElement("div");
    controlsEl.className = "action-controls-wrapper";

    const metaEl = document.createElement("span");
    metaEl.className = "action-meta";
    let cleanMeta = item.meta;
    if (cleanMeta.startsWith("owner:")) {
      cleanMeta = cleanMeta.substring(6);
    }
    metaEl.textContent = cleanMeta;
    controlsEl.appendChild(metaEl);

    if (item.type) {
      const btnContainer = document.createElement("div");
      btnContainer.className = "action-buttons";

      // 1. Resolve/Clear button
      if (item.type === "blocker") {
        const resolveBtn = document.createElement("button");
        resolveBtn.className = "btn-action-mini resolve";
        resolveBtn.innerHTML = `✓ Resolve`;
        resolveBtn.title = "Mark blocker as resolved";
        resolveBtn.addEventListener("click", async (e) => {
          e.stopPropagation();
          try {
            const { updateBlockerField } = await import('../api.js');
            await updateBlockerField(item.path, "status", "resolved");
            if (window.refreshUI) await window.refreshUI();
          } catch (err) {
            alert(`Failed to resolve blocker: ${err.message || err}`);
          }
        });
        btnContainer.appendChild(resolveBtn);
      } else if (item.type === "gate") {
        const clearBtn = document.createElement("button");
        clearBtn.className = "btn-action-mini resolve";
        clearBtn.innerHTML = `✓ Clear`;
        clearBtn.title = "Clear this gate in go_no_go.md";
        clearBtn.addEventListener("click", async (e) => {
          e.stopPropagation();
          try {
            await toggleChecklistItem("go_no_go.md", item.lineIndex, true);
            if (window.refreshUI) await window.refreshUI();
          } catch (err) {
            alert(`Failed to clear gate: ${err.message || err}`);
          }
        });
        btnContainer.appendChild(clearBtn);
      }

      // 2. Waive button
      const waiveBtn = document.createElement("button");
      waiveBtn.className = "btn-action-mini waive";
      waiveBtn.innerHTML = `🛡 Waive`;
      waiveBtn.title = `Add risk waiver for this ${item.type}`;
      waiveBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        if (item.type === "blocker") {
          addWaiver("blocker", item.id, item.path, null);
        } else if (item.type === "gate") {
          addWaiver("gate", item.name, "go_no_go.md", null);
        } else if (item.type === "validation") {
          const type = item.code === "MISSING_DOC" ? "document" : "validation";
          addWaiver(type, null, item.file, item.code);
        } else if (item.type === "qa") {
          addWaiver("qa", item.id, item.path, null);
        }
      });
      btnContainer.appendChild(waiveBtn);

      controlsEl.appendChild(btnContainer);
    }

    el.appendChild(iconEl);
    el.appendChild(textEl);
    el.appendChild(controlsEl);

    if (item.action) {
      el.setAttribute("role", "button");
      el.setAttribute("tabindex", "0");
      el.addEventListener("click", item.action);
      el.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          item.action();
        }
      });
    }
    list.appendChild(el);
  });
}

export function renderDashboardBlockerCounts() {
  const { cachedIndex } = state;
  const banner = document.getElementById("blocker-alert-banner");
  const bannerText = document.getElementById("blocker-banner-text");

  const activeBlockers = (cachedIndex.blockers || []).filter(b => b.status !== "resolved");

  if (banner) {
    if (activeBlockers.length > 0) {
      banner.style.display = "flex";
      if (bannerText) bannerText.textContent = `${activeBlockers.length} active blocker(s) require attention before launch.`;
    } else {
      banner.style.display = "none";
    }
  }

  // Update telemetry and badge counts
  const allBlockers = cachedIndex.blockers || [];
  const countOpen = allBlockers.filter(b => b.status === "open").length;
  const countAtRisk = allBlockers.filter(b => b.status === "in_progress" || b.status === "blocked").length;
  const countCompleted = allBlockers.filter(b => b.status === "resolved").length;

  const elOpen = document.getElementById("count-open");
  const elAtRisk = document.getElementById("count-at-risk");
  const elCompleted = document.getElementById("count-completed");
  if (elOpen) elOpen.textContent = countOpen;
  if (elAtRisk) elAtRisk.textContent = countAtRisk;
  if (elCompleted) elCompleted.textContent = countCompleted;

  const blockerCountEl = document.getElementById("blocker-count");
  if (blockerCountEl) blockerCountEl.textContent = activeBlockers.length;
}

export function renderDashboardApprovals() {
  const { cachedIndex } = state;
  const listContainer = document.getElementById("dashboard-approvals-list");
  if (!listContainer) return;
  listContainer.textContent = "";

  const signoffs = cachedIndex.signoffs || [];
  if (signoffs.length === 0) {
    listContainer.innerHTML = `<div style="display:flex;gap:8px;align-items:center;color:var(--text-muted);font-size:12px;padding:8px 0;">
      <span style="opacity:0.4;font-size:18px;">✓</span>
      <span>No sign-offs configured.<br><span style="font-size:10px;">Add a <code>signoffs:</code> block to go_no_go.md.</span></span>
    </div>`;
    return;
  }

  signoffs.forEach(s => {
    const item = document.createElement("div");
    item.className = "approval-row-item";

    const stakeholderDiv = document.createElement("div");
    stakeholderDiv.className = "approval-stakeholder";

    const roleSpan = document.createElement("span");
    roleSpan.className = "approval-role";
    roleSpan.textContent = s.role;

    const nameSpan = document.createElement("span");
    nameSpan.className = "approval-name";
    nameSpan.textContent = s.name;

    stakeholderDiv.appendChild(roleSpan);
    stakeholderDiv.appendChild(nameSpan);

    const statusSpan = document.createElement("span");
    const pillClass = s.status === 'Approved' ? 'completed' : s.status === 'In Progress' ? 'at-risk' : 'pending';
    statusSpan.className = `status-pill ${pillClass}`;
    statusSpan.textContent = s.status;

    item.appendChild(stakeholderDiv);
    item.appendChild(statusSpan);
    listContainer.appendChild(item);
  });
}

export async function renderDashboardActivity() {
  const listContainer = document.getElementById("dashboard-activity-list");
  if (!listContainer) return;
  listContainer.textContent = "";

  const changes = await fetchRecentChanges();
  const limit = changes.slice(0, 3);

  if (limit.length === 0) {
    listContainer.innerHTML = `<div style="color:var(--text-muted);font-size:12px;">No recent activity.</div>`;
    return;
  }

  limit.forEach(c => {
    const item = document.createElement("div");
    item.className = "dashboard-activity-item";

    const msgDiv = document.createElement("div");
    msgDiv.className = "activity-item-msg";
    msgDiv.textContent = c.message;

    const timeDiv = document.createElement("div");
    timeDiv.className = "activity-item-time";
    timeDiv.textContent = `${c.author} · ${c.date.split(" ")[0]}`;

    item.appendChild(msgDiv);
    item.appendChild(timeDiv);
    listContainer.appendChild(item);
  });
}

export function renderUptimeAndReadiness() {
  const { cachedIndex } = state;
  const score = cachedIndex.readiness_score || 0;

  const drVal = document.getElementById("deployment-readiness-val");
  const drPct = document.getElementById("deployment-readiness-pct");
  const drBar = document.getElementById("deployment-readiness-bar");

  if (drVal) drVal.textContent = `${score}%`;
  if (drPct) drPct.textContent = score >= 80 ? "on track" : score >= 50 ? "at risk" : "needs work";
  if (drBar) {
    drBar.style.width = `${score}%`;
    drBar.className = "progress-bar-fill";
    if (score >= 90) drBar.classList.add("complete");
    else if (score >= 50) drBar.classList.add("at-risk");
  }

  const uptimeVal = document.getElementById("service-uptime-val");
  if (uptimeVal) uptimeVal.textContent = "99.96%";

  const sparkline = document.getElementById("uptime-sparkline");
  if (sparkline) {
    const points = [{x:0,y:12},{x:15,y:15},{x:30,y:8},{x:45,y:18},{x:60,y:5},{x:75,y:10},{x:90,y:12},{x:100,y:6}];

    // Top line path
    let pathD = `M ${points[0].x} ${points[0].y}`;
    for (let i = 1; i < points.length; i++) pathD += ` L ${points[i].x} ${points[i].y}`;

    // Filled area path closed at the bottom (y=30)
    let fillD = `M 0 30 L ${points[0].x} ${points[0].y}`;
    for (let i = 1; i < points.length; i++) fillD += ` L ${points[i].x} ${points[i].y}`;
    fillD += ` L 100 30 Z`;

    const lineEl = sparkline.querySelector(".sparkline-line") || sparkline.querySelector("path:last-of-type") || sparkline.querySelector("path");
    const fillEl = sparkline.querySelector(".sparkline-fill") || sparkline.querySelector("path:first-of-type");

    if (lineEl) {
      lineEl.setAttribute("d", pathD);
    }
    if (fillEl) {
      fillEl.setAttribute("d", fillD);
    }
  }
}

export function renderTerminalHeader() {
  const { cachedIndex } = state;
  const checksEl = document.getElementById("terminal-checks-passed");
  const readinessEl = document.getElementById("terminal-readiness-val");
  const statusLineEl = document.getElementById("terminal-status-line");

  if (!checksEl || !readinessEl || !statusLineEl) return;

  const gates = cachedIndex.go_no_go_gates || [];
  const total = gates.length;
  const passed = gates.filter(g => g.status === "checked").length;
  checksEl.textContent = `${passed}/${total}`;

  const score = cachedIndex.readiness_score || 0;
  const openBlockers = cachedIndex.open_blockers || 0;

  statusLineEl.className = "terminal-line status-line";
  readinessEl.className = "stat-value";

  if (total === 0) {
    statusLineEl.textContent = "Vault loaded — no gates configured.";
    readinessEl.textContent = score > 0 ? `${score}%` : "—";
  } else if (passed === total && openBlockers === 0) {
    statusLineEl.classList.add("success");
    statusLineEl.textContent = "All systems go. Ready to ship.";
    readinessEl.classList.add("positive");
    readinessEl.textContent = `${score}%`;
  } else if (openBlockers > 0) {
    statusLineEl.classList.add("warning");
    statusLineEl.textContent = "Launch is at risk. Resolve open items before shipping.";
    readinessEl.classList.add("warning");
    readinessEl.textContent = `${score}%`;
  } else {
    statusLineEl.classList.add("warning");
    statusLineEl.textContent = "Gates pending. Complete remaining checks before launch.";
    readinessEl.classList.add("warning");
    readinessEl.textContent = `${score}%`;
  }
}

export async function updateCiStatusWidget() {
  let configured = false;

  const providerEl = document.getElementById("ci-status-provider");
  const fileEl = document.getElementById("ci-file-path");

  try {
    const ci = await getCIStatus();
    configured = ci.configured;
    if (providerEl) providerEl.textContent = ci.provider !== "none" ? ci.provider.toUpperCase() : "None";
    if (fileEl) fileEl.textContent = ci.file_path || "-";
  } catch (err) {
    if (providerEl) providerEl.textContent = "—";
    if (fileEl) fileEl.textContent = "-";
  }

  const ciStatusVal = document.getElementById("ci-status-val");
  if (ciStatusVal) {
    ciStatusVal.textContent = configured ? "Active" : "Not Detected";
    ciStatusVal.className = configured ? "stat-val positive" : "stat-val alert";
  }
}

export function renderGatesTable() {
  const { cachedIndex } = state;
  const tbody = document.getElementById("go-no-go-table-body");
  if (!tbody) return;
  tbody.textContent = "";

  const gates = cachedIndex.go_no_go_gates || [];
  const total = gates.length;
  const passed = gates.filter(g => g.status === "checked").length;

  const progressText = document.getElementById("checklist-progress-text");
  const progressBar = document.getElementById("checklist-progress-bar");
  if (progressText) progressText.textContent = `${passed} of ${total} gates complete`;
  if (progressBar) progressBar.style.width = total > 0 ? `${Math.round((passed / total) * 100)}%` : "0%";

  gates.forEach(gate => {
    const { due, updated } = getGateDates(gate.name);
    const statusLabel = gate.status === "checked" ? "Passed" : gate.status === "partial" ? "In Progress" : "Pending";
    const statusClass = gate.status === "checked" ? "completed" : gate.status === "partial" ? "at-risk" : "pending";

    const tr = document.createElement("tr");

    const gateTd = document.createElement("td");
    const gateWrap = document.createElement("label");
    gateWrap.className = "gate-toggle-cell";
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.className = "checklist-checkbox";
    checkbox.checked = gate.status === "checked";
    const gateText = document.createElement("span");
    parseTextBoldFormatting(gateText, stripComments(gate.name));
    gateWrap.appendChild(checkbox);
    gateWrap.appendChild(gateText);
    gateTd.appendChild(gateWrap);

    const statusTd = document.createElement("td");
    const pill = document.createElement("span");
    pill.className = `status-pill ${statusClass}`;
    pill.textContent = statusLabel;
    statusTd.appendChild(pill);

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
      renderTerminalHeader();
      renderHeader();
      renderGatesTableProgress();

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
        renderTerminalHeader();
        renderHeader();
        renderGatesTableProgress();
      }
    });

    const ownerTd = document.createElement("td");
    ownerTd.textContent = gate.owner || "—";

    const dueTd = document.createElement("td");
    dueTd.style.cssText = "font-size:11px;color:var(--text-muted);font-family:var(--font-mono);";
    dueTd.textContent = due;

    const updatedTd = document.createElement("td");
    updatedTd.style.cssText = "font-size:11px;color:var(--text-muted);font-family:var(--font-mono);";
    updatedTd.textContent = updated;

    tr.appendChild(gateTd);
    tr.appendChild(statusTd);
    tr.appendChild(ownerTd);
    tr.appendChild(dueTd);
    tr.appendChild(updatedTd);
    tr.appendChild(document.createElement("td")); // action column placeholder
    tbody.appendChild(tr);
  });
}

function renderGatesTableProgress() {
  const gates = state.cachedIndex.go_no_go_gates || [];
  const total = gates.length;
  const passed = gates.filter(g => g.status === "checked").length;
  const progressText = document.getElementById("checklist-progress-text");
  const progressBar = document.getElementById("checklist-progress-bar");
  if (progressText) progressText.textContent = `${passed} of ${total} gates complete`;
  if (progressBar) progressBar.style.width = total > 0 ? `${Math.round((passed / total) * 100)}%` : "0%";
}

export function renderHeader() {
  const { cachedIndex } = state;

  if (cachedIndex.active_environment && !state.activeEnv) {
    state.activeEnv = cachedIndex.active_environment;
  }

  const releaseVer = document.getElementById("header-release-version");
  if (releaseVer) {
    const projectName = cachedIndex.vault_name
      ? cachedIndex.vault_name.replace(/\s*\(?\s*v?\d+\.\d+\.\d+\s*\)?/gi, " ").replace(/\s*\([^)]*\)/g, "").replace(/\s+/g, " ").trim()
      : "Project";
    releaseVer.textContent = `${projectName} v${cachedIndex.release || "—"}`;
  }

  const envSelect = document.getElementById("header-release-env-select");
  if (envSelect) {
    const activeEnv = state.activeEnv || "production";

    // Dynamically build environment dropdown options from cachedIndex.environment_results
    const envResults = cachedIndex.environment_results || {};
    const envKeys = Object.keys(envResults).sort();
    if (envKeys.length === 0 && !envKeys.includes(activeEnv)) {
      envKeys.push(activeEnv);
    }

    const currentOptions = Array.from(envSelect.options).map(o => o.value);
    const keysStr = envKeys.join(",");
    const currStr = currentOptions.sort().join(",");

    if (keysStr !== currStr || envSelect.options.length === 0) {
      envSelect.innerHTML = "";
      envKeys.forEach(k => {
        const opt = document.createElement("option");
        opt.value = k;
        opt.textContent = k;
        envSelect.appendChild(opt);
      });
    }

    envSelect.value = activeEnv;

    // Style select according to activeEnv class structure
    envSelect.className = "release-env " + activeEnv.toLowerCase();

    if (!envSelect.dataset.listenerBound) {
      envSelect.dataset.listenerBound = "true";
      envSelect.addEventListener("change", (e) => {
        const selectedEnv = e.target.value;
        state.activeEnv = selectedEnv;
        envSelect.className = "release-env " + selectedEnv.toLowerCase();

        const sourceIndex = state.realIndexRaw || state.realIndex || state.cachedIndex;
        if (sourceIndex) {
          const newIndex = normalizeIndexPayload(sourceIndex, selectedEnv);
          setCachedIndex(newIndex);

          renderHeader();
          if (state.activeView === "dashboard") {
            renderDashboard();
          } else if (state.activeView === "gonogo") {
            import("./gonogo.js").then(({ renderGoNoGo }) => {
              renderGoNoGo();
            });
          }
        }
      });
    }
  }

  const statusText = document.getElementById("readiness-status-text");
  const launchIndicator = document.getElementById("launch-status-indicator");
  if (statusText) {
    const r = cachedIndex.readiness || "At Risk";
    const { readinessStatusClass, statusLabel } = getReadinessMeta(r);
    statusText.textContent = statusLabel;
    if (launchIndicator) {
      launchIndicator.className = "launch-status-indicator " + (readinessStatusClass === "go" ? "go" : readinessStatusClass === "no-go" ? "no-go" : "pending");
    }
  }

  const lastUpdated = document.getElementById("last-updated-time");
  if (lastUpdated) {
    const ts = cachedIndex.last_updated;
    if (ts) {
      const d = new Date(typeof ts === "number" ? ts : parseInt(ts, 10));
      lastUpdated.textContent = `Last updated ${d.toLocaleTimeString()}`;
    }
  }
}

async function renderDashboardWaivers() {
  const container = document.getElementById("dashboard-waivers-list");
  if (!container) return;
  container.textContent = "";

  let active = [];
  try {
    const list = await getWaivers();
    active = (list || []).filter(w => w.status === "active");
  } catch (err) {
    console.warn("Failed to fetch waivers for dashboard:", err);
  }

  if (active.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state text-muted";
    empty.style.cssText = "font-size: 11px; padding: 12px; text-align: center;";
    empty.textContent = "No active waivers.";
    container.appendChild(empty);
    return;
  }

  active.forEach(w => {
    const el = document.createElement("div");
    el.className = "action-item";
    el.style.cssText = "padding: 8px 12px; margin-bottom: 6px; display: flex; justify-content: space-between; align-items: center;";

    const content = document.createElement("div");
    content.style.flex = "1";

    const title = document.createElement("div");
    title.style.cssText = "font-size: 12px; font-weight: 600; color: var(--text-primary);";
    title.innerHTML = `🛡 ${w.id}: ${w.target_type}`;

    const reason = document.createElement("div");
    reason.style.cssText = "font-size: 10.5px; color: var(--text-muted); margin-top: 2px;";
    reason.textContent = w.reason;

    content.appendChild(title);
    content.appendChild(reason);

    const btn = document.createElement("button");
    btn.className = "btn-action-mini resolve";
    btn.innerHTML = "Expire";
    btn.style.marginLeft = "8px";
    btn.addEventListener("click", async (e) => {
      e.stopPropagation();
      await expireWaiver(w.id);
    });

    el.appendChild(content);
    el.appendChild(btn);
    container.appendChild(el);
  });
}

async function expireWaiver(id) {
  try {
    const content = await loadFileContent("waivers.json");
    if (!content) return;
    let waivers = JSON.parse(content);
    waivers = waivers.map(w => w.id === id ? { ...w, status: "expired" } : w);
    await saveFileContent("waivers.json", JSON.stringify(waivers, null, 2));
    if (window.refreshUI) await window.refreshUI();
  } catch (err) {
    alert(`Failed to expire waiver: ${err.message || err}`);
  }
}

async function renderDashboardReleaseManifest() {
  const container = document.getElementById("dashboard-manifest-summary");
  if (!container) return;
  container.textContent = "";

  let manifest = null;
  try {
    manifest = await getReleaseManifest();
  } catch (err) {
    console.warn("Failed to fetch release manifest for dashboard:", err);
  }

  if (!manifest) {
    const empty = document.createElement("div");
    empty.className = "empty-state text-muted";
    empty.style.cssText = "font-size: 11px; padding: 12px; text-align: center;";
    empty.textContent = "No release manifest compiled. Run gantry index.";
    container.appendChild(empty);
    return;
  }

  const items = [
    { label: "Release Target", val: manifest.release },
    { label: "Git Commit Ref", val: manifest.commit ? manifest.commit.substring(0, 7) : "—" },
    { label: "Status & Score", val: `${manifest.readiness} (${manifest.readiness_score || 0}%)` },
    { label: "Active Blockers", val: manifest.included_blockers ? manifest.included_blockers.length : 0 },
    { label: "QA Plans Covered", val: manifest.completed_qa ? manifest.completed_qa.length : 0 },
    { label: "Waivers Active", val: manifest.waivers ? manifest.waivers.length : 0 },
  ];

  const grid = document.createElement("div");
  grid.style.cssText = "display: grid; grid-template-columns: 1fr 1fr; gap: 8px; font-size: 11px;";

  items.forEach(it => {
    const widget = document.createElement("div");
    widget.style.cssText = "background: rgba(255,255,255,0.02); border: 1px solid var(--border-glass); border-radius: 4px; padding: 6px 8px;";

    const label = document.createElement("div");
    label.style.color = "var(--text-muted)";
    label.textContent = it.label;

    const val = document.createElement("div");
    val.style.cssText = "font-weight: bold; color: var(--text-primary); margin-top: 2px;";
    val.textContent = it.val;

    widget.appendChild(label);
    widget.appendChild(val);
    grid.appendChild(widget);
  });

  container.appendChild(grid);
}

async function renderDashboardTrends() {
  const container = document.getElementById("dashboard-trends-container");
  if (!container) return;
  container.textContent = "";

  // Title
  const title = document.createElement("div");
  title.className = "card-title";
  title.innerHTML = `📊 Readiness Score Trends`;
  container.appendChild(title);

  let trends = null;
  try {
    trends = await getTrends();
  } catch (err) {
    console.warn("Failed to fetch trends:", err);
  }

  if (!trends || !trends.dates || trends.dates.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state text-muted";
    empty.style.cssText = "font-size: 11px; padding: 24px; text-align: center;";
    empty.textContent = "No readiness trend data compiled. Make changes and run gantry audit to compile snapshots.";
    container.appendChild(empty);
    return;
  }

  const n = trends.dates.length;
  const w = 550;
  const h = 130;
  const paddingLeft = 35;
  const paddingRight = 15;
  const paddingTop = 15;
  const paddingBottom = 25;

  const getX = (idx) => {
    if (n <= 1) return paddingLeft + (w - paddingLeft - paddingRight) / 2;
    return paddingLeft + (idx / (n - 1)) * (w - paddingLeft - paddingRight);
  };

  const getY = (score) => {
    return h - paddingBottom - (score / 100) * (h - paddingTop - paddingBottom);
  };

  // Draw SVG
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", `0 0 ${w} ${h}`);
  svg.style.width = "100%";
  svg.style.height = "auto";
  svg.style.overflow = "visible";

  // Grid lines
  [0, 50, 100].forEach(level => {
    const y = getY(level);
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", paddingLeft);
    line.setAttribute("y1", y);
    line.setAttribute("x2", w - paddingRight);
    line.setAttribute("y2", y);
    line.setAttribute("stroke", "rgba(255,255,255,0.06)");
    line.setAttribute("stroke-dasharray", "3,3");
    svg.appendChild(line);

    // Label
    const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
    text.setAttribute("x", paddingLeft - 8);
    text.setAttribute("y", y + 3);
    text.setAttribute("fill", "var(--text-muted)");
    text.setAttribute("font-size", "8.5px");
    text.setAttribute("text-anchor", "end");
    text.textContent = level;
    svg.appendChild(text);
  });

  // Score line path
  let pathD = "";
  let areaD = `M ${paddingLeft} ${h - paddingBottom} `;

  trends.scores.forEach((score, idx) => {
    const x = getX(idx);
    const y = getY(score);
    if (idx === 0) {
      pathD += `M ${x} ${y} `;
    } else {
      pathD += `L ${x} ${y} `;
    }
    areaD += `L ${x} ${y} `;
  });
  areaD += `L ${getX(n - 1)} ${h - paddingBottom} Z`;

  // Draw Area under line (Gradient)
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  const gradient = document.createElementNS("http://www.w3.org/2000/svg", "linearGradient");
  gradient.setAttribute("id", "trends-grad");
  gradient.setAttribute("x1", "0%");
  gradient.setAttribute("y1", "0%");
  gradient.setAttribute("x2", "0%");
  gradient.setAttribute("y2", "100%");

  const stop1 = document.createElementNS("http://www.w3.org/2000/svg", "stop");
  stop1.setAttribute("offset", "0%");
  stop1.setAttribute("stop-color", "#3b82f6");
  stop1.setAttribute("stop-opacity", "0.22");

  const stop2 = document.createElementNS("http://www.w3.org/2000/svg", "stop");
  stop2.setAttribute("offset", "100%");
  stop2.setAttribute("stop-color", "#3b82f6");
  stop2.setAttribute("stop-opacity", "0.0");

  gradient.appendChild(stop1);
  gradient.appendChild(stop2);
  defs.appendChild(gradient);
  svg.appendChild(defs);

  const areaPath = document.createElementNS("http://www.w3.org/2000/svg", "path");
  areaPath.setAttribute("d", areaD);
  areaPath.setAttribute("fill", "url(#trends-grad)");
  svg.appendChild(areaPath);

  // Draw Main Line
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", pathD);
  path.setAttribute("fill", "none");
  path.setAttribute("stroke", "#3b82f6");
  path.setAttribute("stroke-width", "2");
  svg.appendChild(path);

  // Draw dots and date labels
  trends.scores.forEach((score, idx) => {
    const x = getX(idx);
    const y = getY(score);

    // Dot
    const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    circle.setAttribute("cx", x);
    circle.setAttribute("cy", y);
    circle.setAttribute("r", "3.5");
    circle.setAttribute("fill", "#3b82f6");
    circle.setAttribute("stroke", "#1f2937");
    circle.setAttribute("stroke-width", "1.5");

    // Tooltip title
    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
    title.textContent = `Date: ${trends.dates[idx]}\nScore: ${score}%\nState: ${trends.readiness_states[idx]}`;
    circle.appendChild(title);

    svg.appendChild(circle);

    // Label
    if (n < 8 || idx % Math.ceil(n / 6) === 0 || idx === n - 1) {
      const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
      text.setAttribute("x", x);
      text.setAttribute("y", h - 8);
      text.setAttribute("fill", "var(--text-muted)");
      text.setAttribute("font-size", "8px");
      text.setAttribute("text-anchor", "middle");
      const datePart = trends.dates[idx].substring(5); // e.g. "06-05"
      text.textContent = datePart;
      svg.appendChild(text);
    }
  });

  container.appendChild(svg);
}
