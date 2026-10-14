import { state } from '../state.js';
import { parseTextBoldFormatting, stripComments } from '../utils.js';
import { hasTauri, runAudit, openInIde, listPendingTasks, completeTaskById, ignoreTask, getImpactSummary, openProjectFile } from '../api.js';
import { drawGraph } from '../graph_view.js';
import { renderScorecard } from './scorecard.js';

export async function renderAuditCockpit() {
  const { cachedIndex } = state;
  const scoreValue = document.getElementById("audit-score-value");
  const statusText = document.getElementById("audit-status-text");
  const categoriesGrid = document.getElementById("audit-categories-grid");
  const tasksList = document.getElementById("audit-tasks-list");
  const evidenceList = document.getElementById("audit-evidence-list");

  if (!scoreValue || !statusText || !categoriesGrid || !tasksList || !evidenceList) return;

  const score = cachedIndex.readiness_score || 0;
  scoreValue.textContent = `${score}%`;

  // Update progress-ring circle filling dynamically
  const circle = document.querySelector(".progress-ring__circle");
  if (circle) {
    const circumference = 439.82; // 2 * PI * r (r=70)
    const offset = circumference - (score / 100) * circumference;
    circle.style.strokeDashoffset = offset;

    // Apply color-coding to the progress ring's stroke
    if (score >= 90) {
      circle.style.stroke = "#10B981"; // Go (healthy green)
    } else if (score >= 70) {
      circle.style.stroke = "#F59E0B"; // Pending (warning yellow)
    } else {
      circle.style.stroke = "#EF4444"; // Blocker (critical red)
    }
  }

  // Color-code the centered score text and update bottom status badge
  // Retaining the 'gauge-value' class is critical to center the text inside the ring
  if (score >= 90) {
    scoreValue.className = "gauge-value healthy text-go";
    statusText.textContent = "EXCELLENT";
    statusText.className = "audit-status-badge text-go";
  } else if (score >= 70) {
    scoreValue.className = "gauge-value warning text-pending";
    statusText.textContent = "GOOD";
    statusText.className = "audit-status-badge text-pending";
  } else {
    scoreValue.className = "gauge-value critical text-blocker";
    statusText.textContent = "NEEDS WORK";
    statusText.className = "audit-status-badge text-blocker";
  }

  // Render categories using the unified scorecard component
  renderScorecard(categoriesGrid, cachedIndex);

  // Render tasks
  tasksList.textContent = "";
  let tasks = [];
  try {
    tasks = await listPendingTasks();
  } catch(e) {
    console.warn("Failed to list pending tasks:", e);
  }

  const taskGroups = { "Blockers": [], "Unit Tests": [], "Documentation": [], "Gates": [], "Code Coverage": [], "General": [] };
  tasks.forEach(task => {
    const id = task.id || "";
    const source = task.source || "";
    if (id.startsWith("blocker-") || source === "blocker") taskGroups["Blockers"].push(task);
    else if (id.startsWith("failed-test-") || source === "test_report") taskGroups["Unit Tests"].push(task);
    else if (id.startsWith("missing-doc-") || id.startsWith("stale-doc-") || id.startsWith("validation-err-") || id.startsWith("broken-link-")) taskGroups["Documentation"].push(task);
    else if (id.startsWith("gate-")) taskGroups["Gates"].push(task);
    else if (id.startsWith("uncovered-change-")) taskGroups["Code Coverage"].push(task);
    else taskGroups["General"].push(task);
  });

  let totalTasksRendered = 0;
  Object.keys(taskGroups).forEach(groupName => {
    const list = taskGroups[groupName];
    if (list.length === 0) return;
    totalTasksRendered += list.length;

    const groupHeader = document.createElement("div");
    groupHeader.className = "audit-task-group-header";
    groupHeader.textContent = groupName;
    tasksList.appendChild(groupHeader);

    list.forEach(task => {
      const itemEl = document.createElement("div");
      itemEl.className = "audit-task-item";

      const row1 = document.createElement("div");
      row1.className = "task-row-header";

      const leftEl = document.createElement("div");
      leftEl.className = "task-row-left";

      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.className = "audit-task-item-checkbox";
      checkbox.checked = task.completed;
      checkbox.addEventListener("change", async () => {
        if (checkbox.checked) {
          try {
            await completeTaskById(task.id);
          } catch(e) {
            task.completed = true;
            task.status = "manually_completed";
          }
        }
        if (window.refreshUI) await window.refreshUI();
      });

      const textEl = document.createElement("span");
      textEl.className = "audit-task-item-text";
      if (task.completed) {
        textEl.classList.add("completed");
      }
      const cleanTitle = stripComments(task.title);
      parseTextBoldFormatting(textEl, cleanTitle);

      leftEl.appendChild(checkbox);
      leftEl.appendChild(textEl);
      row1.appendChild(leftEl);

      const actionsEl = document.createElement("div");
      actionsEl.className = "task-row-actions";

      if (!task.completed && task.status !== "ignored") {
        const ignoreBtn = document.createElement("button");
        ignoreBtn.className = "btn btn-xs";
        ignoreBtn.textContent = "Ignore";
        ignoreBtn.addEventListener("click", async () => {
          const reason = prompt(`Enter reason to ignore task: ${cleanTitle}`);
          if (reason) {
            try {
              await ignoreTask(task.id, reason);
            } catch(e) {
              task.completed = true;
              task.status = "ignored";
              task.reason = reason;
            }
            if (window.refreshUI) await window.refreshUI();
          }
        });
        actionsEl.appendChild(ignoreBtn);
      }
      row1.appendChild(actionsEl);
      itemEl.appendChild(row1);

      const badgesEl = document.createElement("div");
      badgesEl.className = "task-row-badges";

      const sevBadge = document.createElement("span");
      sevBadge.className = `task-badge severity-${task.severity.toLowerCase()}`;
      sevBadge.textContent = task.severity;
      badgesEl.appendChild(sevBadge);

      if (task.status && task.status !== "pending") {
        const statBadge = document.createElement("span");
        statBadge.className = `task-badge status-${task.status.replace("_", "-")}`;
        if (task.status === "ignored") {
          statBadge.textContent = "Ignored";
        } else if (task.status === "manually_completed") {
          statBadge.textContent = "Manual Complete";
        } else if (task.status === "fixed") {
          statBadge.textContent = "Auto Fixed";
        }
        badgesEl.appendChild(statBadge);
      }

      if (task.reason) {
        const reasonEl = document.createElement("span");
        reasonEl.className = "task-row-reason";
        reasonEl.textContent = `Reason: ${task.reason}`;
        badgesEl.appendChild(reasonEl);
      }

      itemEl.appendChild(badgesEl);
      tasksList.appendChild(itemEl);
    });
  });

  if (totalTasksRendered === 0) {
    const p = document.createElement("p");
    p.className = "empty-state";
    p.style.color = "var(--text-secondary)";
    p.textContent = "No tasks generated yet. Run audit to populate.";
    tasksList.appendChild(p);
  }

  // Render evidence graph
  evidenceList.textContent = "";
  const evGraph = cachedIndex.evidence_graph || [];

  if (evGraph.length === 0) {
    const p = document.createElement("p");
    p.className = "empty-state";
    p.style.color = "var(--text-secondary)";
    p.textContent = "No warnings or issues found. All systems nominal.";
    evidenceList.appendChild(p);
  } else {
    const kindsMapping = {
      "doc": "Documentation & Links",
      "source_file": "Source Code & File Paths",
      "git_commit": "Git Intelligence & Commit History",
      "test_report": "Test Reports & Ingestion",
      "telemetry": "Production Telemetry",
      "signoff": "Launch Gates & Sign-offs",
      "config": "Project Configuration"
    };

    const grouped = {};
    evGraph.forEach(ev => {
      const k = ev.kind || "unknown";
      if (!grouped[k]) grouped[k] = [];
      grouped[k].push(ev);
    });
    Object.keys(grouped).sort().forEach(kind => {
      const kindHeader = document.createElement("div");
      kindHeader.className = "audit-evidence-group-header";
      kindHeader.textContent = kindsMapping[kind] || `${kind.toUpperCase()} Evidence`;
      evidenceList.appendChild(kindHeader);

      const items = grouped[kind].sort((a, b) => {
        if (a.severity === "error" && b.severity !== "error") return -1;
        if (a.severity !== "error" && b.severity === "error") return 1;
        return 0;
      });

      items.forEach(ev => {
        const itemEl = document.createElement("div");
        itemEl.className = "audit-evidence-item";

        const row1 = document.createElement("div");
        row1.className = "evidence-row-header";

        const leftEl = document.createElement("div");
        leftEl.className = "evidence-row-left";

        const dot = document.createElement("div");
        dot.className = `audit-evidence-dot ${ev.severity === "error" ? "critical" : "warning"}`;

        const msg = document.createElement("span");
        msg.className = "audit-evidence-msg";
        msg.textContent = ev.message;

        leftEl.appendChild(dot);
        leftEl.appendChild(msg);

        const fileLink = document.createElement("span");
        fileLink.className = "audit-evidence-file";
        fileLink.textContent = ev.path || "";

        row1.appendChild(leftEl);
        row1.appendChild(fileLink);
        itemEl.appendChild(row1);

        const badgesContainer = document.createElement("div");
        badgesContainer.className = "evidence-row-badges";

        const addBadge = (text, typeClass, mono = false) => {
          const badge = document.createElement("span");
          badge.className = `evidence-badge badge-${typeClass}${mono ? " mono" : ""}`;
          badge.textContent = text;
          badgesContainer.appendChild(badge);
        };

        if (ev.confidence !== undefined && ev.confidence !== null)
          addBadge(`Confidence: ${Math.round(ev.confidence * 100)}%`, "go");
        if (ev.owner)
          addBadge(`Owner: ${ev.owner}`, "blue");
        if (ev.source_commit)
          addBadge(`Commit: ${ev.source_commit.substring(0, 7)}`, "primary", true);
        if (ev.test_name)
          addBadge(`Test: ${ev.test_name}`, "blocker");
        if (ev.line_start !== undefined && ev.line_start !== null) {
          const lineText = (ev.line_end !== undefined && ev.line_end !== null && ev.line_end !== ev.line_start)
            ? `Lines: ${ev.line_start}-${ev.line_end}` : `Line: ${ev.line_start}`;
          addBadge(lineText, "muted");
        }
        if (ev.remediation_task_id)
          addBadge(`Task ID: ${ev.remediation_task_id}`, "pending");

        if (badgesContainer.children.length > 0) itemEl.appendChild(badgesContainer);

        if (ev.suggested_action) {
          const actionEl = document.createElement("div");
          actionEl.className = "evidence-row-action";
          actionEl.textContent = `[ACTION] ${ev.suggested_action}`;
          itemEl.appendChild(actionEl);
        }

        if (ev.path) {
          fileLink.style.cursor = "pointer";
          fileLink.addEventListener("click", async (e) => {
            e.stopPropagation();
            if (hasTauri) {
              try { await openInIde(ev.path); }
              catch (err) { alert(`Failed to open in IDE: ${err}`); }
            } else {
              alert(`Tauri CLI simulator triggered: Open ${ev.path} in IDE.`);
            }
          });
        }

        evidenceList.appendChild(itemEl);
      });
    });
  }

  await renderImpactTrace();
  renderDetailedScorecardRules();
}

