// Mock fallback vault database for web browser preview mode.
// Relative dates keep the sample useful instead of turning every release overdue.
function sampleDate(daysFromToday) {
  const date = new Date();
  date.setUTCHours(0, 0, 0, 0);
  date.setUTCDate(date.getUTCDate() + daysFromToday);
  return date.toISOString().slice(0, 10);
}

const SAMPLE_TARGET_DATE = sampleDate(3);
const SAMPLE_VERIFIED_DATE = sampleDate(-1);

export let mockVaultFiles = {
  "go_no_go.md": `---
type: go_no_go
release: 3.4.0
target_date: ${SAMPLE_TARGET_DATE}
required_qa_completion: 100
status: at_risk
---

# Release Gate: Northstar Mobile 3.4.0

Supporting legacy Go/No-Go matrix for the same Android candidate shown in Release Portfolio.

## Release Readiness Gating Checklist

- [x] **Product:** Store notes and rollout messaging are approved. <!-- @maya -->
- [/] **Engineering:** Candidate is SHA-bound; one deep-link blocker remains. <!-- @devon -->
- [ ] **QA:** Full regression suite completed on the exact Android candidate. <!-- @riley -->
- [x] **Support:** Release-day escalation and rollback owners are documented. <!-- @maya -->

---

## Decision Dashboard

* **Target Release Date:** ${SAMPLE_TARGET_DATE}
* **Current Blockers:** 1 Open (See [[BLK-102-android-deep-link]])
* **QA Progress:** 5/8 Checks Completed`,

  "blockers/BLK-102-android-deep-link.md": `---
type: blocker
id: BLK-102
title: Android checkout deep link opens a stale cart
severity: P0
status: blocked
owner: "@devon"
source_files:
  - lib/routing/checkout_router.dart
  - android/app/src/main/AndroidManifest.xml
last_verified: ${SAMPLE_VERIFIED_DATE}
created_at: ${sampleDate(-4)}
---

# Android checkout deep link opens a stale cart

## Description
Opening the production checkout link from an Android notification can restore a cached
cart instead of the order referenced by the signed link. The exact AAB remains At Risk
until the routing fix and regression evidence are attached.

## Action Items
- [ ] Bind the signed order ID before restoring cached navigation state.
- [ ] Run the deep-link suite against the exact release AAB.
- [ ] Attach the passing CI run and rerun the candidate audit.`,

  "qa/regression.md": `---
type: qa
id: QA-REGRESSION
title: Mobile Candidate Regression Suite
owner: "@riley"
platforms:
  - ios
  - android
last_verified: ${SAMPLE_VERIFIED_DATE}
---

# Mobile Candidate Regression Suite (v3.4.0)

## Candidate checks

- [x] Candidate SHA matches the Play Console upload artifact. <!-- @riley -->
- [x] Release signing certificate is verified. <!-- @riley -->
- [ ] Checkout notification deep link opens the signed order. <!-- @riley -->
- [x] Privacy and permission changes match the approved policy. <!-- @riley -->
- [x] Rollback build is available to the release owner. <!-- @riley -->

## Store and recovery checks

- [ ] Staged rollout pause has been rehearsed. <!-- @riley -->
- [x] Support escalation owner is reachable. <!-- @riley -->
- [ ] Final post-upload smoke test is scheduled. <!-- @riley -->`,

  "tasks.md": `# Gantry Repair Queue

<!-- gantry:id:missing-doc-README-launch source:audit severity:P0 evidence:README-launch.md created_at:${sampleDate(-4)} -->
- [ ] Create missing required document: README-launch.md

<!-- gantry:id:missing-doc-AGENTS source:audit severity:P0 evidence:AGENTS.md created_at:${sampleDate(-4)} -->
- [ ] Create missing required document: AGENTS.md
`,

  "README-launch.md": `# Northstar Mobile 3.4.0 Release

## Overview

Android production rollout governed against the exact signed AAB in the release portfolio.

## Release Notes

## Known Issues

## Rollback Plan

The release owner may pause staged rollout and restore build 103 while Engineering
investigates checkout or authentication regressions.
`,

  "AGENTS.md": `# Agent Roster

## Responsibilities

<!-- TODO: List all AI agents operating in this codebase and their scope -->

## Active Agents

- [ ] Document each agent's file scope and permitted write targets
- [ ] Confirm each agent respects the .gantry/ vault schema
`,
  "config.json": `{\n  "vaultName": "Northstar Mobile v3.4.0",\n  "version": "3.4.0",\n  "releaseDate": "${SAMPLE_TARGET_DATE}",\n  "reportUrl": ""\n}`
};

