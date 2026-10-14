import { mockVaultFiles, mockLaunchIndex, mockRecentChanges } from './constants.js';
import { state } from './state.js';
import { normalizeIndexPayload } from './contract.js';
import { hasTauri, invokeNative } from './runtime.js';
import {
  validateMobileAuditResult,
  validateMobileRelease,
  validateReleaseAuditResult,
  validateReleaseList,
  validateReleasePortfolio,
  validateReleaseView,
  validateSchemaRecord
} from './mobile-contracts.js';

export { hasTauri } from './runtime.js';
let activeMobileAuditRequestId = null;
const invoke = invokeNative;
const DEMO_ANDROID_SHA = "8f2c453d9e674ec8a8c4795e5d84608f142756b36e8e8fe4655f84b97b6f32e9";
const DEMO_IOS_SHA = "4c91a407b55d09fa3c3f7e90c770ac51c614cecb9d939c97f4e5a3d94dcf40fe";
const DEMO_GIT_COMMIT = "a1b2c3d4e5f678901234567890abcdef12345678";

function demoDate(daysFromToday) {
  const date = new Date();
  date.setUTCHours(0, 0, 0, 0);
  date.setUTCDate(date.getUTCDate() + daysFromToday);
  return date.toISOString().slice(0, 10);
}

function demoTimestamp(daysFromToday, hour = 12) {
  return `${demoDate(daysFromToday)}T${String(hour).padStart(2, "0")}:00:00Z`;
}

function demoArtifactSha(artifactPath) {
  return String(artifactPath || "").toLowerCase().endsWith(".ipa") ? DEMO_IOS_SHA : DEMO_ANDROID_SHA;
}

export async function fetchLaunchIndex(demoMode = false, env = null) {
  const active = env || (state && state.activeEnv) || "production";

  let raw;
  if (demoMode || !hasTauri || state.demoMode) {
    raw = mockLaunchIndex;
  } else {
    try {
      raw = await invoke("rebuild_gantry_index", { env: active });
    } catch (err) {
      console.error("Failed to compile Gantry index from Tauri Rust:", err);
      throw err;
    }
  }

  state.realIndexRaw = raw;
  return normalizeIndexPayload(raw, active);
}

export async function loadFileContent(filePath) {
  if (hasTauri && !state.demoMode) {
    return await invoke("read_vault_file", { path: filePath });
  }
  return mockVaultFiles[filePath] || "";
}

export async function saveFileContent(filePath, content) {
  // Respect demo mode: bypass Tauri and write to mock database
  if (hasTauri && !state.demoMode) {
    try {
      await invoke("write_vault_file", { path: filePath, content });
    } catch (err) {
      throw new Error(`Save failed: ${err}`);
    }
  } else {
    mockVaultFiles[filePath] = content;
  }
}

export async function listVaultFiles() {
  if (hasTauri && !state.demoMode) {
    return await invoke("list_vault_files");
  }
  return Object.keys(mockVaultFiles).sort();
}

export async function updateBlockerField(path, field, value) {
  // Respect demo mode: bypass Tauri and update mock files frontmatter
  if (hasTauri && !state.demoMode) {
    try {
      await invoke("update_blocker_field", { path, field, value });
      console.log(`Updated ${field} = ${value} on ${path}`);
    } catch (err) {
      console.error("Failed to update blocker field:", err);
      throw err;
    }
  } else {
    const content = mockVaultFiles[path];
    if (content) {
      const lines = content.split("\n");
      let found = false;
      for (let i = 0; i < lines.length; i++) {
        if (lines[i].startsWith(`${field}:`)) {
          lines[i] = `${field}: ${value.startsWith('"') ? value : '"' + value + '"'}`;
          found = true;
          break;
        }
      }
      if (!found) {
        for (let i = 1; i < lines.length; i++) {
          if (lines[i].trim() === "---") {
            lines.splice(i, 0, `${field}: ${value}`);
            break;
          }
        }
      }
      mockVaultFiles[path] = lines.join("\n");
    }
  }
}

function updateMockChecklistLine(path, lineIndex, checked) {
  const content = mockVaultFiles[path];
  if (!content) throw new Error(`Mock vault file not found: ${path}`);
  const lines = content.split("\n");
  const line = lines[lineIndex];
  if (!line || !/-\s*\[[ x/]\]/i.test(line)) {
    throw new Error(`Line ${lineIndex + 1} is not a checklist item.`);
  }
  const today = new Date().toISOString().split("T")[0];
  const ownerMatch = line.match(/<!--\s*(?:owner:)?(@[\w.-]+)/);
  const owner = ownerMatch ? ownerMatch[1] : "@unassigned";
  let next = line.replace(/-\s*\[[ x/]\]/i, `- [${checked ? "x" : " "}]`);
  if (next.includes("<!--") && next.includes("-->")) {
    next = next.replace(/<!--[\s\S]*?-->/, checked ? `<!-- ${owner} ${today} by:@demo -->` : `<!-- ${owner} -->`);
  } else {
    next = `${next.trimEnd()} ${checked ? `<!-- ${owner} ${today} by:@demo -->` : `<!-- ${owner} -->`}`;
  }
  lines[lineIndex] = next;
  mockVaultFiles[path] = lines.join("\n");

  if (path === "go_no_go.md") {
    const gate = (mockLaunchIndex.go_no_go_gates || []).find(g => g.line_index === lineIndex);
    if (gate) gate.status = checked ? "checked" : "unchecked";
  }

  if (path.startsWith("qa/")) {
    const suite = (mockLaunchIndex.qa_suites || []).find(q => q.path === path);
    if (suite) {
      const matches = mockVaultFiles[path].match(/^\s*-\s*\[[ x/]\]/gim) || [];
      const checkedMatches = mockVaultFiles[path].match(/^\s*-\s*\[x\]/gim) || [];
      suite.total_tests = matches.length;
      suite.completed_tests = checkedMatches.length;
      mockLaunchIndex.qa_total = (mockLaunchIndex.qa_suites || []).reduce((sum, q) => sum + (q.total_tests || 0), 0);
      mockLaunchIndex.qa_completed = (mockLaunchIndex.qa_suites || []).reduce((sum, q) => sum + (q.completed_tests || 0), 0);
    }
  }
  mockLaunchIndex.last_updated = Date.now().toString();
}

export async function toggleChecklistItem(path, lineIndex, checked) {
  if (hasTauri && !state.demoMode) {
    await invoke("toggle_checklist_item", { path, lineIndex, checked });
    return;
  }
  updateMockChecklistLine(path, lineIndex, checked);
}

function slugify(value) {
  return (value || "untitled")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "untitled";
}

function yamlBlock(key, items) {
  const clean = (items || []).map(v => v.trim()).filter(Boolean);
  if (!clean.length) return `${key}: []`;
  return `${key}:\n${clean.map(v => `  - "${v.replace(/"/g, '\\"')}"`).join("\n")}`;
}

export async function createBlockerFile(id, title, severity, owner, sourceFiles = []) {
  if (hasTauri && !state.demoMode) {
    return await invoke("create_blocker_file", { id, title, severity, owner, sourceFiles });
  }
  const today = new Date().toISOString().split("T")[0];
  const path = `blockers/${id}-${slugify(title)}.md`;
  if (mockVaultFiles[path]) throw new Error(`A blocker file already exists at ${path}`);
  mockVaultFiles[path] = `---\ntype: blocker\nid: ${id}\ntitle: "${title.replace(/"/g, '\\"')}"\nseverity: ${severity}\nstatus: open\nowner: "${owner}"\n${yamlBlock("source_files", sourceFiles)}\nlast_verified: ${today}\ncreated_at: ${today}\n---\n\n# ${title}\n\n## Description\n\n\n## Action Items\n\n- [ ] Triage impact and reproduction steps. <!-- ${owner} -->\n`;
  mockLaunchIndex.blockers.push({ path, id, title, severity, status: "open", owner, source_files: sourceFiles, stale: false, last_verified: today });
  mockLaunchIndex.open_blockers = mockLaunchIndex.blockers.filter(b => b.status !== "resolved").length;
  mockLaunchIndex.last_updated = Date.now().toString();
  return path;
}

