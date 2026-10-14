import { openProjectFile } from '../api.js';

/**
 * Reusable data-driven evidence summary ledger used across Gantry views (Dashboard and Go/No-Go).
 * It aggregates blockers, gates/signoffs, QA, docs/schema, telemetry, impacted files, and snapshots/activity.
 * 
 * @param {HTMLElement} containerEl - The container element to render the summary in
 * @param {import('../contract.js').IndexPayload} payload - The normalized launch index payload
 */
export function renderEvidenceSummary(containerEl, payload) {
  if (!containerEl) return;
  containerEl.textContent = "";

  const container = document.createElement("div");
  container.className = "evidence-summary-container";

  // Heading
  const heading = document.createElement("h3");
  heading.className = "evidence-section-title";
  heading.textContent = "Evidence Ledger";
  container.appendChild(heading);

  let hasAnyEvidence = false;

  // 1. Blockers (Trace all blockers, open and resolved, for provenance)
  const blockers = payload.blockers || [];
  if (blockers.length > 0) {
    hasAnyEvidence = true;
    const group = createEvidenceGroup("Blockers");
    blockers.forEach(b => {
      const isResolved = b.status === "resolved";
      const statusType = isResolved ? "go" : (b.severity === "P0" || b.severity === "P1" ? "no-go" : "warning");
      const label = isResolved ? "RESOLVED" : b.status.toUpperCase();
      
      const row = createEvidenceRow(
        statusType,
        "BLOCKER",
        `[${b.severity}] ${b.id}: ${b.title} (${b.owner}) - ${label}`,
        b.path,
        true // vault path
      );
      group.appendChild(row);
    });
    container.appendChild(group);
  }

  // 2. Gates & Sign-offs
  const gates = payload.go_no_go_gates || [];
  const signoffs = payload.signoffs || [];
  if (gates.length > 0 || signoffs.length > 0) {
    hasAnyEvidence = true;
    const group = createEvidenceGroup("Gates & Sign-offs");
    
    gates.forEach(g => {
      const statusType = g.status === "checked" ? "go" : (g.status === "partial" ? "warning" : "pending");
      const label = g.status === "checked" ? "PASSED" : (g.status === "partial" ? "IN PROGRESS" : "PENDING");
      
      const row = createEvidenceRow(
        statusType,
        "GATE",
        `${g.name} (${g.owner || "Unassigned"}) - ${label}`,
        "go_no_go.md",
        true // vault path
      );
      group.appendChild(row);
    });

    signoffs.forEach(s => {
      const statusType = s.status === "Approved" ? "go" : (s.status === "In Progress" ? "warning" : "pending");
      const label = s.status.toUpperCase();
      
      const row = createEvidenceRow(
        statusType,
        "SIGNOFF",
        `${s.role}: ${s.name} - ${label}`,
        "go_no_go.md",
        true // vault path
      );
      group.appendChild(row);
    });

    container.appendChild(group);
  }

  // 3. QA Checklist Coverage
  const suites = payload.qa_suites || [];
  if (suites.length > 0) {
    hasAnyEvidence = true;
    const group = createEvidenceGroup("QA");
    
    suites.forEach(s => {
      const isComplete = s.completed_tests >= s.total_tests;
      const statusType = isComplete ? "go" : (s.completed_tests > 0 ? "warning" : "pending");
      const label = isComplete ? "VERIFIED" : `${s.completed_tests}/${s.total_tests} PASSED`;
      
      const row = createEvidenceRow(
        statusType,
        "QA",
        `${s.title}: ${label} (${s.owner})`,
        s.path,
        true // vault path
      );
      group.appendChild(row);
    });

    container.appendChild(group);
  }

  // 4. Docs & Schema
  const staleDocs = payload.stale_docs || [];
  const missingDocs = payload.missing_docs || [];
  const validationErrors = payload.validation_errors || [];
  const brokenLinks = payload.broken_links || [];
  const sourceMissing = payload.source_missing || [];

  if (staleDocs.length > 0 || missingDocs.length > 0 || validationErrors.length > 0 || brokenLinks.length > 0 || sourceMissing.length > 0) {
    hasAnyEvidence = true;
    const group = createEvidenceGroup("Docs & Schema");

    missingDocs.forEach(doc => {
      const row = createEvidenceRow(
        "no-go",
        "DOC",
        `Missing required document: ${doc}`,
        doc,
        true // vault path
      );
      group.appendChild(row);
    });

    staleDocs.forEach(doc => {
      const row = createEvidenceRow(
        "warning",
        "DOC",
        `Document needs re-verification: ${doc}`,
        doc,
        true // vault path
      );
      group.appendChild(row);
    });

    validationErrors.forEach(err => {
      const row = createEvidenceRow(
        err.severity === "critical" || err.severity === "error" ? "no-go" : "warning",
        "SCHEMA",
        `[${err.code}] ${err.file}: ${err.message}`,
        err.file,
        true // vault path
      );
      group.appendChild(row);
    });

    brokenLinks.forEach(link => {
      const row = createEvidenceRow(
        "warning",
        "LINK",
        `[BROKEN LINK] ${link.file}: ${link.message}`,
        link.file,
        true // vault path
      );
      group.appendChild(row);
    });

    sourceMissing.forEach(src => {
      const row = createEvidenceRow(
        "warning",
        "SOURCE",
        `[MISSING FILE] ${src.file}: ${src.message}`,
        src.file,
        true // vault path
      );
      group.appendChild(row);
    });

    container.appendChild(group);
  }

  // 5. Telemetry
  const hasTelemetry = payload.telemetry_status && payload.telemetry_status !== "not_required";
  if (hasTelemetry) {
    const status = payload.telemetry_status.toLowerCase();
    const isError = status === "missing" || status === "invalid" || status === "stale";
    const telemetry = payload.observed_telemetry;

    hasAnyEvidence = true;
    const group = createEvidenceGroup("Telemetry");

    let msg = `Telemetry Status: ${status.toUpperCase()}`;
    if (telemetry && typeof telemetry.crash_free_rate === "number") {
      msg += ` (Crash-Free Rate: ${telemetry.crash_free_rate.toFixed(2)}% via ${telemetry.source})`;
    }

    const row = createEvidenceRow(
      isError ? "no-go" : "go",
      "TELEMETRY",
      msg,
      "config.json",
      true // vault path
    );
    group.appendChild(row);
    container.appendChild(group);
  }

  // 6. Impacted Files (Kosli-style changed files provenance)
  const changedFiles = payload.impact_summary?.changed_files || [];
  if (changedFiles.length > 0) {
    hasAnyEvidence = true;
    const group = createEvidenceGroup("Impacted Files");
    
    const uncovered = payload.impact_summary?.uncovered_changes || [];

    changedFiles.forEach(file => {
      const isUncovered = uncovered.includes(file);
      const statusType = isUncovered ? "warning" : "go";
      const label = isUncovered ? "UNCOVERED" : "VERIFIED";
      
      const row = createEvidenceRow(
        statusType,
        "DIFF",
        `Modified file: ${file} [${label}]`,
        file,
        false // project source path
      );
      group.appendChild(row);
    });
    container.appendChild(group);
  }

  // 7. Snapshots & Activity
  if (payload.last_updated) {
    hasAnyEvidence = true;
    const group = createEvidenceGroup("Snapshots & Activity");
    
    let timeStr = payload.last_updated;
    try {
      const d = new Date(typeof timeStr === "number" ? timeStr : parseInt(timeStr, 10));
      if (!isNaN(d.getTime())) {
        timeStr = d.toLocaleString();
      }
    } catch (e) {}

    const row = createEvidenceRow(
      "go",
      "SNAPSHOT",
      `Last launch index compiled at ${timeStr}`,
      "index.json",
      true // vault path
    );
    group.appendChild(row);
    container.appendChild(group);
  }

  if (!hasAnyEvidence) {
    const emptyState = document.createElement("div");
    emptyState.className = "evidence-empty-state";
    emptyState.textContent = "No blocking evidence was found.";
    container.appendChild(emptyState);
  }

  containerEl.appendChild(container);
}

