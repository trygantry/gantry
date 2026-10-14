/**
 * @typedef {Object} Blocker
 * @property {string} path - Path to the blocker file
 * @property {string} id - Unique identifier, e.g. "BLK-102"
 * @property {string} title - Human-readable title
 * @property {string} severity - "P0" | "P1" | "P2"
 * @property {string} status - "open" | "in_progress" | "blocked" | "resolved"
 * @property {string} owner - Stakeholder username/handle
 * @property {string[]} source_files - Project files affected by this blocker
 * @property {boolean} stale - Whether the metadata is outdated
 * @property {string} last_verified - Last verification date string
 */

/**
 * @typedef {Object} QASuite
 * @property {string} path - Path to the QA suite file
 * @property {string} id - Unique identifier, e.g. "QA-REGRESSION"
 * @property {string} title - Title of the test suite
 * @property {string} owner - QA owner username/handle
 * @property {number} total_tests - Total tests in suite
 * @property {number} completed_tests - Passed/completed tests
 * @property {boolean} [stale] - Whether the verification is stale
 * @property {string} [last_verified] - Last verification date string
 * @property {string[]} [source_files] - Associated source code files
 */

/**
 * @typedef {Object} Gate
 * @property {string} name - Title description of the gate
 * @property {string} status - "checked" | "unchecked" | "partial"
 * @property {string} owner - Owner username/handle
 * @property {number} line_index - Line position in the Markdown file
 */

/**
 * @typedef {Object} Signoff
 * @property {string} name - Approver username/handle
 * @property {string} role - Approver title or department
 * @property {string} status - "Approved" | "In Progress" | "Pending"
 * @property {string} date - Date signed off or empty string
 */

/**
 * @typedef {Object} Telemetry
 * @property {number} crash_free_rate - Stability rate percentage
 * @property {string} source - Telemetry source identifier
 */

/**
 * @typedef {Object} GantryEvidence
 * @property {string} id - Unique evidence ID
 * @property {string} kind - "doc" | "source_file" | "checklist" | "telemetry"
 * @property {string} path - Path referencing this item
 * @property {number|null} line - Line number if applicable
 * @property {string} message - Descriptive message
 * @property {string} severity - "warning" | "critical"
 * @property {string} related_finding - Type of finding code
 * @property {string} suggested_action - Proposed remediation step
 */

/**
 * @typedef {Object} GantryIssue
 * @property {string} severity - "error" | "warning"
 * @property {string} file - Path referencing this issue
 * @property {string} code - Issue code, e.g. "INVALID_SCHEMA" | "BROKEN_LINK" | "SOURCE_MISSING" | "STALE_DOC" | "MISSING_DOC"
 * @property {string} message - Descriptive error message
 * @property {string|null} field - Config field name if applicable
 * @property {GantryEvidence|null} [evidence] - Associated evidence if available
 */

/**
 * @typedef {Object} ImpactSummary
 * @property {string[]} changed_files - Files modified since target branch
 * @property {string[]} impacted_blockers - Blockers matching modified files
 * @property {string[]} impacted_qa - QA suites covering modified files
 * @property {string[]} uncovered_changes - Modified files lacking coverage
 * @property {string[]} suggested_tasks - AI/Audit suggested tasks
 */

/**
 * @typedef {Object} EnvironmentPayload
 * @property {Record<string, any>} profile - Configuration thresholds/rules
 * @property {string} readiness - Active readiness state ("Go" | "At Risk" | "No-Go")
 * @property {number} readiness_score - Score out of 100
 * @property {string[]} readiness_reasons - Explanations for current state
 * @property {Record<string, number>} category_scores - Detailed category scores
 * @property {Record<string, string[]>} score_reasons - Penalty causes
 * @property {string} telemetry_status - Telemetry state ("fresh" | "stale" | "missing" | "invalid" | "not_required")
 * @property {Telemetry|null} observed_telemetry - Actual stability records
 */