export async function createQAFile(id, title, owner, platforms = []) {
  if (hasTauri && !state.demoMode) {
    return await invoke("create_qa_file", { id, title, owner, platforms });
  }
  const today = new Date().toISOString().split("T")[0];
  const path = `qa/${slugify(title)}.md`;
  if (mockVaultFiles[path]) throw new Error(`A QA file already exists at ${path}`);
  mockVaultFiles[path] = `---\ntype: qa\nid: ${id}\ntitle: "${title.replace(/"/g, '\\"')}"\nowner: "${owner}"\n${yamlBlock("platforms", platforms)}\nlast_verified: ${today}\n---\n\n# ${title}\n\n## Test Cases\n\n- [ ] Verify primary release flow. <!-- ${owner} -->\n- [ ] Verify rollback path is documented. <!-- ${owner} -->\n`;
  mockLaunchIndex.qa_suites.push({ path, id, title, owner, total_tests: 2, completed_tests: 0, stale: false, last_verified: today, source_files: [] });
  mockLaunchIndex.qa_total = (mockLaunchIndex.qa_suites || []).reduce((sum, q) => sum + (q.total_tests || 0), 0);
  mockLaunchIndex.qa_completed = (mockLaunchIndex.qa_suites || []).reduce((sum, q) => sum + (q.completed_tests || 0), 0);
  mockLaunchIndex.last_updated = Date.now().toString();
  return path;
}

export async function fetchRecentChanges() {
  if (hasTauri && !state.demoMode) {
    return await invoke("get_recent_changes");
  }
  return mockRecentChanges;
}

export async function getWorkspacePath() {
  // Respect demo mode: bypass Tauri and return empty string to show placeholder
  if (!hasTauri || state.demoMode) return "";
  try {
    return await invoke("get_current_workspace");
  } catch (e) {
    console.warn("Could not get workspace:", e);
    return "";
  }
}

export async function checkVaultHealth(repair = false) {
  if (!hasTauri || state.demoMode) {
    return {
      schema_version: '1.0',
      healthy: true,
      repaired_transactions: repair ? 1 : 0,
      pending_transactions: 0,
      verified_releases: 2,
      errors: []
    };
  }
  const report = await invoke('check_vault_cmd', { repair });
  return validateSchemaRecord(report, 'vault health report');
}

export async function previewVaultMigration() {
  if (!hasTauri || state.demoMode) {
    return {
      schema_version: '1.0',
      dry_run: true,
      current_release_schema: '1.0',
      migrations_required: 0,
      migrated_releases: 0,
      backup_path: null,
      messages: ['The current vault already uses the supported release schema.']
    };
  }
  const report = await invoke('migrate_vault_cmd', { dryRun: true });
  return validateSchemaRecord(report, 'vault migration report');
}

export async function writeSupportBundle() {
  if (!hasTauri || state.demoMode) {
    return {
      schema_version: '1.0',
      path: '.gantry/support/gantry-support-demo.json',
      bundle: { schema_version: '1.0' }
    };
  }
  const result = validateSchemaRecord(
    await invoke('write_support_bundle_cmd'),
    'support bundle result'
  );
  validateSchemaRecord(result.bundle, 'redacted support bundle');
  return result;
}

export async function pickProjectFolder() {
  if (!hasTauri) return null;
  try {
    return await invoke("pick_project_folder");
  } catch (e) {
    console.error("Failed to pick project folder:", e);
    return null;
  }
}

export async function runAudit(write = false) {
  // Respect demo mode: bypass Tauri audit
  if (hasTauri && !state.demoMode) {
    return await invoke("gantry_audit", { write });
  }
  return null; // Handle mock audit in renderer/state
}

export async function openInIde(path) {
  // Respect demo mode: bypass Tauri IDE opener
  if (hasTauri && !state.demoMode) {
    return await invoke("open_in_ide", { path });
  }
  console.log(`Mock: Opening ${path} in IDE`);
}

export async function watchVault(callback) {
  // Respect demo mode: bypass Tauri vault watcher
  if (hasTauri && !state.demoMode) {
    try {
      await invoke("watch_vault");
    } catch (err) {
      console.warn("Watcher disabled or failed:", err);
      return null;
    }
    try {
      return await window.__TAURI__.event.listen("vault-changed", callback);
    } catch (err) {
      console.warn("Vault event listener failed:", err);
      return null;
    }
  }
  return null;
}

let demoTasks = null;

export async function listPendingTasks() {
  // Respect demo mode: bypass Tauri tasks list
  if (hasTauri && !state.demoMode) {
    return await invoke("list_pending_tasks_cmd");
  }
  if (!demoTasks) {
    demoTasks = [
      { id: "missing-doc-README-launch.md", title: "Create missing required document: README-launch.md", source: "audit", severity: "P0", evidence: "README-launch.md", completed: false, status: "pending", reason: null },
      { id: "missing-doc-tasks.md", title: "Create missing required document: tasks.md", source: "audit", severity: "P0", evidence: "tasks.md", completed: false, status: "pending", reason: null },
      { id: "missing-doc-AGENTS.md", title: "Create missing required document: AGENTS.md", source: "audit", severity: "P0", evidence: "AGENTS.md", completed: false, status: "pending", reason: null },
      { id: "stale-doc-blockers/BLK-102-android-deep-link.md", title: "Re-verify stale document: blockers/BLK-102-android-deep-link.md", source: "audit", severity: "P1", evidence: "blockers/BLK-102-android-deep-link.md", completed: false, status: "pending", reason: null }
    ];
  }
  return demoTasks;
}

export async function completeTaskById(id) {
  // Respect demo mode: bypass Tauri task completion
  if (hasTauri && !state.demoMode) {
    await invoke("complete_task_by_id_cmd", { id });
  } else if (demoTasks) {
    const t = demoTasks.find(x => x.id === id);
    if (t) {
      t.completed = true;
      t.status = "manually_completed";
    }
  }
}

export async function ignoreTask(id, reason) {
  // Respect demo mode: bypass Tauri task ignore
  if (hasTauri && !state.demoMode) {
    await invoke("ignore_task_cmd", { id, reason });
  } else if (demoTasks) {
    const t = demoTasks.find(x => x.id === id);
    if (t) {
      t.completed = true;
      t.status = "ignored";
      t.reason = reason;
    }
  }
}

export async function getGitUser() {
  // Respect demo mode: bypass Tauri git user query
  if (hasTauri && !state.demoMode) {
    try {
      return await invoke("get_git_user");
    } catch (e) {
      console.warn("Could not get git user:", e);
      return { name: "", email: "" };
    }
  }
  return { name: "", email: "" };
}

export async function checkGitStatus() {
  // Respect demo mode: bypass Tauri git status check
  if (hasTauri && !state.demoMode) {
    try {
      return await invoke("check_git_status");
    } catch (e) {
      console.warn("Could not check git status:", e);
      return { is_git_repo: false, repo_root: "" };
    }
  }
  // Browser preview: assume a git repo for demo purposes
  return { is_git_repo: true, repo_root: "(browser preview)" };
}

export async function getCIStatus() {
  // Respect demo mode: bypass Tauri CI status check
  if (hasTauri && !state.demoMode) {
    try {
      return await invoke("get_ci_status");
    } catch (err) {
      console.error("Failed to get CI status:", err);
      return { configured: false, provider: "none", file_path: "" };
    }
  }
  return { configured: true, provider: "github", file_path: ".github/workflows/gantry.yml" };
}

