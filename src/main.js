import { state, subscribe, setDemoMode, setCachedIndex, updateSimConfig } from './modules/state.js';
import { fetchLaunchIndex, saveFileContent, hasTauri, watchVault, getWorkspacePath, pickProjectFolder, fetchRecentChanges, openInIde, runAudit, getGitUser, checkGitStatus } from './modules/api.js';
import { renderDashboard, renderTerminalHeader, renderHeader } from './modules/renderers/dashboard.js';
import { renderBlockerBoard } from './modules/renderers/blockers.js';
import { renderQARegression } from './modules/renderers/qa.js';
import { renderGoNoGo } from './modules/renderers/gonogo.js';
import { openFileInEditor, renderEditor, initEditorTabs, hasDirtyEditorTabs, saveActiveEditor, initSidebarToggle, initPreviewModeButtons, attachInputListener, attachPreviewCheckboxListener, attachSmartListListener, attachMarkdownShortcutListener } from './modules/renderers/editor.js';
import { renderAuditCockpit } from './modules/renderers/audit.js';
import { renderDocsHealth } from './modules/renderers/health.js';
import { renderTimeline } from './modules/renderers/timeline.js';
import { renderCiSetup } from './modules/renderers/cisetup.js';
import { renderSettings } from './modules/renderers/settings.js';
import { openMobileWorkspace, renderMobileRelease } from './modules/renderers/mobile.js?v=0.5.2';
import { switchView, toggleSpotlight, setNoVaultState, renderSpotlightResults, showAuditPreviewModal, initCreatorWizards, initOnboardingWizard } from './modules/ui.js';
import { startCountdown, startGantryLogSimulation } from './modules/utils.js';
import { mockLaunchIndex } from './modules/constants.js';
import { runUpdateCheck } from './modules/updater.js';
import { initGraphView } from './modules/graph_view.js';
import { normalizeIndexPayload } from './modules/contract.js';

window.switchView = switchView;
window.openFileInEditor = openFileInEditor;
window.refreshUI = refreshUI;
window.hasDirtyEditorTabs = hasDirtyEditorTabs;
window.hasTauri = hasTauri;
window.checkForUpdates = () => runUpdateCheck({ silent: false });

let vaultUnlisten = null;

export async function setupVaultWatcher() {
  if (vaultUnlisten) {
    try {
      vaultUnlisten();
      vaultUnlisten = null;
    } catch (e) {
      console.warn("Failed to unlisten from vault changes:", e);
    }
  }

  if (hasTauri) {
    let watchTimeout;
    const unlisten = await watchVault(() => {
      clearTimeout(watchTimeout);
      watchTimeout = setTimeout(async () => {
        if (state.editorDirty && state.activeView === 'editor') {
          // Refresh everything except the editor textarea content
          await refreshUI();
        } else {
          await refreshUI();
        }
      }, 100);
    });
    if (unlisten) {
      vaultUnlisten = unlisten;
    }
  }
}

window.setupVaultWatcher = setupVaultWatcher;

async function updateGitStatusIndicator() {
  const dot  = document.getElementById("git-status-dot");
  const text = document.getElementById("git-status-text");
  const badge = document.getElementById("git-status-badge");
  if (!dot || !text) return;

  const status = await checkGitStatus();
  if (status.is_git_repo) {
    dot.classList.remove("no-git");
    text.textContent = "Git \u2713";
    if (badge) badge.title = `Repo: ${status.repo_root}`;
  } else {
    dot.classList.add("no-git");
    text.textContent = "No Git";
    if (badge) badge.title = "No git repository found in this project folder";
  }
}