/**
 * @typedef {Object} IndexPayload
 * @property {string} vault_name - Human-readable cockpit name
 * @property {string} release - Targeted version tag
 * @property {string} readiness - Unified state ("Go" | "At Risk" | "No-Go")
 * @property {string|null} target_date - Launch date deadline
 * @property {number} open_blockers - Total active blockers count
 * @property {number} qa_total - Sum of total tests
 * @property {number} qa_completed - Sum of passed tests
 * @property {string[]} stale_docs - Documents needing update
 * @property {string[]} missing_docs - Declared files that do not exist
 * @property {Blocker[]} blockers - Configured blockers
 * @property {QASuite[]} qa_suites - Test checklists
 * @property {string} last_updated - Unix timestamp string
 * @property {GantryIssue[]} validation_errors - Structural issues
 * @property {GantryIssue[]} broken_links - Wiki/Doc references pointing to missing files
 * @property {GantryIssue[]} source_missing - Missing source files references
 * @property {string} docs_health_status - General documentation status
 * @property {Gate[]} go_no_go_gates - Gates from go_no_go.md
 * @property {Signoff[]} signoffs - Required signoffs
 * @property {string[]} readiness_reasons - Causes of current status
 * @property {number} readiness_score - Index score (0-100)
 * @property {Record<string, number>} category_scores - Category performance breakdown
 * @property {Record<string, string[]>} score_reasons - Explanations of penalties
 * @property {any[]} evidence_graph - Connected nodes of evidence
 * @property {ImpactSummary} impact_summary - Diff impact results
 * @property {string} active_environment - Active deployment phase
 * @property {Record<string, EnvironmentPayload>} environment_results - Phase overrides
 * @property {string} [telemetry_status] - Root telemetry status
 * @property {Telemetry|null} [observed_telemetry] - Root telemetry records
 */

/**
 * Clones and normalizes the IndexPayload, ensuring no undefined fields are present
 * and applying environment results overrides in one centralized helper.
 * 
 * @param {any} inputPayload - Raw index payload loaded from the Rust backend or constants
 * @param {string|null} activeEnv - The specific environment to apply overrides for
 * @returns {IndexPayload} The cloned and normalized index payload
 */
export function normalizeIndexPayload(inputPayload, activeEnv = null) {
  if (!inputPayload) {
    return createEmptyPayload();
  }

  // Clone payload immutably to prevent mutating the original constants or cache
  /** @type {IndexPayload} */
  const clone = JSON.parse(JSON.stringify(inputPayload));

  // Default collections to empty structures if null or undefined
  clone.vault_name = clone.vault_name || "Gantry Control Room";
  clone.release = clone.release || "";
  clone.readiness = clone.readiness || "At Risk";
  clone.target_date = clone.target_date || null;
  clone.open_blockers = typeof clone.open_blockers === "number" ? clone.open_blockers : 0;
  clone.qa_total = typeof clone.qa_total === "number" ? clone.qa_total : 0;
  clone.qa_completed = typeof clone.qa_completed === "number" ? clone.qa_completed : 0;
  
  clone.stale_docs = Array.isArray(clone.stale_docs) ? clone.stale_docs : [];
  clone.missing_docs = Array.isArray(clone.missing_docs) ? clone.missing_docs : [];
  clone.blockers = Array.isArray(clone.blockers) ? clone.blockers : [];
  clone.qa_suites = Array.isArray(clone.qa_suites) ? clone.qa_suites : [];
  clone.last_updated = clone.last_updated || "";
  clone.validation_errors = Array.isArray(clone.validation_errors) ? clone.validation_errors : [];
  clone.broken_links = Array.isArray(clone.broken_links) ? clone.broken_links : [];
  clone.source_missing = Array.isArray(clone.source_missing) ? clone.source_missing : [];
  clone.docs_health_status = clone.docs_health_status || "";
  clone.go_no_go_gates = Array.isArray(clone.go_no_go_gates) ? clone.go_no_go_gates : [];
  clone.signoffs = Array.isArray(clone.signoffs) ? clone.signoffs : [];
  clone.readiness_reasons = Array.isArray(clone.readiness_reasons) ? clone.readiness_reasons : [];
  clone.readiness_score = typeof clone.readiness_score === "number" ? clone.readiness_score : 0;
  
  clone.category_scores = clone.category_scores && typeof clone.category_scores === "object" ? clone.category_scores : {};
  clone.score_reasons = clone.score_reasons && typeof clone.score_reasons === "object" ? clone.score_reasons : {};
  clone.evidence_graph = Array.isArray(clone.evidence_graph) ? clone.evidence_graph : [];

  const defaultImpact = { changed_files: [], impacted_blockers: [], impacted_qa: [], uncovered_changes: [], suggested_tasks: [] };
  if (!clone.impact_summary || typeof clone.impact_summary !== "object") {
    clone.impact_summary = defaultImpact;
  } else {
    clone.impact_summary.changed_files = Array.isArray(clone.impact_summary.changed_files) ? clone.impact_summary.changed_files : [];
    clone.impact_summary.impacted_blockers = Array.isArray(clone.impact_summary.impacted_blockers) ? clone.impact_summary.impacted_blockers : [];
    clone.impact_summary.impacted_qa = Array.isArray(clone.impact_summary.impacted_qa) ? clone.impact_summary.impacted_qa : [];
    clone.impact_summary.uncovered_changes = Array.isArray(clone.impact_summary.uncovered_changes) ? clone.impact_summary.uncovered_changes : [];
    clone.impact_summary.suggested_tasks = Array.isArray(clone.impact_summary.suggested_tasks) ? clone.impact_summary.suggested_tasks : [];
  }

  clone.environment_results = clone.environment_results && typeof clone.environment_results === "object" ? clone.environment_results : {};

  // Resolve target environment
  const targetEnv = activeEnv || clone.active_environment || "production";
  clone.active_environment = targetEnv;

  // Apply environment-specific overrides if configured
  if (clone.environment_results && clone.environment_results[targetEnv]) {
    const envRes = clone.environment_results[targetEnv];
    
    clone.readiness = envRes.readiness || clone.readiness;
    clone.readiness_score = typeof envRes.readiness_score === "number" ? envRes.readiness_score : clone.readiness_score;
    clone.readiness_reasons = Array.isArray(envRes.readiness_reasons) ? envRes.readiness_reasons : clone.readiness_reasons;
    
    if (envRes.category_scores && typeof envRes.category_scores === "object") {
      clone.category_scores = Object.assign({}, clone.category_scores, envRes.category_scores);
    }
    
    if (envRes.score_reasons && typeof envRes.score_reasons === "object") {
      clone.score_reasons = Object.assign({}, clone.score_reasons, envRes.score_reasons);
    }
    
    clone.telemetry_status = envRes.telemetry_status || "not_required";
    clone.observed_telemetry = envRes.observed_telemetry || null;
  } else {
    // If no overrides exist for target environment, apply defaults to root telemetry properties
    clone.telemetry_status = clone.telemetry_status || "not_required";
    clone.observed_telemetry = clone.observed_telemetry || null;
  }

  return clone;
}