export async function fetchCommitDetails(hash) {
  // Respect demo mode: bypass Tauri commit details retrieval
  if (hasTauri && !state.demoMode) {
    try {
      return await invoke("get_commit_details", { hash });
    } catch (err) {
      console.error("Failed to fetch commit details:", err);
      throw err;
    }
  } else {
    const mocks = {
      "a1b2c3d": {
        hash: "a1b2c3d",
        author: "@devon",
        date: `${demoDate(-1)} 14:22:00`,
        message: "fix: bind checkout deep links before restoring cart state",
        body: "Resolve the signed order identifier before cached navigation state is restored, and cover the notification entry path with a candidate regression test.",
        stat: "lib/routing/checkout_router.dart | 18 +++++++++++++-----\ntest/deep_link_test.dart           | 22 ++++++++++++++++++++++\n 2 files changed, 35 insertions(+), 5 deletions(-)"
      },
      "e4f5g6h": {
        hash: "e4f5g6h",
        author: "@maya",
        date: `${demoDate(-3)} 10:15:00`,
        message: "chore: bind the 3.4.0 Android release candidate",
        body: "Recorded the exact signed AAB, release owner, store submission date, and rollout target in the Gantry release workflow.",
        stat: ".gantry/go_no_go.md            | 28 +++++++++++++++++++++++++\n.gantry/tasks.md               |  8 ++++++++\n 2 files changed, 36 insertions(+)"
      },
      "i7j8k9l": {
        hash: "i7j8k9l",
        author: "@riley",
        date: `${demoDate(-4)} 16:45:00`,
        message: "test: add candidate-bound mobile regression coverage",
        body: "Added release smoke, rollback, signing, privacy, and deep-link checks mapped to the Android candidate.",
        stat: ".gantry/qa/regression.md       | 89 +++++++++++++++++++++++++++++++++++++++++++++++++++++++\n 1 file changed, 89 insertions(+)"
      }
    };
    return mocks[hash] || {
      hash,
      author: "@developer",
      date: `${demoDate(-5)} 12:00:00`,
      message: "Commit message placeholder",
      body: "Detailed description of the commit contents.",
      stat: "No files changed."
    };
  }
}

export async function detectProjectType() {
  if (hasTauri && !state.demoMode) {
    try {
      return await invoke("detect_project_type_cmd");
    } catch (err) {
      console.error("Failed to detect project type:", err);
      return "saas-app";
    }
  }
  return "mobile-app";
}

export async function openProjectFile(path) {
  if (hasTauri && !state.demoMode) {
    return await invoke("open_project_file", { path });
  } else {
    return path;
  }
}

export async function initCustomizedVault(options) {
  if (hasTauri && !state.demoMode) {
    await invoke("init_customized_vault_cmd", { options });
  } else {
    // Demo Mode custom setup simulation
    state.demoMode = true;
    mockLaunchIndex.release = options.version;
    mockLaunchIndex.target_date = options.release_date;
    
    // Filter and update gates
    const defaultGates = {
      "saas-app": [
        { name: "Engineering: SSL certificates & DB migrations", owner: options.lead_owner },
        { name: "QA: Stripe checkout flow verified", owner: options.qa_lead },
        { name: "Product: User analytics & landing page live", owner: options.lead_owner }
      ],
      "ai-app": [
        { name: "Engineering: Model fallback tested under timeout", owner: options.lead_owner },
        { name: "QA: Prompt jailbreaks & budget verification", owner: options.qa_lead },
        { name: "Product: Guidelines & moderation set up", owner: options.lead_owner }
      ],
      "backend-api": [
        { name: "Engineering: Schema migration & rollback tests", owner: options.lead_owner },
        { name: "QA: API load testing (10k req/sec peak)", owner: options.qa_lead },
        { name: "Security: OAuth scopes & endpoint audit", owner: options.lead_owner }
      ],
      "mobile-app": [
        { name: "Engineering: Exact IPA, AAB, or APK candidate is bound and audited", owner: options.lead_owner },
        { name: "QA: Candidate-bound regression and rollback checks are complete", owner: options.qa_lead },
        { name: "Product: Store submission and staged-rollout plan are approved", owner: options.lead_owner }
      ]
    };
    
    const gatesList = defaultGates[options.template_type] || defaultGates["saas-app"];
    mockLaunchIndex.go_no_go_gates = gatesList
      .filter(g => {
        const cat = g.name.split(":")[0];
        return options.enabled_gates.some(eg => eg.toLowerCase() === cat.toLowerCase());
      })
      .map((g, idx) => ({
        name: g.name,
        status: "unchecked",
        owner: g.owner,
        line_index: idx
      }));

    if (!options.create_qa) {
      mockLaunchIndex.qa_suites = [];
      mockLaunchIndex.qa_total = 0;
      mockLaunchIndex.qa_completed = 0;
    } else {
      mockLaunchIndex.qa_suites = [
        {
          path: `qa/regression.md`,
          id: `QA-PLAN`,
          title: `Custom Release Verification`,
          owner: options.qa_lead,
          total_tests: 2,
          completed_tests: 0,
          stale: false,
          last_verified: new Date().toISOString().split("T")[0],
          source_files: [options.template_type === "mobile-app" ? "lib/main.dart" : options.template_type === "saas-app" ? "src/checkout.js" : "src/ai/llm.py"]
        }
      ];
      mockLaunchIndex.qa_total = 2;
      mockLaunchIndex.qa_completed = 0;
    }
    
    mockLaunchIndex.last_updated = Date.now().toString();
  }
}

export async function getWorkspaceStatus() {
  if (hasTauri && !state.demoMode) {
    try {
      return await invoke("get_workspace_status");
    } catch (err) {
      console.error("Failed to check workspace status:", err);
      return { state: "noWorkspace", path: "", has_vault: false, persisted_workspace: null };
    }
  }
  return { state: "demoMode", path: "(demo workspace)", has_vault: true, persisted_workspace: null };
}

export async function getImpactSummary(baseRef) {
  if (hasTauri && !state.demoMode) {
    try {
      return await invoke("get_impact_summary_cmd", { baseRef: baseRef || null });
    } catch (err) {
      console.error("Failed to get impact summary:", err);
      throw err;
    }
  }
  return {
    changed_files: ["lib/routing/checkout_router.dart", "android/app/src/main/AndroidManifest.xml"],
    impacted_blockers: ["BLK-102"],
    impacted_qa: ["QA-PLAN"],
    uncovered_changes: [],
    suggested_tasks: ["Re-verify BLK-102 against the bound Android candidate"]
  };
}

export async function getImpactGraph(baseRef) {
  if (hasTauri && !state.demoMode) {
    try {
      return await invoke("get_impact_graph_cmd", { baseRef: baseRef || null });
    } catch (err) {
      console.error("Failed to get impact graph:", err);
      throw err;
    }
  }
  return {
    nodes: [
      { id: "commit_node", label: "HEAD", kind: "commit", status: "active" },
      { id: "file:checkout-router", label: "lib/routing/checkout_router.dart", kind: "file", status: null },
      { id: "file:android-manifest", label: "android/app/src/main/AndroidManifest.xml", kind: "file", status: null },
      { id: "BLK-102", label: "Android checkout deep link", kind: "blocker", status: "open" },
      { id: "QA-PLAN", label: "Mobile Candidate Regression", kind: "qa_suite", status: "63%" },
      { id: "gate:Engineering", label: "Gate: Candidate integrity", kind: "gate", status: "partial" },
      { id: "signoff:Release Owner", label: "Signoff: Release Owner", kind: "signoff", status: "Approved" }
    ],
    edges: [
      { source: "commit_node", target: "file:checkout-router", kind: "impacts" },
      { source: "commit_node", target: "file:android-manifest", kind: "impacts" },
      { source: "file:checkout-router", target: "BLK-102", kind: "blocks" },
      { source: "file:checkout-router", target: "QA-PLAN", kind: "covers" }
    ]
  };
}

export async function getWaivers() {
  if (hasTauri && !state.demoMode) {
    return await invoke("get_waivers_cmd");
  }
  return [
    {
      id: "WAV-001",
      target_type: "blocker",
      target_id: "BLK-102",
      target_path: "blockers/BLK-102-android-deep-link.md",
      target_code: null,
      reason: "Temporary acceptance while the deep-link fix completes candidate regression",
      approved_by: "@maya",
      created_at: demoDate(-1),
      expires_at: demoDate(7),
      status: "active"
    }
  ];
}

export async function getReleaseManifest(release = null) {
  if (hasTauri && !state.demoMode) {
    try {
      return await invoke("get_release_manifest_cmd", { release });
    } catch (err) {
      console.error("Failed to fetch release manifest:", err);
      throw err;
    }
  }
  return {
    release: release || "3.4.0",
    commit: "a1b2c3d",
    commit_range: null,
    created_at: demoTimestamp(-1),
    environments: ["production", "staging"],
    artifact_sha: DEMO_ANDROID_SHA,
    build_url: "https://github.com/example/northstar/actions/runs/123",
    included_blockers: ["BLK-102"],
    completed_qa: ["QA-PLAN"],
    signoffs: ["Release Owner:Approved"],
    waivers: ["WAV-001"],
    evidence_ids: ["EVI-1234"],
    readiness: "Go",
    readiness_score: 95
  };
}

