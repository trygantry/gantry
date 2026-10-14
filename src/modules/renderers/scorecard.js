/**
 * Renders the OpsLevel/Cortex-style scorecard view based on category scores and reasons.
 * 
 * @param {HTMLElement} containerEl - Container element
 * @param {import('../contract.js').IndexPayload} payload - Normalized index payload
 */
export function renderScorecard(containerEl, payload) {
  if (!containerEl) return;
  containerEl.textContent = "";

  const container = document.createElement("div");
  container.className = "scorecard-container";

  const categories = payload.category_scores || {};
  const reasonsMap = payload.score_reasons || {};

  const sortedCategories = Object.keys(categories).sort();

  if (sortedCategories.length === 0) {
    const empty = document.createElement("div");
    empty.style.cssText = "color: var(--text-muted); font-size: 12px; padding: 12px; text-align: center;";
    empty.textContent = "No category scorecard data available.";
    container.appendChild(empty);
    containerEl.appendChild(container);
    return;
  }

  sortedCategories.forEach(category => {
    const score = categories[category] ?? 0;
    
    // Status color class
    let statusClass = "go";
    let statusLabel = "PASS";
    if (score < 50) {
      statusClass = "no-go";
      statusLabel = "FAIL";
    } else if (score < 90) {
      statusClass = "warning";
      statusLabel = "WARN";
    }

    const row = document.createElement("div");
    row.className = "scorecard-row";

    // Category Name
    const catEl = document.createElement("div");
    catEl.className = "scorecard-category";
    catEl.textContent = category;

    // Score
    const scoreWrapper = document.createElement("div");
    scoreWrapper.className = "scorecard-score-wrapper";
    
    const scoreVal = document.createElement("span");
    scoreVal.className = `scorecard-score ${statusClass}`;
    scoreVal.textContent = `${score}%`;
    
    const scoreMax = document.createElement("span");
    scoreMax.style.cssText = "color: var(--text-muted); font-size: 10px;";
    scoreMax.textContent = "/100";
    
    scoreWrapper.appendChild(scoreVal);
    scoreWrapper.appendChild(scoreMax);

    // Status Label Badge
    const statusBadge = document.createElement("div");
    statusBadge.className = `scorecard-status ${statusClass}`;
    statusBadge.textContent = statusLabel;

    // Reasons / Notes
    const reasonsEl = document.createElement("div");
    reasonsEl.className = "scorecard-reasons";
    const reasons = reasonsMap[category] || [];
    if (reasons.length > 0) {
      reasonsEl.textContent = reasons.join("; ");
      reasonsEl.title = reasons.join("\n");
      reasonsEl.style.color = "var(--color-pending)";
    } else {
      reasonsEl.textContent = "All checks nominal.";
      reasonsEl.style.color = "var(--text-muted)";
    }

    // Optional Action Link
    let hasAction = false;
    let actionText = "View Details";
    let actionFn = () => {};

    if (category === "Engineering" && payload.blockers && payload.blockers.some(b => b.status !== "resolved")) {
      hasAction = true;
      actionText = "Resolve Blockers";
      actionFn = () => window.switchView("blockers");
    } else if (category === "QA" && payload.qa_suites && payload.qa_suites.length > 0) {
      hasAction = true;
      actionText = "Complete QA Checklist";
      actionFn = () => window.switchView("qa");
    } else if ((category === "Docs" || category === "Documentation") && 
               (payload.stale_docs?.length > 0 || payload.missing_docs?.length > 0 || 
                payload.validation_errors?.length > 0 || payload.broken_links?.length > 0)) {
      hasAction = true;
      actionText = "Review Docs Health";
      actionFn = () => window.switchView("health");
    } else if (category === "Product" && (payload.go_no_go_gates?.length > 0 || payload.signoffs?.length > 0)) {
      hasAction = true;
      actionText = "Check Sign-offs";
      actionFn = () => window.switchView("gonogo");
    } else if (category === "Monitoring" && payload.telemetry_status && payload.telemetry_status !== "not_required") {
      hasAction = true;
      actionText = "Check Telemetry";
      actionFn = () => window.switchView("dashboard");
    }

    row.appendChild(catEl);
    row.appendChild(scoreWrapper);
    row.appendChild(statusBadge);
    row.appendChild(reasonsEl);

    if (hasAction) {
      const actionEl = document.createElement("div");
      actionEl.className = "scorecard-action";
      actionEl.textContent = actionText;
      actionEl.addEventListener("click", (e) => {
        e.stopPropagation();
        actionFn();
      });
      row.appendChild(actionEl);
    } else {
      // Empty placeholder to maintain alignment
      const placeholder = document.createElement("div");
      placeholder.style.cssText = "width: 120px; flex-shrink: 0;";
      row.appendChild(placeholder);
    }

    container.appendChild(row);
  });

  containerEl.appendChild(container);
}