/**
 * Creates an evidence group panel element.
 * 
 * @param {string} title - The title of the group
 * @returns {HTMLDivElement} Group container element
 */
function createEvidenceGroup(title) {
  const group = document.createElement("div");
  group.className = "evidence-group";

  const header = document.createElement("div");
  group.className = "evidence-group";

  const headerEl = document.createElement("div");
  headerEl.className = "evidence-group-title";
  headerEl.textContent = title;

  group.appendChild(headerEl);
  return group;
}

/**
 * Creates an interactive evidence row element.
 * 
 * @param {"go"|"warning"|"no-go"|"pending"} statusType - The marker state
 * @param {string} source - The source type (e.g. BLOCKER, GATE, QA, Doc)
 * @param {string} text - Row text description
 * @param {string} path - Target path to open on click
 * @param {boolean} isVaultPath - True if a vault file, false if a project source code file
 * @returns {HTMLDivElement} Flat interactive row element
 */
function createEvidenceRow(statusType, source, text, path, isVaultPath) {
  const row = document.createElement("div");
  row.className = "evidence-row";

  // Marker Label badge
  const marker = document.createElement("span");
  marker.className = `evidence-marker ${statusType}`;
  
  let markerText = "INFO";
  if (statusType === "go") markerText = "PASS";
  else if (statusType === "warning") markerText = "WARN";
  else if (statusType === "no-go") markerText = "FAIL";
  else if (statusType === "pending") markerText = "PEND";
  marker.textContent = markerText;

  // Source Type Label
  const sourceEl = document.createElement("span");
  sourceEl.style.cssText = "font-family: var(--font-mono); font-size: 10px; font-weight: 700; color: var(--text-muted); min-width: 80px; text-transform: uppercase;";
  sourceEl.textContent = source;

  // Text container
  const content = document.createElement("span");
  content.className = "evidence-content";
  content.textContent = text;

  row.appendChild(marker);
  row.appendChild(sourceEl);
  row.appendChild(content);

  if (path) {
    // Click target target indication chevron
    const chevron = document.createElement("span");
    chevron.className = "evidence-chevron";
    chevron.textContent = "→";
    row.appendChild(chevron);

    // Keyboard navigation support
    row.setAttribute("role", "button");
    row.setAttribute("tabindex", "0");

    const clickHandler = () => {
      if (isVaultPath) {
        if (window.openFileInEditor) {
          window.openFileInEditor(path);
        }
      } else {
        openProjectFile(path);
      }
    };

    row.addEventListener("click", clickHandler);
    row.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        clickHandler();
      }
    });
  }

  return row;
}