export async function getTrends() {
  if (hasTauri && !state.demoMode) {
    return await invoke("get_trends_cmd");
  }
  return {
    dates: [demoDate(-5), demoDate(-4), demoDate(-3), demoDate(-2), demoDate(-1)],
    scores: [60, 75, 80, 85, 95],
    readiness_states: ["No-Go", "At Risk", "At Risk", "At Risk", "Go"],
    blockers: [3, 2, 2, 1, 0],
    qa_completion: [20, 50, 70, 90, 100],
    telemetry_freshness: ["stale", "fresh", "fresh", "fresh", "fresh"],
    waivers: [0, 1, 1, 1, 1],
    evidence: [0, 1, 2, 3, 4]
  };
}

export async function detectMobileProject() {
  if (hasTauri && !state.demoMode) {
    return await invoke("detect_mobile_project_cmd");
  }
  return {
    root: "Northstar Mobile",
    stacks: ["flutter"],
    platforms: ["ios", "android"],
    ios_targets: [{
      name: "Runner",
      info_plist: "ios/Runner/Info.plist",
      bundle_identifier: "com.example.northstar",
      version: "3.4.0",
      build: "104",
      minimum_os_version: "16.0",
      privacy_manifest: "ios/Runner/PrivacyInfo.xcprivacy",
      entitlements: "ios/Runner/Runner.entitlements",
      usage_descriptions: []
    }],
    android_apps: [{
      name: "app",
      gradle_file: "android/app/build.gradle.kts",
      manifest: "android/app/src/main/AndroidManifest.xml",
      application_id: "com.example.northstar",
      namespace: "com.example.northstar",
      target_sdk: 34,
      min_sdk: 24,
      version_code: "104",
      version_name: "3.4.0",
      permissions: [],
      release_signing_configured: true,
      debuggable: false,
      cleartext_traffic: false
    }],
    dependency_inventory: [
      { ecosystem: "flutter", path: "pubspec.lock", package_count: 47 },
      { ecosystem: "android", path: "android/gradle.lockfile", package_count: 31 }
    ],
    inspected_files: 18,
    scan_limitations: []
  };
}