async function renderImpactTrace() {
  const container = document.getElementById("audit-impact-list");
  if (!container) return;
  
  container.textContent = "";
  
  let summary = state.cachedIndex.impact_summary;
  if (!summary) {
    try {
      summary = await getImpactSummary();
    } catch (e) {
      console.warn("Failed to fetch impact summary:", e);
    }
  }
  
  if (!summary || !summary.changed_files || summary.changed_files.length === 0) {
    container.innerHTML = `<p class="empty-state">No changes detected since last release tag.</p>`;
    return;
  }
  
  const blockers = state.cachedIndex.blockers || [];
  const qaSuites = state.cachedIndex.qa_suites || [];
  
  summary.changed_files.forEach(file => {
    const filename = file.split("/").pop();
    
    // Find matching blockers
    const matchedBlockers = blockers.filter(b => b.source_files && b.source_files.includes(file));
    // Find matching QA suites
    const matchedQAs = qaSuites.filter(q => q.source_files && q.source_files.includes(file));
    
    // If no match, render uncovered trace
    if (matchedBlockers.length === 0 && matchedQAs.length === 0) {
      const row = document.createElement("div");
      row.className = "trace-row";
      
      const fileNode = document.createElement("div");
      fileNode.className = "trace-node file";
      fileNode.title = `Click to open ${file} in IDE`;
      fileNode.textContent = `[FILE] ${filename}`;
      fileNode.addEventListener("click", () => openProjectFile(file));
      
      const arrow1 = document.createElement("div");
      arrow1.className = "trace-arrow";
      arrow1.textContent = "➔";
      
      const qaNode = document.createElement("div");
      qaNode.className = "trace-node qa uncovered";
      qaNode.title = "No QA plan covers this changed file!";
      qaNode.textContent = `[UNCOVERED]`;
      
      const arrow2 = document.createElement("div");
      arrow2.className = "trace-arrow";
      arrow2.textContent = "➔";
      
      const blockerNode = document.createElement("div");
      blockerNode.className = "trace-node blocker empty";
      blockerNode.textContent = "──";
      
      const arrow3 = document.createElement("div");
      arrow3.className = "trace-arrow";
      arrow3.textContent = "➔";
      
      const gateNode = document.createElement("div");
      gateNode.className = "trace-node gate";
      gateNode.title = "Click to open Decision Room";
      gateNode.textContent = `[DECISION ROOM]`;
      gateNode.addEventListener("click", () => window.switchView("gonogo"));
      
      row.appendChild(fileNode);
      row.appendChild(arrow1);
      row.appendChild(qaNode);
      row.appendChild(arrow2);
      row.appendChild(blockerNode);
      row.appendChild(arrow3);
      row.appendChild(gateNode);
      container.appendChild(row);
      return;
    }
    
    // Else render combinations
    const maxLen = Math.max(matchedBlockers.length, matchedQAs.length);
    for (let i = 0; i < maxLen; i++) {
      const row = document.createElement("div");
      row.className = "trace-row";
      
      const fileNode = document.createElement("div");
      fileNode.className = "trace-node file";
      fileNode.title = `Click to open ${file} in IDE`;
      fileNode.textContent = `[FILE] ${filename}`;
      fileNode.addEventListener("click", () => openProjectFile(file));
      
      const arrow1 = document.createElement("div");
      arrow1.className = "trace-arrow";
      arrow1.textContent = "➔";
      
      const qaSuite = matchedQAs[i] || null;
      const qaNode = document.createElement("div");
      if (qaSuite) {
        qaNode.className = "trace-node qa";
        qaNode.title = `Click to switch to QA suite: ${qaSuite.title}`;
        qaNode.textContent = `[QA] ${qaSuite.id}`;
        qaNode.addEventListener("click", () => {
          state.selectedQASuitePath = qaSuite.path;
          window.switchView("qa");
        });
      } else {
        qaNode.className = "trace-node qa empty";
        qaNode.textContent = "──";
      }
      
      const arrow2 = document.createElement("div");
      arrow2.className = "trace-arrow";
      arrow2.textContent = "➔";
      
      const blocker = matchedBlockers[i] || null;
      const blockerNode = document.createElement("div");
      if (blocker) {
        blockerNode.className = "trace-node blocker";
        blockerNode.title = `Click to view blocker: ${blocker.title}`;
        blockerNode.textContent = `[BLOCKER] ${blocker.id}`;
        blockerNode.addEventListener("click", () => window.openFileInEditor(blocker.path));
      } else {
        blockerNode.className = "trace-node blocker empty";
        blockerNode.textContent = "──";
      }
      
      const arrow3 = document.createElement("div");
      arrow3.className = "trace-arrow";
      arrow3.textContent = "➔";
      
      const gateNode = document.createElement("div");
      gateNode.className = "trace-node gate";
      gateNode.title = "Click to open Decision Room";
      gateNode.textContent = `[DECISION ROOM]`;
      gateNode.addEventListener("click", () => window.switchView("gonogo"));
      
      row.appendChild(fileNode);
      row.appendChild(arrow1);
      row.appendChild(qaNode);
      row.appendChild(arrow2);
      row.appendChild(blockerNode);
      row.appendChild(arrow3);
      row.appendChild(gateNode);
      container.appendChild(row);
    }
  });

  // Only render graph data when graph tab is visible.
  const graphContainer = document.getElementById("audit-impact-graph-container");
  if (graphContainer && graphContainer.style.display !== "none") {
    await drawGraph();
  }
}