async function updateSidebarBadges() {
  const blockerBadge = document.getElementById("sidebar-blocker-badge");
  const commitBadge = document.getElementById("sidebar-commit-badge");
  const { cachedIndex } = state;

  const activeBlockers = (cachedIndex.blockers || []).filter(b => b.status !== "resolved");
  if (blockerBadge) {
    blockerBadge.style.display = activeBlockers.length > 0 ? "inline-flex" : "none";
    if (activeBlockers.length > 0) blockerBadge.textContent = activeBlockers.length;
  }

  const changes = await fetchRecentChanges();
  if (commitBadge) {
    commitBadge.style.display = (changes && changes.length > 0) ? "inline-flex" : "none";
    if (changes && changes.length > 0) commitBadge.textContent = changes.length;
  }

  const editorDirtyBadge = document.getElementById("sidebar-editor-dirty-badge");
  if (editorDirtyBadge) {
    editorDirtyBadge.style.display = hasDirtyEditorTabs() ? "inline-flex" : "none";
  }
}

function recomputeSimulatedReadiness() {
  const { simConfig, realIndex, cachedIndex } = state;
  if (!simConfig.active) return;

  const source = realIndex || cachedIndex;
  const sim = JSON.parse(JSON.stringify(source));

  if (simConfig.waivedBlocker) {
    sim.blockers = sim.blockers.map(b =>
      b.id === simConfig.waivedBlocker ? { ...b, status: "resolved", waived: true } : b
    );
  }

  if (simConfig.simulateFile) {
    if (simConfig.simulateFile === "lib/routing/checkout_router.dart") {
      if (!sim.stale_docs.includes("blockers/BLK-102-android-deep-link.md"))
        sim.stale_docs.push("blockers/BLK-102-android-deep-link.md");
      sim.blockers = sim.blockers.map(b => b.id === "BLK-102" ? { ...b, stale: true } : b);
    } else if (simConfig.simulateFile === "android/app/src/main/AndroidManifest.xml") {
      if (!sim.stale_docs.includes("qa/regression.md"))
        sim.stale_docs.push("qa/regression.md");
    } else {
      const fileKey = simConfig.simulateFile.replace(/\//g, "-");
      const ev = {
        id: `uncovered-change-${fileKey}`,
        kind: "source_file",
        path: simConfig.simulateFile,
        line: null,
        message: "Modified source code missing QA coverage or blocker reference",
        severity: "warning",
        related_finding: "UNCOVERED_CODE_CHANGES",
        suggested_action: `Add a QA plan or blocker for ${simConfig.simulateFile}`
      };
      sim.validation_errors.push({ severity: "warning", file: simConfig.simulateFile, code: "UNCOVERED_CODE_CHANGES", message: ev.message, field: null, evidence: ev });
      sim.evidence_graph.push(ev);
    }
  }

  sim.open_blockers = sim.blockers.filter(b => b.status !== "resolved").length;

  const qaTotal = sim.qa_total || 7;
  sim.qa_completed = Math.round((simConfig.qaPct / 100) * qaTotal);
  sim.qa_total = qaTotal;

  sim.signoffs = [
    { name: "@pm_dan",   role: "Product",      status: simConfig.PM  ? "Approved"    : "Pending",     date: simConfig.PM  ? new Date().toISOString().split("T")[0] : "" },
    { name: "@dev_jane", role: "Engineering",  status: simConfig.Dev ? "Approved"    : "In Progress", date: simConfig.Dev ? new Date().toISOString().split("T")[0] : "" },
    { name: "@qa_bob",   role: "QA",           status: simConfig.QA  ? "Approved"    : "Pending",     date: simConfig.QA  ? new Date().toISOString().split("T")[0] : "" }
  ];

  let engScore = Math.round(simConfig.crashRate) - sim.open_blockers * 30;
  if (engScore < 0) engScore = 0;
  const qaScore = Math.round(simConfig.qaPct);
  const prodScore = simConfig.PM ? 100 : 50;
  let total = Math.min(100, Math.max(0, Math.round(engScore * 0.4 + qaScore * 0.3 + prodScore * 0.3)));

  if (sim.open_blockers > 0 && total > 49) {
    total = 49;
    if (!sim.score_reasons) sim.score_reasons = {};
    sim.score_reasons["Cap"] = ["Readiness score capped at 49% due to active launch blockers."];
  }

  sim.readiness_score = total;
  sim.category_scores = {
    "Product": prodScore, "Engineering": engScore, "QA": qaScore,
    "Store": source.category_scores?.Store ?? 70,
    "Billing": source.category_scores?.Billing ?? 100,
    "Monitoring": source.category_scores?.Monitoring ?? 100,
    "Support": source.category_scores?.Support ?? 50
  };
  sim.readiness = sim.open_blockers > 0 ? "No-Go" : (total >= 90 && simConfig.PM && simConfig.Dev && simConfig.QA ? "Go" : "At Risk");

  state.cachedIndex = normalizeIndexPayload(sim, state.activeEnv);
}

function populateWaiveDropdown() {
  const select = document.getElementById("sandbox-waive-blocker");
  if (!select) return;
  const current = select.value;
  select.textContent = "";

  const noneOpt = document.createElement("option");
  noneOpt.value = "";
  noneOpt.textContent = "-- None --";
  select.appendChild(noneOpt);

  (state.cachedIndex.blockers || [])
    .filter(b => b.status !== "resolved")
    .forEach(b => {
      const opt = document.createElement("option");
      opt.value = b.id;
      opt.textContent = `${b.id}: ${b.title}`;
      select.appendChild(opt);
    });

  // Restore prior selection if still valid
  if (current && [...select.options].some(o => o.value === current)) {
    select.value = current;
  }
}

// Sync workspace Chrome elements (banner and path displays)
async function syncWorkspaceChrome() {
  const banner = document.getElementById("demo-mode-banner");
  if (banner) {
    banner.style.display = state.demoMode ? "flex" : "none";
  }
  const btnSettingsTryDemo = document.getElementById("btn-settings-try-demo");
  if (btnSettingsTryDemo) {
    btnSettingsTryDemo.style.display = state.demoMode ? "none" : "inline-flex";
  }
  const wsEl = document.getElementById("workspace-path-display");
  if (wsEl) {
    if (state.demoMode) {
      wsEl.value = "Northstar Mobile (sample)";
    } else {
      const workspacePath = await getWorkspacePath();
      wsEl.value = workspacePath || "";
    }
  }
}

function resetGantryLogSimulation() {
  if (window.__gantryLogInterval) {
    clearInterval(window.__gantryLogInterval);
  }
  const streamEl = document.getElementById("gantry-log-stream");
  if (streamEl) {
    streamEl.textContent = "";
  }
  window.__gantryLogInterval = startGantryLogSimulation(streamEl);
}

// Shared demo loading flow
async function loadDemoVault() {
  if (hasDirtyEditorTabs()) {
    if (!confirm("You have unsaved changes in your tabs. Discard them and load the demo vault?")) {
      return;
    }
  }
  const { setOpenEditorTabs, setActiveFilePath } = await import('./modules/state.js');
  setOpenEditorTabs([]);
  setActiveFilePath("");
  setDemoMode(true);
  setNoVaultState(false);
  await initEditorTabs();
  resetGantryLogSimulation();
  switchView("mobile");
  await openMobileWorkspace("releases", "releases");
}

// Exit demo mode to project setup flow
async function exitDemoModeToSetup() {
  if (hasDirtyEditorTabs()) {
    if (!confirm("You have unsaved changes in your tabs. Exiting demo mode will discard them. Proceed?")) {
      return;
    }
  }
  const { setOpenEditorTabs, setActiveFilePath } = await import('./modules/state.js');
  setOpenEditorTabs([]);
  setActiveFilePath("");
  setDemoMode(false);
  resetGantryLogSimulation();
  await refreshUI();
}

// Project folder picker — refresh UI after selection
async function handlePickProject() {
  if (hasDirtyEditorTabs()) {
    if (!confirm("You have unsaved changes in your tabs. Switching projects will discard them. Proceed?")) {
      return;
    }
  }
  const path = await pickProjectFolder();
  if (path) {
    const wsEl = document.getElementById("workspace-path-display");
    if (wsEl) wsEl.value = path;
    setDemoMode(false);
    try {
      const { setOpenEditorTabs, setActiveFilePath } = await import('./modules/state.js');
      setOpenEditorTabs([]);
      setActiveFilePath("");
      await initEditorTabs();
      await setupVaultWatcher();
      resetGantryLogSimulation();
      await refreshUI();
      setNoVaultState(false);
    } catch (e) {
      setNoVaultState(true);
    }
    updateGitStatusIndicator();
  }
}

async function refreshUI() {
  await syncWorkspaceChrome();

  try {
    const index = await fetchLaunchIndex(state.demoMode, state.activeEnv);
    state.realIndex = index;
    setNoVaultState(false);
  } catch (err) {
    if (!state.simConfig.active) {
      setNoVaultState(true);
      clearInterval(window.__gantryCountdownInterval);
      startCountdown(
        null,
        document.getElementById("countdown-display"),
        document.getElementById("countdown-target-date")
      );
      return;
    }
  }

  if (state.simConfig.active) {
    recomputeSimulatedReadiness();
  } else if (state.realIndex) {
    setCachedIndex(state.realIndex);
  }

  renderTerminalHeader();
  renderHeader();
  await updateSidebarBadges();
  populateWaiveDropdown();

  // Refresh active release target countdown dynamically
  clearInterval(window.__gantryCountdownInterval);
  window.__gantryCountdownInterval = startCountdown(
    state.cachedIndex?.target_date || state.cachedIndex?.releaseDate,
    document.getElementById("countdown-display"),
    document.getElementById("countdown-target-date")
  );

  switch (state.activeView) {
    case 'dashboard': await renderDashboard(); break;
    case 'blockers':  renderBlockerBoard(); break;
    case 'qa':        await renderQARegression(); break;
    case 'gonogo':    renderGoNoGo(); break;
    case 'health':    renderDocsHealth(); break;
    case 'timeline':  await renderTimeline(); break;
    case 'editor':    renderEditor(); break;
    case 'audit':     await renderAuditCockpit(); break;
    case 'ci-setup':  await renderCiSetup(); break;
    case 'settings':  await renderSettings(); break;
    case 'mobile':    await renderMobileRelease(); break;
  }
}

window.addEventListener("DOMContentLoaded", async () => {
  const savedTheme = localStorage.getItem("gantry-theme");
  if (savedTheme && savedTheme !== 'default') {
    document.body.classList.add(`theme-${savedTheme}`);
  }

  const workspacePath = await getWorkspacePath();
  const workspaceEl = document.getElementById("workspace-path-display");
  if (workspaceEl) workspaceEl.value = workspacePath || "";

  // Initialize tabs Working Set
  await initEditorTabs();
  initCreatorWizards();
  initOnboardingWizard({
    onChangeFolder: handlePickProject,
    onTryDemo: loadDemoVault
  });

  await refreshUI();
  updateGitStatusIndicator();

  window.__gantryLogInterval = startGantryLogSimulation(document.getElementById("gantry-log-stream"));

  // Load git user profile once on startup
  getGitUser().then(user => {
    const nameEl = document.getElementById("profile-name");
    const avatarEl = document.getElementById("profile-avatar");
    if (nameEl && user.name) nameEl.textContent = user.name;
    if (avatarEl && user.name) {
      const parts = user.name.trim().split(/\s+/);
      avatarEl.textContent = parts.length >= 2
        ? (parts[0][0] + parts[parts.length - 1][0]).toUpperCase()
        : user.name.substring(0, 2).toUpperCase();
    }
  }).catch(() => {});

  // Profile card → open Settings
  const profileCard = document.getElementById("sidebar-profile-card");
  if (profileCard) {
    profileCard.setAttribute("role", "button");
    profileCard.setAttribute("tabindex", "0");
    profileCard.addEventListener("click", () => switchView("settings"));
    profileCard.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        switchView("settings");
      }
    });
  }

  // Nav items
  document.querySelectorAll(".nav-item").forEach(item => {
    item.setAttribute("tabindex", "0");
    const activate = async () => {
      switchView(item.getAttribute("data-view"));
      if (item.dataset.mobileWorkspace) await openMobileWorkspace(item.dataset.mobileWorkspace, item.dataset.mobileContext);
    };
    item.addEventListener("click", activate);
    item.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        activate();
      }
    });
  });

  // Header refresh
  const refreshBtn = document.getElementById("header-refresh-btn");
  if (refreshBtn) {
    refreshBtn.addEventListener("click", async () => {
      refreshBtn.classList.add("spinning");
      await refreshUI();
      setTimeout(() => refreshBtn.classList.remove("spinning"), 500);
    });
  }

  // Editor: initialize sidebar, preview modes, input listener
  initSidebarToggle();
  initPreviewModeButtons();
  attachInputListener();
  attachPreviewCheckboxListener();
  attachSmartListListener();
  attachMarkdownShortcutListener();

  // Editor: save (delegated to editor.js)
  const btnSaveFile = document.getElementById("btn-save-file");
  if (btnSaveFile) btnSaveFile.addEventListener("click", saveActiveEditor);

  // Editor: open in IDE
  const btnOpenIde = document.getElementById("btn-open-ide");
  if (btnOpenIde) {
    btnOpenIde.addEventListener("click", async () => {
      if (hasTauri) {
        try { await openInIde(state.activeFilePath); }
        catch (err) { alert(`Failed to open in IDE: ${err}`); }
      } else {
        alert(`Tauri CLI simulator: Open ${state.activeFilePath} in IDE.`);
      }
    });
  }

  // Editor: Ctrl/Cmd+S to save
  document.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "s" && state.activeView === "editor") {
      e.preventDefault();
      saveActiveEditor();
    }
  });

  // Editor: fullscreen toggle
  const btnFullscreen = document.getElementById("btn-editor-fullscreen");
  if (btnFullscreen) {
    const workspace = document.querySelector(".editor-workspace");
    const iconExpand = document.getElementById("icon-expand");
    const iconCompress = document.getElementById("icon-compress");
    const toggleFullscreen = () => {
      const isFs = workspace.classList.toggle("editor-fullscreen");
      if (iconExpand) iconExpand.style.display = isFs ? "none" : "";
      if (iconCompress) iconCompress.style.display = isFs ? "" : "none";
    };
    btnFullscreen.addEventListener("click", toggleFullscreen);
    document.addEventListener("keydown", (e) => {
      if (e.key === "F11" && state.activeView === "editor") {
        e.preventDefault();
        toggleFullscreen();
      }
      if (e.key === "Escape" && workspace.classList.contains("editor-fullscreen")) {
        workspace.classList.remove("editor-fullscreen");
        if (iconExpand) iconExpand.style.display = "";
        if (iconCompress) iconCompress.style.display = "none";
      }
    });
  }

  // Spotlight / search
  const searchBtn = document.getElementById("search-trigger-btn");
  const spotlightOverlay = document.getElementById("spotlight-overlay");
  const spotlightInput = document.getElementById("spotlight-input");

  if (searchBtn) searchBtn.addEventListener("click", () => toggleSpotlight(true));
  if (spotlightOverlay) {
    spotlightOverlay.addEventListener("click", (e) => {
      if (e.target === spotlightOverlay) toggleSpotlight(false);
    });
  }
  if (spotlightInput) {
    spotlightInput.addEventListener("input", (e) => renderSpotlightResults(e.target.value));
  }

  window.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "k") {
      e.preventDefault();
      toggleSpotlight();
    }
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.code === "Space") {
      e.preventDefault();
      toggleSpotlight();
    }
    if (e.key === "Escape" && spotlightOverlay?.classList.contains("active")) {
      toggleSpotlight(false);
    }
  });

  const btnChangeProject = document.getElementById("btn-change-project");
  if (btnChangeProject) btnChangeProject.addEventListener("click", handlePickProject);

  const btnOpenOverlay = document.getElementById("btn-open-project-overlay");
  if (btnOpenOverlay) btnOpenOverlay.addEventListener("click", handlePickProject);

  const btnSettingsTryDemo = document.getElementById("btn-settings-try-demo");
  if (btnSettingsTryDemo) btnSettingsTryDemo.addEventListener("click", loadDemoVault);

  const btnDemoExit = document.getElementById("btn-demo-exit");
  if (btnDemoExit) btnDemoExit.addEventListener("click", exitDemoModeToSetup);

  const btnDemoChangeFolder = document.getElementById("btn-demo-change-folder");
  if (btnDemoChangeFolder) btnDemoChangeFolder.addEventListener("click", handlePickProject);

  // Demo mode
  const demoLabel = document.getElementById("demo-vault-label");
  if (demoLabel) {
    const name = mockLaunchIndex.vault_name.replace(/\s*\(.*?\)\s*$/, "").trim();
    demoLabel.textContent = `${name} · demo data`;
  }

  const btnLoadDemo = document.getElementById("btn-load-demo");
  if (btnLoadDemo) {
    btnLoadDemo.addEventListener("click", loadDemoVault);
  }

  // App reload / close tab warning for dirty changes
  window.addEventListener("beforeunload", (e) => {
    if (hasDirtyEditorTabs()) {
      e.preventDefault();
      e.returnValue = "You have unsaved changes in one or more tabs. Are you sure you want to leave?";
      return e.returnValue;
    }
  });

  // Audit button
  const btnRunAudit = document.getElementById("btn-run-audit");
  if (btnRunAudit) {
    btnRunAudit.addEventListener("click", async () => {
      btnRunAudit.disabled = true;
      const btnSpan = btnRunAudit.querySelector("span");
      const spinnerSvg = btnRunAudit.querySelector(".spinner-icon");
      const originalText = btnSpan ? btnSpan.textContent : "Run Launch Audit";
      if (btnSpan) btnSpan.textContent = "Analyzing Workspace...";
      if (spinnerSvg) spinnerSvg.classList.add("spinning");

      const resetBtn = () => {
        btnRunAudit.disabled = false;
        if (btnSpan) btnSpan.textContent = originalText;
        if (spinnerSvg) spinnerSvg.classList.remove("spinning");
      };

      try {
        if (hasTauri) {
          const dryRunIndex = await runAudit(false);
          resetBtn();
          showAuditPreviewModal(dryRunIndex.audit_preview, async () => {
            btnRunAudit.disabled = true;
            if (btnSpan) btnSpan.textContent = "Writing Changes...";
            if (spinnerSvg) spinnerSvg.classList.add("spinning");
            try {
              const writeIndex = await runAudit(true);
              setCachedIndex(writeIndex);
              await refreshUI();
            } catch (err) {
              console.error("Write audit failed:", err);
              alert(`Write audit failed: ${err}`);
            } finally { resetBtn(); }
          });
        } else {
          await new Promise(resolve => setTimeout(resolve, 1000));
          resetBtn();
          const mockPreview = {
            documents_to_create: ["release-notes.md", "engineering-notes.md", "runbook.md", "postlaunch.md"],
            tasks_to_create: [
              "Create missing required document: README-launch.md",
              "Create missing required document: tasks.md",
              "Create missing required document: AGENTS.md"
            ],
            tasks_to_auto_resolve: [],
            affected_files: [".gantry/index.json", ".gantry/tasks.md", ".gantry/release-notes.md"]
          };
          showAuditPreviewModal(mockPreview, async () => {
            setCachedIndex({ ...state.cachedIndex, readiness_score: 65, category_scores: { Product: 80, Engineering: 50, QA: 40, Store: 70, Billing: 100, Monitoring: 100, Support: 50 } });
            await refreshUI();
          });
        }
      } catch (err) {
        console.error("Audit failed:", err);
        alert(`Audit failed: ${err}`);
        resetBtn();
      }
    });
  }

  // Sandbox sliders
  const sandboxCrashRate = document.getElementById("sandbox-crash-rate");
  const sandboxCrashVal  = document.getElementById("sandbox-crash-val");
  const sandboxQAPct     = document.getElementById("sandbox-qa-pct");
  const sandboxQAVal     = document.getElementById("sandbox-qa-val");
  const sandboxWaive     = document.getElementById("sandbox-waive-blocker");
  const sandboxFile      = document.getElementById("sandbox-simulate-file");
  const sandboxPM        = document.getElementById("sandbox-signoff-pm");
  const sandboxDev       = document.getElementById("sandbox-signoff-dev");
  const sandboxQA        = document.getElementById("sandbox-signoff-qa");
  const btnSandboxApply  = document.getElementById("btn-sandbox-apply");
  const btnSandboxClear  = document.getElementById("btn-sandbox-clear");

  if (sandboxCrashRate && sandboxCrashVal) {
    sandboxCrashRate.addEventListener("input", (e) => {
      sandboxCrashVal.textContent = `${parseFloat(e.target.value).toFixed(1)}%`;
    });
  }
  if (sandboxQAPct && sandboxQAVal) {
    sandboxQAPct.addEventListener("input", (e) => {
      sandboxQAVal.textContent = `${e.target.value}%`;
    });
  }

  if (btnSandboxApply) {
    btnSandboxApply.addEventListener("click", async () => {
      updateSimConfig({
        active: true,
        crashRate: parseFloat(sandboxCrashRate?.value ?? 99.8),
        qaPct: parseInt(sandboxQAPct?.value ?? 43),
        waivedBlocker: sandboxWaive?.value ?? "",
        simulateFile: sandboxFile?.value ?? "",
        PM: sandboxPM?.checked ?? false,
        Dev: sandboxDev?.checked ?? false,
        QA: sandboxQA?.checked ?? false,
      });
      await refreshUI();
    });
  }

  if (btnSandboxClear) {
    btnSandboxClear.addEventListener("click", () => {
      updateSimConfig({ active: false, crashRate: 99.8, qaPct: 43, waivedBlocker: "", simulateFile: "", PM: false, Dev: false, QA: false });
      if (sandboxCrashRate) { sandboxCrashRate.value = 99.8; if (sandboxCrashVal) sandboxCrashVal.textContent = "99.8%"; }
      if (sandboxQAPct) { sandboxQAPct.value = 43; if (sandboxQAVal) sandboxQAVal.textContent = "43%"; }
      if (sandboxWaive) sandboxWaive.value = "";
      if (sandboxFile) sandboxFile.value = "";
      if (sandboxPM) sandboxPM.checked = false;
      if (sandboxDev) sandboxDev.checked = false;
      if (sandboxQA) sandboxQA.checked = false;
      refreshUI();
    });
  }

  // CI/CD copy buttons
  document.querySelectorAll(".btn-copy").forEach(btn => {
    btn.addEventListener("click", async () => {
      const codeEl = document.getElementById(btn.getAttribute("data-target"));
      if (!codeEl) return;
      try {
        await navigator.clipboard.writeText(codeEl.textContent);
        const orig = btn.textContent;
        btn.textContent = "Copied!";
        btn.classList.add("copied");
        setTimeout(() => { btn.textContent = orig; btn.classList.remove("copied"); }, 2000);
      } catch (err) { console.error("Failed to copy:", err); }
    });
  });

  // CI/CD config tabs
  document.querySelectorAll(".config-tab-btn").forEach(btn => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".config-tab-btn").forEach(b => b.classList.remove("active"));
      document.querySelectorAll(".config-content").forEach(c => c.classList.remove("active"));
      btn.classList.add("active");
      const content = document.getElementById(btn.getAttribute("data-tab"));
      if (content) content.classList.add("active");
    });
  });

  // Launch status badge → Go/No-Go Room
  const launchStatusBadge = document.getElementById("launch-status-indicator");
  if (launchStatusBadge) {
    launchStatusBadge.setAttribute("role", "button");
    launchStatusBadge.setAttribute("tabindex", "0");
    launchStatusBadge.addEventListener("click", () => switchView("gonogo"));
    launchStatusBadge.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        switchView("gonogo");
      }
    });
  }

  // Dashboard navigation shortcuts
  const btnGotoGonogo = document.getElementById("btn-goto-gonogo");
  if (btnGotoGonogo) {
    btnGotoGonogo.addEventListener("click", (e) => { e.preventDefault(); switchView("gonogo"); });
  }
  const btnGotoTimeline = document.getElementById("btn-goto-timeline");
  if (btnGotoTimeline) {
    btnGotoTimeline.addEventListener("click", (e) => { e.preventDefault(); switchView("timeline"); });
  }

  // Live log expand/collapse toggle
  const logExpandBtn = document.getElementById("log-expand-btn");
  if (logExpandBtn) {
    logExpandBtn.addEventListener("click", () => {
      const logCard = logExpandBtn.closest(".card");
      const expandIcon = document.getElementById("log-expand-icon");
      const expandText = document.getElementById("log-expand-text");

      if (logCard) {
        const isExpanded = logCard.classList.toggle("expanded");
        if (expandText) {
          expandText.textContent = isExpanded ? "Collapse" : "Expand";
        }
        if (expandIcon) {
          if (isExpanded) {
            // Collapse icon paths: arrows pointing inward
            expandIcon.innerHTML = `<path stroke-linecap="round" stroke-linejoin="round" d="M19 9h-4V5m0 4l5-5M5 15h4v4m0-4l-5 5m14 0h-4v4m0-4l5 5M5 9h4V5m0 4L4 4" />`;
          } else {
            // Expand icon paths: arrows pointing outward
            expandIcon.innerHTML = `<path stroke-linecap="round" stroke-linejoin="round" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5v-4m0 4h-4m4 0l-5-5" />`;
          }
        }
      }
    });
  }

  // Collapsible Terminal Card
  const terminalCard = document.querySelector(".terminal-header-card");
  if (terminalCard) {
    const isCollapsed = localStorage.getItem("gantry-terminal-collapsed") === "true";
    if (isCollapsed) {
      terminalCard.classList.add("collapsed");
    }
    const titlebar = terminalCard.querySelector(".terminal-titlebar");
    if (titlebar) {
      titlebar.addEventListener("click", () => {
        const collapsed = terminalCard.classList.toggle("collapsed");
        localStorage.setItem("gantry-terminal-collapsed", String(collapsed));
      });
    }
  }

  // Dashboard Tab Switching
  document.querySelectorAll(".dashboard-tab-btn").forEach(btn => {
    btn.addEventListener("click", () => {
      const tabName = btn.getAttribute("data-dashboard-tab");

      // Set active button
      document.querySelectorAll(".dashboard-tab-btn").forEach(b => {
        if (b === btn) b.classList.add("active");
        else b.classList.remove("active");
      });

      // Set active tab content
      document.querySelectorAll(".dashboard-tab-content").forEach(content => {
        if (content.id === `db-tab-${tabName}`) {
          content.classList.add("active");
        } else {
          content.classList.remove("active");
        }
      });
    });
  });

  // Vault watcher initialization
  await setupVaultWatcher();

  // Initialize Codebase Change Impact graph tabs
  await initGraphView();

  // Silent update check after startup settles
  setTimeout(() => { runUpdateCheck({ silent: true }); }, 3000);
});