// Mock fallback IndexPayload
export const mockLaunchIndex = {
  vault_name: "Northstar Mobile v3.4.0",
  release: "3.4.0",
  readiness: "At Risk",
  target_date: SAMPLE_TARGET_DATE,
  open_blockers: 1,
  qa_total: 8,
  qa_completed: 5,
  stale_docs: ["blockers/BLK-102-android-deep-link.md"],
  missing_docs: ["release-notes.md"],
  blockers: [
    {
      path: "blockers/BLK-102-android-deep-link.md",
      id: "BLK-102",
      title: "Android checkout deep link opens a stale cart",
      severity: "P0",
      status: "blocked",
      owner: "@devon",
      source_files: ["lib/routing/checkout_router.dart", "android/app/src/main/AndroidManifest.xml"],
      stale: true,
      last_verified: SAMPLE_VERIFIED_DATE
    }
  ],
  qa_suites: [
    {
      path: "qa/regression.md",
      id: "QA-REGRESSION",
      title: "Mobile Candidate Regression Suite",
      owner: "@riley",
      total_tests: 8,
      completed_tests: 5
    }
  ],
  last_updated: Date.now().toString(),
  validation_errors: [
    {
      severity: "warning",
      file: "release-notes.md",
      code: "MISSING_DOC",
      message: "Missing recommended document: release-notes.md",
      field: null,
      evidence: {
        id: "missing-doc-release-notes.md",
        kind: "doc",
        path: "release-notes.md",
        line: null,
        message: "Missing recommended document: release-notes.md",
        severity: "warning",
        related_finding: "MISSING_DOC",
        suggested_action: "Create missing document: release-notes.md"
      }
    }
  ],
  broken_links: [],
  source_missing: [],
  docs_health_status: "warnings",
  go_no_go_gates: [
    {
      name: "**Product:** Store notes and rollout messaging are approved.",
      status: "checked",
      owner: "@maya",
      line_index: 14
    },
    {
      name: "**Engineering:** Candidate is SHA-bound; one deep-link blocker remains.",
      status: "partial",
      owner: "@devon",
      line_index: 15
    },
    {
      name: "**QA:** Full regression suite completed on the exact Android candidate.",
      status: "unchecked",
      owner: "@riley",
      line_index: 16
    },
    {
      name: "**Support:** Release-day escalation and rollback owners are documented.",
      status: "checked",
      owner: "@maya",
      line_index: 17
    }
  ],
  signoffs: [
    {
      name: "@maya",
      role: "Product",
      status: "Approved",
      date: SAMPLE_VERIFIED_DATE
    },
    {
      name: "@devon",
      role: "Engineering",
      status: "In Progress",
      date: ""
    },
    {
      name: "@riley",
      role: "QA",
      status: "Pending",
      date: ""
    }
  ],
  readiness_reasons: [
    "Launch is at risk: 1 open P0 blocker remains.",
    "Required stakeholder sign-off is incomplete: @riley (QA)"
  ],
  readiness_score: 62,
  category_scores: {
    "Product": 90,
    "Engineering": 45,
    "QA": 55,
    "Docs": 80,
    "Monitoring": 70,
    "Support": 70
  },
  score_reasons: {
    Monitoring: ["-30: Sentry crash monitoring organization or project is not configured in config.json."]
  },
  evidence_graph: [
    {
      id: "missing-doc-release-notes.md",
      kind: "doc",
      path: "release-notes.md",
      line: null,
      message: "Missing recommended document: release-notes.md",
      severity: "warning",
      related_finding: "MISSING_DOC",
      suggested_action: "Create missing document: release-notes.md"
    }
  ],
  impact_summary: {
    changed_files: ["lib/routing/checkout_router.dart", "android/app/src/main/AndroidManifest.xml", "pubspec.lock"],
    impacted_blockers: ["BLK-102 (Android checkout deep link)"],
    impacted_qa: ["QA-REGRESSION (Mobile Candidate Regression Suite)"],
    uncovered_changes: ["lib/routing/"],
    suggested_tasks: [
      "Re-verify BLK-102 against the bound Android candidate",
      "Attach deep-link regression evidence before approval"
    ]
  },
  active_environment: "production",
  environment_results: {
    production: {
      profile: {
        requiredQaCompletion: 100,
        blockOnP0: true,
        minCrashFreeRate: 99.9,
        requireSignoffs: true,
        maxObservedAgeHours: 24,
        blockOnMissingTelemetry: true
      },
      readiness: "At Risk",
      readiness_score: 62,
      readiness_reasons: [
        "Launch is at risk: 1 open P0 blocker remains.",
        "Required stakeholder sign-off is incomplete: @riley (QA)"
      ],
      category_scores: {
        "Product": 90,
        "Engineering": 45,
        "QA": 55,
        "Docs": 80,
        "Monitoring": 70,
        "Support": 70
      },
      score_reasons: {
        Monitoring: ["-30: Sentry crash monitoring organization or project is not configured in config.json."]
      },
      telemetry_status: "missing",
      observed_telemetry: null
    },
    staging: {
      profile: {
        requiredQaCompletion: 80,
        blockOnP0: true,
        minCrashFreeRate: null,
        requireSignoffs: false,
        maxObservedAgeHours: 24,
        blockOnMissingTelemetry: false
      },
      readiness: "Go",
      readiness_score: 85,
      readiness_reasons: [
        "Staging gates cleared. Sign-offs bypassed by policy."
      ],
      category_scores: {
        "Product": 90,
        "Engineering": 80,
        "QA": 80,
        "Docs": 80,
        "Monitoring": 100,
        "Support": 70
      },
      score_reasons: {},
      telemetry_status: "not_required",
      observed_telemetry: null
    }
  }
};

// Mock recent changes for browser preview mode
export const mockRecentChanges = [
  { hash: "a1b2c3d", date: `${SAMPLE_VERIFIED_DATE} 14:22:00`, message: "fix: bind checkout deep links before restoring cart state", author: "@devon", kind: "fix" },
  { hash: "e4f5g6h", date: `${sampleDate(-3)} 10:15:00`, message: "chore: bind the 3.4.0 Android release candidate", author: "@maya", kind: "chore" },
  { hash: "i7j8k9l", date: `${sampleDate(-4)} 16:45:00`, message: "test: add candidate-bound mobile regression coverage", author: "@riley", kind: "test" }
];