/**
 * Creates a fully initialized blank IndexPayload.
 * @returns {IndexPayload} An empty index payload structure.
 */
function createEmptyPayload() {
  return {
    vault_name: "Gantry Control Room",
    release: "",
    readiness: "At Risk",
    target_date: null,
    open_blockers: 0,
    qa_total: 0,
    qa_completed: 0,
    stale_docs: [],
    missing_docs: [],
    blockers: [],
    qa_suites: [],
    last_updated: "",
    validation_errors: [],
    broken_links: [],
    source_missing: [],
    docs_health_status: "unknown",
    go_no_go_gates: [],
    signoffs: [],
    readiness_reasons: [],
    readiness_score: 0,
    category_scores: {},
    score_reasons: {},
    evidence_graph: [],
    impact_summary: {
      changed_files: [],
      impacted_blockers: [],
      impacted_qa: [],
      uncovered_changes: [],
      suggested_tasks: []
    },
    active_environment: "production",
    environment_results: {}
  };
}

/**
 * Derives consistent status metadata from a readiness value.
 * @param {string} readiness - The readiness state ("Go" | "At Risk" | "No-Go")
 * @returns {{ readinessStatusClass: string, isBlockingStatus: boolean, statusLabel: string }}
 */
export function getReadinessMeta(readiness) {
  const clean = (readiness || "At Risk").toLowerCase().replace(/\s+/g, "");
  if (clean === "go") {
    return {
      readinessStatusClass: "go",
      isBlockingStatus: false,
      statusLabel: "GO FOR LAUNCH"
    };
  } else if (clean === "nogo" || clean === "no-go") {
    return {
      readinessStatusClass: "no-go",
      isBlockingStatus: true,
      statusLabel: "NO-GO — LAUNCH BLOCKED"
    };
  } else {
    return {
      readinessStatusClass: "at-risk",
      isBlockingStatus: false,
      statusLabel: "AT RISK — REVIEW REQUIRED"
    };
  }
}