export function renderDetailedScorecardRules() {
  const { cachedIndex } = state;
  const container = document.getElementById("audit-scorecard-rules-list");
  if (!container) return;
  container.textContent = "";

  const results = cachedIndex.scorecard_results || [];

  if (results.length === 0) {
    const p = document.createElement("p");
    p.className = "empty-state";
    p.style.color = "var(--text-secondary)";
    p.textContent = "No scorecard rules evaluated yet. Run audit to view.";
    container.appendChild(p);
    return;
  }

  results.forEach(res => {
    const itemEl = document.createElement("div");
    itemEl.className = "audit-evidence-item";
    itemEl.style.cssText = "padding: 14px; margin-bottom: 8px; display: flex; flex-direction: column; gap: 8px;";

    const header = document.createElement("div");
    header.style.cssText = "display: flex; justify-content: space-between; align-items: center; width: 100%;";

    const titleContainer = document.createElement("div");
    titleContainer.style.cssText = "display: flex; align-items: center; gap: 8px;";

    const catBadge = document.createElement("span");
    catBadge.className = "evidence-badge badge-blue";
    catBadge.style.cssText = "font-size: 9px; font-weight: 700; text-transform: uppercase;";
    catBadge.textContent = res.category;

    const ruleId = document.createElement("span");
    ruleId.style.cssText = "font-family: var(--font-mono); font-size: 11px; color: var(--text-muted);";
    ruleId.textContent = res.rule_id;

    titleContainer.appendChild(catBadge);
    titleContainer.appendChild(ruleId);

    const pill = document.createElement("span");
    if (res.waived) {
      pill.className = "status-pill pending";
      pill.style.cssText = "background: rgba(165, 180, 252, 0.1); border-color: rgba(165, 180, 252, 0.25); color: #a5b4fc; font-weight: 700;";
      pill.textContent = "Waived";
    } else if (res.passed) {
      pill.className = "status-pill completed";
      pill.textContent = "Passed";
    } else {
      pill.className = "status-pill no-go";
      pill.style.cssText = "background: rgba(239, 68, 68, 0.1); border-color: rgba(239, 68, 68, 0.2); color: var(--color-blocker); font-weight: 700;";
      pill.textContent = "Failed";
    }

    header.appendChild(titleContainer);
    header.appendChild(pill);

    const title = document.createElement("div");
    title.style.cssText = "font-size: 13.5px; font-weight: 600; color: var(--text-primary);";
    title.textContent = res.title;

    const detailsContainer = document.createElement("div");
    detailsContainer.style.cssText = "display: flex; flex-direction: column; gap: 4px; font-size: 12px;";

    if (!res.passed || res.waived) {
      if (res.failure_message) {
        const failMsg = document.createElement("div");
        failMsg.style.cssText = "color: var(--color-pending);";
        failMsg.innerHTML = `<span style="font-weight: 600;">Reason:</span> ${res.failure_message}`;
        detailsContainer.appendChild(failMsg);
      }
    }

    if (res.suggested_action) {
      const suggestMsg = document.createElement("div");
      suggestMsg.style.cssText = "color: var(--text-secondary);";
      suggestMsg.innerHTML = `<span style="font-weight: 600;">Remediation:</span> ${res.suggested_action}`;
      detailsContainer.appendChild(suggestMsg);
    }

    const footer = document.createElement("div");
    footer.style.cssText = "display: flex; justify-content: space-between; font-size: 11px; color: var(--text-muted); border-top: 1px solid rgba(255, 255, 255, 0.05); padding-top: 6px; margin-top: 2px;";

    const weightEl = document.createElement("span");
    weightEl.textContent = `Weight: ${res.weight} points`;

    const valueEl = document.createElement("span");
    valueEl.style.fontFamily = "var(--font-mono)";
    if (res.value !== undefined && res.value !== null) {
      valueEl.textContent = `Value: ${JSON.stringify(res.value)}`;
    }

    footer.appendChild(weightEl);
    footer.appendChild(valueEl);

    itemEl.appendChild(header);
    itemEl.appendChild(title);
    if (detailsContainer.children.length > 0) {
      itemEl.appendChild(detailsContainer);
    }
    itemEl.appendChild(footer);

    container.appendChild(itemEl);
  });
}