export async function runMobileAudit(artifactPath = null, write = false) {
  if (hasTauri && !state.demoMode) {
    const requestId = globalThis.crypto?.randomUUID?.() || `audit-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    activeMobileAuditRequestId = requestId;
    try {
      return validateMobileAuditResult(await invoke("run_mobile_audit_cmd", { artifactPath, write, requestId }));
    } finally {
      if (activeMobileAuditRequestId === requestId) activeMobileAuditRequestId = null;
    }
  }
  const isCandidate = Boolean(artifactPath);
  const isIosCandidate = String(artifactPath || '').toLowerCase().endsWith('.ipa');
  const demoHash = demoArtifactSha(artifactPath);
  const baseFinding = {
    severity: "advisory",
    observed: null,
    expected: null,
    locations: [],
    rationale: "",
    remediation: "",
    authoritative_source: null,
    source_reviewed_at: null
  };
  const finding = (id, subject, title, category, status, values = {}) => ({
    ...baseFinding,
    id,
    rule_id: id,
    subject_id: subject,
    finding_instance_id: `${id}:${subject}`,
    title,
    category,
    status,
    ...values
  });
  const findings = [
    finding("MOB-SRC-001", "flutter_project", "Mobile project detected", "source", "pass", {
      observed: "Flutter project with iOS and Android targets",
      rationale: "Gantry found the first-party mobile configuration required for source checks."
    }),
    finding("MOB-CFG-002", "android_release", "Android release build is not debuggable", "configuration", "pass", {
      observed: "android:debuggable is false for release",
      locations: ["android/app/src/main/AndroidManifest.xml"],
      rationale: "Debuggable production applications expose unnecessary runtime access."
    }),
    finding("MOB-PRV-001", "ios_privacy_manifest", "iOS privacy manifest is present", "privacy", "pass", {
      observed: "PrivacyInfo.xcprivacy found",
      locations: ["ios/Runner/PrivacyInfo.xcprivacy"],
      authoritative_source: "Apple privacy manifest requirements",
      source_reviewed_at: "2026-07-13"
    }),
    finding("MOB-SEC-001", "android_cleartext", "Cleartext traffic is disabled", "security", "pass", {
      observed: "usesCleartextTraffic=false",
      expected: "Release traffic is encrypted by default",
      locations: ["android/app/src/main/AndroidManifest.xml"]
    }),
    finding("MOB-POL-001", "mobile_policy", "Repository policy loaded", "policy", "pass", {
      observed: ".gantry/mobile-policy.json · policy pack 2026.07",
      rationale: "The verdict is evaluated against a reviewable, versioned repository policy."
    }),
    isIosCandidate
      ? finding("MOB-CFG-007", "ios_deployment_target", "iOS deployment target should be reviewed", "configuration", "warning", {
        severity: "warning",
        observed: "IPHONEOS_DEPLOYMENT_TARGET 13.0",
        expected: "Confirm the supported OS range for this release",
        remediation: "Confirm the minimum iOS version against the product support policy and update the target if needed.",
        locations: ["ios/Runner.xcodeproj/project.pbxproj"],
        authoritative_source: "Apple deployment target guidance",
        source_reviewed_at: "2026-07-13"
      })
      : finding("MOB-CFG-006", "android_target_sdk", "Android target SDK should be reviewed", "configuration", "warning", {
        severity: "warning",
        observed: "targetSdk 34",
        expected: "Confirm the current store submission requirement before release",
        remediation: "Confirm the target SDK against the current Play submission requirement and update the release configuration if needed.",
        locations: ["android/app/build.gradle"],
        authoritative_source: "Google Play target API requirements",
        source_reviewed_at: "2026-07-13"
      }),
    finding("MOB-EVD-001", "release_owner_approval", "Release owner approval", "evidence", "manual", {
      severity: "warning",
      expected: "A named release owner approves this candidate",
      rationale: "Automated checks cannot establish human accountability for the shipping decision.",
      remediation: "Record release-owner approval for this candidate in the evidence packet."
    }),
    finding("MOB-EVD-002", "rollback_plan", "Rollback plan verified", "evidence", "manual", {
      severity: "warning",
      expected: "The team confirms a tested rollback or mitigation path",
      rationale: "A technically valid build can still be unsafe to release without a recovery plan.",
      remediation: "Attach or reference the tested rollback plan and name its owner."
    })
  ];
  if (!isCandidate) {
    findings.push(finding("MOB-ART-001", "release_artifact_inspection", "Exact release candidate not supplied", "artifact", "manual", {
      severity: "warning",
      expected: "The exact IPA, AAB, or APK intended for upload",
      rationale: "Source readiness does not prove that the final packaged binary matches the reviewed configuration.",
      remediation: "Select the exact IPA, AAB, or APK intended for store upload and rerun the audit."
    }));
  } else {
    findings.push(finding("MOB-ART-001", "release_artifact_inspection", "Release candidate fingerprinted", "artifact", "pass", {
      observed: `${artifactPath} · sha256 ${demoHash.slice(0, 16)}…`,
      rationale: "The verdict is cryptographically bound to the inspected candidate."
    }));
  }
  const summary = findings.reduce((counts, item) => {
    const key = item.status === "fail" ? "blockers" : item.status === "pass" ? "passed" : item.status === "warning" ? "warnings" : item.status;
    counts[key] += 1;
    return counts;
  }, { passed: 0, blockers: 0, warnings: 0, manual: 0, unavailable: 0 });
  return validateMobileAuditResult({
    schema_version: "2.0",
    report_path: write ? ".gantry/mobile/audits/demo.json" : null,
    report: {
      schema_version: "2.0",
      generated_at: new Date().toISOString(),
      project_root: ".",
      git_commit: DEMO_GIT_COMMIT,
      git_branch: "release/3.4.0",
      git_dirty: false,
      tool_version: "0.5.2",
      policy_digest: "a9f81aa0cc3015b8fc6b61fbf6204c0bcf6f312aac277bc09c8ec047ad089349",
      report_fingerprint: isIosCandidate
        ? "c94a124662994a67e62b78950ba92408f4e0775d5b023954af01ec28e143eafe"
        : "d82b42f09e799062718844f677c4b2ee50f86c0a30726dfbd80e5e2a4fc251a3",
      policy: { policy_pack_version: "2026.07", reviewed_at: "2026-07-13" },
      decision: "at_risk",
      decision_reasons: [
        `${summary.warnings} warning requires release review`,
        `${summary.manual} human evidence items remain`,
        ...(!isCandidate ? ["No exact release candidate was supplied"] : [])
      ],
      summary,
      project: await detectMobileProject(),
      artifact: isCandidate ? {
        path: artifactPath,
        kind: String(artifactPath).toLowerCase().endsWith(".ipa") ? "ipa" : String(artifactPath).toLowerCase().endsWith(".apk") ? "apk" : "aab",
        platform: String(artifactPath).toLowerCase().endsWith(".ipa") ? "ios" : "android",
        sha256: demoHash,
        size_bytes: isIosCandidate ? 34127890 : 28765432,
        application_id: "com.example.northstar",
        version: "3.4.0",
        build: "104",
        minimum_os_version: isIosCandidate ? "16.0" : null,
        target_sdk: isIosCandidate ? null : 34,
        permissions: [],
        signature_material: "demo release certificate",
        signature_verification: { status: "verified", verifier: "demo", details: "Signature verified", certificate_fingerprints: [] },
        privacy_manifest_present: isIosCandidate ? true : null,
        privacy_manifests: isIosCandidate ? ["Payload/Northstar.app/PrivacyInfo.xcprivacy"] : [],
        embedded_components: [],
        package_entries: 814,
        limitations: []
      } : null,
      findings
    }
  });
}

export async function cancelMobileAudit() {
  const requestId = activeMobileAuditRequestId;
  if (!requestId || !hasTauri || state.demoMode) return false;
  return Boolean(await invoke('cancel_mobile_audit_cmd', { requestId }));
}

export async function pickMobileArtifact() {
  if (hasTauri && !state.demoMode) {
    return await invoke("pick_mobile_artifact_cmd");
  }
  return null;
}

export async function initMobilePolicy(force = false) {
  if (hasTauri && !state.demoMode) {
    return await invoke("init_mobile_policy_cmd", { force });
  }
  return ".gantry/mobile-policy.json";
}

let demoWorkflowDecision = null;
let demoWorkflowCandidate = null;
let demoWorkflowAudit = null;
let demoWorkflowReport = null;
let demoWorkflowEvidence = [];
let demoWorkflowResolutions = [];
let demoWorkflowWaivers = [];
let demoCreatedReleases = [];
let demoReleaseSchedules = new Map();

export async function listReleaseWorkflows() {
  if (hasTauri && !state.demoMode) return validateReleaseList(await invoke("list_release_workflows_cmd"));
  const releases = [
    { id: "REL-DEMO210001", version: "3.4.0", name: "Android production rollout", owner: "maya@northstar.example", target_date: demoDate(3), submission_date: demoDate(1), rollout_date: demoDate(4), stage: demoWorkflowDecision?.release_id === "REL-DEMO210001" ? demoWorkflowDecision.decision : "in_review", updated_at: demoTimestamp(-1, 9), candidate_ids: ["CAN-DEMOA0001"], active_candidates: { android: "CAN-DEMOA0001" }, active_audits: { android: "AUD-DEMOA0001" }, active_decisions: demoWorkflowDecision?.release_id === "REL-DEMO210001" ? { android: demoWorkflowDecision.id } : {}, active_waiver_ids: [], audit_ids: ["AUD-DEMOA0001"], evidence_link_ids: [], resolution_ids: [], decision_ids: demoWorkflowDecision?.release_id === "REL-DEMO210001" ? [demoWorkflowDecision.id] : [], waiver_ids: [], schema_version: "1.0", created_at: demoTimestamp(-3, 16), latest_event_sequence: demoWorkflowDecision?.release_id === "REL-DEMO210001" ? 4 : 3, latest_event_hash: "a".repeat(64) },
    { id: "REL-DEMO220001", version: "3.5.0-beta", name: "iOS beta candidate", owner: "release@northstar.example", target_date: demoDate(10), submission_date: demoDate(8), rollout_date: demoDate(11), stage: "candidate", updated_at: demoTimestamp(-1, 18), candidate_ids: ["CAN-DEMOB0001"], active_candidates: { ios: "CAN-DEMOB0001" }, active_audits: {}, active_decisions: {}, active_waiver_ids: [], audit_ids: [], evidence_link_ids: [], resolution_ids: [], decision_ids: [], waiver_ids: [], schema_version: "1.0", created_at: demoTimestamp(-1, 18), latest_event_sequence: 2, latest_event_hash: "b".repeat(64) },
    ...demoCreatedReleases
  ];
  releases.forEach((release) => {
    Object.assign(release, demoReleaseSchedules.get(release.id) || {});
    if (demoWorkflowCandidate?.release_id === release.id) {
      if (!release.candidate_ids.includes(demoWorkflowCandidate.id)) release.candidate_ids.push(demoWorkflowCandidate.id);
      release.active_candidates[demoWorkflowCandidate.platform] = demoWorkflowCandidate.id;
      release.updated_at = demoWorkflowCandidate.created_at;
    }
    if (demoWorkflowAudit?.release_id === release.id) {
      if (!release.audit_ids.includes(demoWorkflowAudit.id)) release.audit_ids.push(demoWorkflowAudit.id);
      release.active_audits[demoWorkflowAudit.platform] = demoWorkflowAudit.id;
      release.stage = "in_review";
      release.updated_at = demoWorkflowAudit.recorded_at;
    }
    if (demoWorkflowDecision?.release_id === release.id) {
      release.active_decisions[demoWorkflowDecision.platform] = demoWorkflowDecision.id;
      if (!release.decision_ids.includes(demoWorkflowDecision.id)) release.decision_ids.push(demoWorkflowDecision.id);
      release.stage = demoWorkflowDecision.decision;
      release.updated_at = demoWorkflowDecision.created_at;
    }
    const evidence = demoWorkflowEvidence.filter(item => item.release_id === release.id);
    const resolutions = demoWorkflowResolutions.filter(item => item.release_id === release.id);
    const waivers = demoWorkflowWaivers.filter(item => item.release_id === release.id);
    evidence.forEach(item => { if (!release.evidence_link_ids.includes(item.id)) release.evidence_link_ids.push(item.id); });
    resolutions.forEach(item => { if (!release.resolution_ids.includes(item.id)) release.resolution_ids.push(item.id); });
    waivers.forEach(item => {
      if (!release.waiver_ids.includes(item.id)) release.waiver_ids.push(item.id);
      if (!release.active_waiver_ids.includes(item.id)) release.active_waiver_ids.push(item.id);
    });
    release.latest_event_sequence += evidence.length + resolutions.length + waivers.length;
  });
  return validateReleaseList(releases);
}

export async function ensureReleaseWorkflow(version, name = null, owner = null, actor = null) {
  if (hasTauri && !state.demoMode) return validateMobileRelease(await invoke("ensure_release_workflow_cmd", { version, name, owner, actor }));
  const releases = await listReleaseWorkflows();
  const existing = releases.find(release => release.version === version);
  if (existing) return validateMobileRelease(existing);
  const now = new Date().toISOString();
  const release = {
    schema_version: "1.0", id: `REL-DEMO${String(demoCreatedReleases.length + 1).padStart(6, "0")}`,
    version, name, owner, target_date: null, submission_date: null, rollout_date: null, stage: "draft", created_at: now, updated_at: now,
    candidate_ids: [], active_candidates: {}, audit_ids: [], active_audits: {}, evidence_link_ids: [],
    resolution_ids: [], decision_ids: [], active_decisions: {}, waiver_ids: [], active_waiver_ids: [],
    latest_event_sequence: 1, latest_event_hash: "c".repeat(64), actor
  };
  demoCreatedReleases.push(release);
  return validateMobileRelease(release);
}

export async function scheduleRelease(release, targetDate = null, submissionDate = null, rolloutDate = null, actor = null) {
  if (hasTauri && !state.demoMode) return validateMobileRelease(await invoke("schedule_release_cmd", { release, targetDate, submissionDate, rolloutDate, actor }));
  const view = await showReleaseWorkflow(release);
  const schedule = {
    target_date: targetDate || view.release.target_date || null,
    submission_date: submissionDate || view.release.submission_date || null,
    rollout_date: rolloutDate || view.release.rollout_date || null,
    updated_at: new Date().toISOString()
  };
  demoReleaseSchedules.set(view.release.id, schedule);
  return validateMobileRelease({ ...view.release, ...schedule });
}

export async function getReleasePortfolio() {
  if (hasTauri && !state.demoMode) return validateReleasePortfolio(await invoke("release_portfolio_cmd"));
  const releases = await listReleaseWorkflows();
  const views = await Promise.all(releases.map(release => showReleaseWorkflow(release.id)));
  const reviewQueue = [];
  const portfolioReleases = views.map((view) => {
    const release = view.release;
    const platforms = Object.keys(release.active_candidates || {});
    const activeAudits = platforms.map(platform => view.audits.find(audit => audit.id === release.active_audits?.[platform])).filter(Boolean);
    const activeDecisions = platforms.map(platform => view.decisions.find(decision => decision.id === release.active_decisions?.[platform])).filter(Boolean);
    const counts = activeAudits.reduce((total, audit) => {
      Object.keys(total).forEach(key => { total[key] += Number(audit.counts?.[key] || 0); });
      return total;
    }, { passed: 0, blockers: 0, warnings: 0, manual: 0, unavailable: 0 });
    const push = (kind, platform, severity, title, reason, nextAction, dueAt = null) => reviewQueue.push({
      kind, release_id: release.id, release_version: release.version, release_name: release.name,
      owner: release.owner, platform, severity, title, reason, next_action: nextAction, due_at: dueAt
    });
    const targetTime = release.target_date ? Date.parse(`${release.target_date}T00:00:00Z`) : NaN;
    const today = new Date();
    const todayTime = Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate());
    const daysToTarget = Number.isNaN(targetTime) ? null : Math.ceil((targetTime - todayTime) / 86400000);
    if (daysToTarget !== null && daysToTarget < 0) push('overdue', null, 'warning', 'Release target is overdue', `The target date passed ${Math.abs(daysToTarget)} day(s) ago.`, 'Confirm the new target date or complete the remaining release work now.', `${release.target_date}T00:00:00Z`);
    else if (daysToTarget !== null && daysToTarget <= 7) {
      const reason = daysToTarget === 0 ? 'The release target is today.' : daysToTarget === 1 ? 'The release target is tomorrow.' : `The release target is in ${daysToTarget} days.`;
      push('due_soon', null, 'review', 'Release target is approaching', reason, 'Confirm the submission, approval, and rollout owners before the target date.', `${release.target_date}T00:00:00Z`);
    }
    if (!release.owner) push('missing_owner', null, 'review', 'Release owner required', 'No accountable release owner is recorded.', 'Assign the person responsible for coordinating this release.');
    if (!platforms.length) push('missing_candidate', null, 'review', 'Candidate not bound', 'No active candidate is attached.', 'Bind the exact IPA, AAB, or APK candidate.');
    platforms.forEach((platform) => {
      const audit = view.audits.find(item => item.id === release.active_audits?.[platform]);
      const decision = view.decisions.find(item => item.id === release.active_decisions?.[platform]);
      if (!audit) push('missing_audit', platform, 'review', 'Candidate audit required', `${platform.toUpperCase()} has no active verified audit.`, 'Run the active candidate audit before requesting approval.');
      else if (audit.decision === 'no_go' || Number(audit.counts?.blockers || 0) > 0) push('blocked', platform, 'blocker', 'Automated release blockers', `${platform.toUpperCase()} is blocked by the active audit.`, 'Resolve the blocking findings and record a new audit.');
      if (decision?.decision === 'rejected') push('rejected', platform, 'blocker', 'Candidate rejected', `Rejected by ${decision.decided_by}.`, 'Address the rejection rationale and bind or audit the corrected candidate.');
      else if (decision?.decision === 'approved' && audit?.decision === 'at_risk') push('approved_with_risk', platform, 'risk', 'Approved candidate still carries open risk', `${platform.toUpperCase()} is approved, but the active audit remains at risk.`, 'Resolve or explicitly waive the remaining work, rerun the audit, and refresh the handoff packet.');
      else if (audit && audit.decision !== 'no_go' && !decision) push('approval_required', platform, 'review', 'Human approval required', `${platform.toUpperCase()} is ${String(audit.decision).replaceAll('_', ' ')} with no decision.`, 'Review the evidence and record an accountable approval or rejection.');
    });
    const activeWaivers = view.waivers.filter(waiver => release.active_waiver_ids?.includes(waiver.id) && new Date(waiver.expires_at) > new Date());
    const activeEvidence = view.evidence_links.filter(link => release.active_candidates?.[link.platform] === link.candidate_id && release.active_audits?.[link.platform] === link.source_audit_id);
    activeEvidence.filter(link => link.expires_at && new Date(link.expires_at).getTime() <= Date.now() + 7 * 86400000).forEach((link) => {
      const expired = new Date(link.expires_at).getTime() <= Date.now();
      push('expiring_evidence', link.platform, expired ? 'blocker' : 'warning', expired ? 'Release evidence expired' : 'Release evidence expires soon', `Evidence ${link.evidence_id} ${expired ? 'expired' : 'expires'} at ${link.expires_at}.`, 'Replace or renew the evidence, then rerun the candidate audit before approval.', link.expires_at);
    });
    activeWaivers.forEach((waiver) => {
      push('accepted_risk', waiver.platform, 'risk', 'Accepted risk remains active', `${waiver.approved_by} accepted ${waiver.finding_instance_id}.`, 'Confirm the waiver remains necessary and visible in the handoff.', waiver.expires_at);
      if (new Date(waiver.expires_at).getTime() <= Date.now() + 7 * 86400000) push('expiring_waiver', waiver.platform, 'warning', 'Risk acceptance expires soon', `Waiver ${waiver.id} expires soon.`, 'Renew with current evidence or resolve the underlying finding.', waiver.expires_at);
    });
    const decisions = activeAudits.map(audit => audit.decision);
    const automatedDecision = activeAudits.length !== platforms.length || !activeAudits.length ? null : decisions.includes('no_go') ? 'no_go' : decisions.includes('at_risk') ? 'at_risk' : 'go';
    const items = reviewQueue.filter(item => item.release_id === release.id);
    const ageDays = Math.max(0, Math.floor((Date.now() - new Date(release.updated_at).getTime()) / 86400000));
    return {
      id: release.id, version: release.version, name: release.name, owner: release.owner,
      target_date: release.target_date || null, submission_date: release.submission_date || null,
      rollout_date: release.rollout_date || null, days_to_target: daysToTarget, stage: release.stage,
      updated_at: release.updated_at, age_days: ageDays, platforms, automated_decision: automatedDecision,
      counts, active_decisions: activeDecisions.length, active_waivers: activeWaivers.length,
      expiring_evidence: items.filter(item => item.kind === 'expiring_evidence').length,
      expiring_waivers: items.filter(item => item.kind === 'expiring_waiver').length,
      needs_attention: items.length > 0, next_action: items[0]?.next_action || 'Preserve the verified handoff packet.',
      review_kinds: [...new Set(items.map(item => item.kind))]
    };
  });
  const counts = {
    total: portfolioReleases.length,
    needs_attention: portfolioReleases.filter(release => release.needs_attention).length,
    overdue: reviewQueue.filter(item => item.kind === 'overdue').length,
    due_soon: reviewQueue.filter(item => item.kind === 'due_soon').length,
    awaiting_approval: reviewQueue.filter(item => item.kind === 'approval_required').length,
    blocked_or_rejected: reviewQueue.filter(item => ['blocked', 'rejected'].includes(item.kind)).length,
    approved_with_risk: reviewQueue.filter(item => item.kind === 'approved_with_risk').length,
    accepted_risk: reviewQueue.filter(item => item.kind === 'accepted_risk').length,
    expiring_evidence: reviewQueue.filter(item => item.kind === 'expiring_evidence').length,
    expiring_waivers: reviewQueue.filter(item => item.kind === 'expiring_waiver').length
  };
  return validateReleasePortfolio({ schema_version: '1.0', generated_at: new Date().toISOString(), counts, releases: portfolioReleases, review_queue: reviewQueue });
}

export async function showReleaseWorkflow(release) {
  if (hasTauri && !state.demoMode) return validateReleaseView(await invoke("show_release_workflow_cmd", { release }));
  const releases = await listReleaseWorkflows();
  const workflow = releases.find(item => item.id === release || item.version === release) || releases[0];
  const isAndroid = workflow.id === "REL-DEMO210001";
  const isBeta = workflow.id === "REL-DEMO220001";
  const candidateId = isAndroid ? "CAN-DEMOA0001" : isBeta ? "CAN-DEMOB0001" : null;
  const candidatePath = isAndroid ? "build/app-release.aab" : "build/Northstar.ipa";
  const candidate = candidateId ? { schema_version: "1.0", id: candidateId, release_id: workflow.id, platform: isAndroid ? "android" : "ios", artifact_kind: isAndroid ? "aab" : "ipa", artifact_path: candidatePath, artifact_path_relative: true, artifact_sha256: demoArtifactSha(candidatePath), artifact_size_bytes: isAndroid ? 28765432 : 34127890, git_commit: DEMO_GIT_COMMIT, created_at: workflow.created_at } : null;
  const auditResult = isAndroid ? await runMobileAudit("build/app-release.aab", false) : null;
  const audit = auditResult && candidate ? { schema_version: "1.0", id: "AUD-DEMOA0001", release_id: workflow.id, candidate_id: candidateId, platform: "android", candidate_sha256: candidate.artifact_sha256, report_fingerprint: auditResult.report.report_fingerprint, policy_digest: auditResult.report.policy_digest, decision: "at_risk", counts: auditResult.report.summary, git_commit: auditResult.report.git_commit, generated_at: auditResult.report.generated_at, recorded_at: workflow.updated_at, report_path: `releases/${workflow.id}/reports/AUD-DEMOA0001.json` } : null;
  const candidates = [...(candidate ? [candidate] : []), ...(demoWorkflowCandidate?.release_id === workflow.id ? [demoWorkflowCandidate] : [])];
  const audits = [...(audit ? [audit] : []), ...(demoWorkflowAudit?.release_id === workflow.id ? [demoWorkflowAudit] : [])];
  const evidence = demoWorkflowEvidence.filter(item => item.release_id === workflow.id);
  const resolutions = demoWorkflowResolutions.filter(item => item.release_id === workflow.id);
  const waivers = demoWorkflowWaivers.filter(item => item.release_id === workflow.id);
  const mutableEvents = [];
  const eventInputs = [
    { kind: "release_created", occurred_at: workflow.created_at, actor: workflow.owner },
    ...(workflow.target_date ? [{ kind: "release_scheduled", occurred_at: workflow.updated_at, actor: workflow.owner }] : []),
    ...(candidate ? [{ kind: "candidate_bound", occurred_at: workflow.updated_at, actor: "ci@northstar.example" }] : []),
    ...(audit ? [{ kind: "candidate_audited", occurred_at: workflow.updated_at, actor: workflow.owner }] : []),
    ...(demoWorkflowCandidate?.release_id === workflow.id ? [{ kind: "candidate_bound", occurred_at: demoWorkflowCandidate.created_at, actor: demoWorkflowCandidate.actor }] : []),
    ...(demoWorkflowAudit?.release_id === workflow.id ? [{ kind: "candidate_audited", occurred_at: demoWorkflowAudit.recorded_at, actor: demoWorkflowAudit.actor }] : []),
    ...evidence.map(item => ({ kind: "evidence_linked", occurred_at: item.created_at, actor: item.actor })),
    ...resolutions.map(item => ({ kind: "finding_resolved", occurred_at: item.created_at, actor: item.actor })),
    ...waivers.map(item => ({ kind: "finding_waived", occurred_at: item.created_at, actor: item.approved_by })),
    ...(demoWorkflowDecision?.release_id === workflow.id ? [{ kind: `release_${demoWorkflowDecision.decision}`, occurred_at: demoWorkflowDecision.created_at, actor: demoWorkflowDecision.decided_by }] : [])
  ];
  eventInputs.forEach((event) => mutableEvents.push({ sequence: mutableEvents.length + 1, ...event, hash: String(mutableEvents.length + 1).repeat(64).slice(0, 64) }));
  return validateReleaseView({ release: workflow, candidates, audits, evidence_links: evidence, resolutions, decisions: demoWorkflowDecision?.release_id === workflow.id ? [demoWorkflowDecision] : [], waivers, events: mutableEvents });
}

export async function listReleaseReports(release) {
  if (hasTauri && !state.demoMode) return await invoke("list_release_reports_cmd", { release });
  const reports = [];
  if (release === "REL-DEMO210001" || release === "3.4.0") {
    reports.push((await runMobileAudit("build/app-release.aab", false)).report);
  }
  if (demoWorkflowReport && (demoWorkflowAudit?.release_id === release || demoWorkflowAudit?.release_id === (await showReleaseWorkflow(release)).release.id)) {
    reports.push(demoWorkflowReport);
  }
  return reports;
}

export async function addReleaseCandidate(release, artifactPath, platform = null, actor = null) {
  if (hasTauri && !state.demoMode) return validateSchemaRecord(await invoke("add_release_candidate_cmd", { release, artifactPath, platform, actor }), 'release candidate');
  const view = await showReleaseWorkflow(release);
  const resolvedPlatform = platform || (String(artifactPath).toLowerCase().endsWith(".ipa") ? "ios" : "android");
  const now = new Date().toISOString();
  demoWorkflowCandidate = { schema_version: "1.0", id: "CAN-DEMOBOUND1", release_id: view.release.id, platform: resolvedPlatform, artifact_kind: String(artifactPath).split(".").pop().toLowerCase(), artifact_path: artifactPath, artifact_path_relative: true, artifact_sha256: demoArtifactSha(artifactPath), artifact_size_bytes: resolvedPlatform === "ios" ? 34127890 : 30485760, git_commit: DEMO_GIT_COMMIT, created_at: now, actor };
  demoWorkflowAudit = null;
  demoWorkflowReport = null;
  return validateSchemaRecord(demoWorkflowCandidate, 'release candidate');
}

export async function auditReleaseCandidate(release, platform = null, actor = null) {
  if (hasTauri && !state.demoMode) return validateReleaseAuditResult(await invoke("audit_release_candidate_cmd", { release, platform, actor }));
  const view = await showReleaseWorkflow(release);
  const resolvedPlatform = platform || Object.keys(view.release.active_candidates || {})[0];
  const candidateId = view.release.active_candidates[resolvedPlatform];
  const candidate = view.candidates.find(item => item.id === candidateId);
  if (!candidate) throw new Error(`No active ${resolvedPlatform || "mobile"} candidate is bound.`);
  demoWorkflowReport = (await runMobileAudit(candidate.artifact_path, false)).report;
  const now = new Date().toISOString();
  demoWorkflowAudit = { schema_version: "1.0", id: "AUD-DEMONEW0001", release_id: view.release.id, candidate_id: candidate.id, platform: resolvedPlatform, candidate_sha256: candidate.artifact_sha256, report_fingerprint: demoWorkflowReport.report_fingerprint, policy_digest: demoWorkflowReport.policy_digest, decision: demoWorkflowReport.decision, counts: demoWorkflowReport.summary, git_commit: demoWorkflowReport.git_commit, generated_at: demoWorkflowReport.generated_at, recorded_at: now, report_path: `releases/${view.release.id}/reports/AUD-DEMONEW0001.json`, actor };
  return validateReleaseAuditResult({ schema_version: "1.0", audit: demoWorkflowAudit, report: demoWorkflowReport });
}

export async function addReleaseEvidence(release, platform, findingInstanceId, mobileEvidenceKind, title, issuer, url = null, expiresAt = null, actor = null) {
  if (hasTauri && !state.demoMode) return await invoke("add_release_evidence_cmd", { input: { release, platform, findingInstanceId, mobileEvidenceKind, title, issuer, url, expiresAt, actor } });
  const view = await showReleaseWorkflow(release);
  const resolvedPlatform = platform || Object.keys(view.release.active_candidates || {})[0];
  const candidateId = view.release.active_candidates[resolvedPlatform];
  const auditId = view.release.active_audits[resolvedPlatform];
  if (!candidateId || !auditId) throw new Error(`Audit the active ${resolvedPlatform || "mobile"} candidate before attaching evidence.`);
  const now = new Date().toISOString();
  const link = { schema_version: "1.0", id: `LNK-DEMO${String(demoWorkflowEvidence.length + 1).padStart(4, "0")}`, evidence_id: `EVI-DEMO${String(demoWorkflowEvidence.length + 1).padStart(4, "0")}`, evidence_digest: "e".repeat(64), release_id: view.release.id, candidate_id: candidateId, source_audit_id: auditId, platform: resolvedPlatform, finding_instance_id: findingInstanceId, mobile_evidence_kind: mobileEvidenceKind, actor: actor || issuer, created_at: now, expires_at: expiresAt };
  demoWorkflowEvidence.push(link);
  return { schema_version: "1.0", link, evidence: { id: link.evidence_id, kind: "mobile_attestation", title, source: "release_workflow", url, expires_at: expiresAt } };
}

export async function resolveReleaseFinding(release, platform, findingInstanceId, reason, evidenceIds = [], actor = null) {
  if (hasTauri && !state.demoMode) return await invoke("resolve_release_finding_cmd", { release, platform, findingInstanceId, reason, evidenceIds, actor });
  const view = await showReleaseWorkflow(release);
  const resolvedPlatform = platform || Object.keys(view.release.active_candidates || {})[0];
  const candidateId = view.release.active_candidates[resolvedPlatform];
  const auditId = view.release.active_audits[resolvedPlatform];
  if (!candidateId || !auditId) throw new Error(`Audit the active ${resolvedPlatform || "mobile"} candidate before resolving findings.`);
  const resolution = { schema_version: "1.0", id: `RES-DEMO${String(demoWorkflowResolutions.length + 1).padStart(4, "0")}`, release_id: view.release.id, candidate_id: candidateId, source_audit_id: auditId, platform: resolvedPlatform, finding_instance_id: findingInstanceId, status: "resolved", reason, evidence_ids: evidenceIds, actor, created_at: new Date().toISOString() };
  demoWorkflowResolutions.push(resolution);
  return resolution;
}

export async function waiveReleaseFinding(release, platform, findingInstanceId, reason, approvedBy, expiresAt) {
  if (hasTauri && !state.demoMode) return await invoke("waive_release_finding_cmd", { release, platform, findingInstanceId, reason, approvedBy, expiresAt });
  const view = await showReleaseWorkflow(release);
  const resolvedPlatform = platform || Object.keys(view.release.active_candidates || {})[0];
  const candidateId = view.release.active_candidates[resolvedPlatform];
  const auditId = view.release.active_audits[resolvedPlatform];
  const candidate = view.candidates.find(item => item.id === candidateId);
  const audit = view.audits.find(item => item.id === auditId);
  if (!candidate || !audit) throw new Error(`Audit the active ${resolvedPlatform || "mobile"} candidate before accepting risk.`);
  const waiver = { schema_version: "1.0", id: `WAV-DEMO${String(demoWorkflowWaivers.length + 1).padStart(4, "0")}`, release_id: view.release.id, candidate_id: candidate.id, candidate_sha256: candidate.artifact_sha256, audit_id: audit.id, platform: resolvedPlatform, report_fingerprint: audit.report_fingerprint, policy_digest: audit.policy_digest, finding_instance_id: findingInstanceId, reason, approved_by: approvedBy, created_at: new Date().toISOString(), expires_at: expiresAt, status: "active" };
  demoWorkflowWaivers.push(waiver);
  return waiver;
}

export async function writeReleasePacket(release) {
  if (hasTauri && !state.demoMode) return await invoke("write_release_packet_cmd", { release });
  const view = await showReleaseWorkflow(release);
  const activeAudits = Object.keys(view.release.active_candidates || {}).map(platform => view.audits.find(audit => audit.id === view.release.active_audits?.[platform])).filter(Boolean);
  const activeDecisions = Object.keys(view.release.active_candidates || {}).map(platform => view.decisions.find(decision => decision.id === view.release.active_decisions?.[platform])).filter(Boolean);
  const blocked = activeAudits.some(audit => audit.decision === 'no_go');
  const rejected = activeDecisions.some(decision => decision.decision === 'rejected');
  const pending = activeDecisions.length < Object.keys(view.release.active_candidates || {}).length;
  const approvedWithRisk = activeDecisions.some(decision => decision.decision === 'approved') && activeAudits.some(audit => audit.decision === 'at_risk');
  const status = blocked ? 'BLOCKED' : rejected ? 'REJECTED' : pending ? 'AWAITING APPROVAL' : approvedWithRisk ? 'APPROVED WITH RISK' : 'APPROVED HANDOFF';
  const next = blocked ? 'Resolve blockers and rerun the candidate audit.' : pending ? 'Review the evidence and record the missing decision.' : approvedWithRisk ? 'Resolve or waive remaining findings, rerun the audit, and refresh this handoff.' : 'Transfer the exact approved artifacts with this verified packet.';
  const handoff = `## Handoff summary\n\n- Status: **${status}**\n- Owner: ${view.release.owner || 'Unassigned'}\n- Target date: ${view.release.target_date || 'Unscheduled'}\n- Submission date: ${view.release.submission_date || 'Unscheduled'}\n- Rollout date: ${view.release.rollout_date || 'Unscheduled'}\n- Exact candidates: ${Object.keys(view.release.active_candidates || {}).length}\n- Next action: ${next}\n`;
  return { packet: { packet_id: "PKT-DEMO000001", packet_digest: "d".repeat(64), release: view.release }, json_path: `releases/${view.release.id}/packets/PKT-DEMO000001.json`, markdown_path: `releases/${view.release.id}/packets/PKT-DEMO000001.md`, html_path: `releases/${view.release.id}/packets/PKT-DEMO000001.html`, handoff_summary: handoff };
}

export async function diffReleaseCandidates(release, platform, fromCandidateId = null, toCandidateId = null) {
  if (hasTauri && !state.demoMode) return await invoke("diff_release_candidates_cmd", { release, platform, fromCandidateId, toCandidateId });
  return { release_id: release, platform, from_candidate: { id: fromCandidateId || "CAN-PREVIOUS01", artifact_sha256: "1".repeat(64) }, to_candidate: { id: toCandidateId || "CAN-DEMOA0001", artifact_sha256: "8f".repeat(32) }, artifact_size_delta_bytes: 143220, git_commit_changed: true, policy_changed: false, automated_decision_changed: true, introduced_findings: [], resolved_findings: [{ finding_instance_id: "MOB-CFG-006:android", title: "Target SDK updated" }], changed_findings: [], related_events: [] };
}

export async function decideRelease(release, platform, decision, decidedBy, role, rationale) {
  if (hasTauri && !state.demoMode) return await invoke("decide_release_cmd", { release, platform, decision, decidedBy, role, rationale });
  const view = await showReleaseWorkflow(release);
  const resolvedPlatform = platform || Object.keys(view.release.active_candidates || {})[0];
  const candidateId = view.release.active_candidates[resolvedPlatform];
  const auditId = view.release.active_audits[resolvedPlatform];
  const candidate = view.candidates.find(item => item.id === candidateId);
  const audit = view.audits.find(item => item.id === auditId);
  if (!candidate || !audit) throw new Error(`Audit the active ${resolvedPlatform || "mobile"} candidate before recording a decision.`);
  demoWorkflowDecision = { id: "DEC-DEMO000001", release_id: view.release.id, candidate_id: candidate.id, candidate_sha256: candidate.artifact_sha256, audit_id: audit.id, platform: resolvedPlatform, report_fingerprint: audit.report_fingerprint, policy_digest: audit.policy_digest, decision, decided_by: decidedBy, role, rationale, created_at: new Date().toISOString(), schema_version: "1.0" };
  return demoWorkflowDecision;
}
