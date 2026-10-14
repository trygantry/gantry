import { state, setActiveView, setCachedIndex } from './state.js';
import { canLeaveEditor, openFileInEditor } from './renderers/editor.js';
import { createBlockerFile, createQAFile, getGitUser, getWorkspaceStatus, detectProjectType, initCustomizedVault } from './api.js';
import { showToast } from './checklist.js';
import { closeAccessibleModal, openAccessibleModal } from './modal.js';

export function switchView(viewName) {
  if (state.activeView === 'editor' && viewName !== 'editor') {
    if (!canLeaveEditor()) return;
  }
  setActiveView(viewName);
  document.body.dataset.productArea = viewName === 'mobile' ? 'release' : 'legacy';
  
  document.querySelectorAll(".nav-item").forEach(item => {
    if (item.getAttribute("data-view") === viewName) {
      item.classList.add("active");
    } else {
      item.classList.remove("active");
    }
  });

  document.querySelectorAll(".view-panel").forEach(panel => {
    if (panel.id === `view-${viewName}`) {
      panel.classList.add("active");
    } else {
      panel.classList.remove("active");
    }
  });

  const displayPageTitle = document.getElementById("page-title-display");
  const viewTitles = {
    "dashboard": "Launch Dashboard",
    "audit": "Launch Readiness Audit Cockpit",
    "health": "Documentation Health Analyzer",
    "blockers": "Critical Issues Board",
    "qa": "QA Readiness Checklists",
    "gonogo": "Go / No-Go Decision Room",
    "timeline": "Timeline — What's New by Date",
    "editor": "Release Vault Editor",
    "ci-setup": "CI/CD Pipeline Integration",
    "settings": "Gantry Settings",
    "mobile": "Mobile Release Governance"
  };
  if (displayPageTitle) {
    displayPageTitle.textContent = viewTitles[viewName] || "Launch Control";
  }

  // Trigger surgical re-render for the newly active view
  if (window.refreshUI) {
    window.refreshUI();
  }
}

export function toggleSpotlight(forceOpen) {
  const spotlightOverlay = document.getElementById("spotlight-overlay");
  const spotlightInput = document.getElementById("spotlight-input");
  if (!spotlightOverlay) return;

  const isOpen = spotlightOverlay.classList.contains("active");
  const shouldOpen = forceOpen !== undefined ? forceOpen : !isOpen;
  
  if (shouldOpen) {
    if (spotlightInput) spotlightInput.value = "";
    openAccessibleModal(spotlightOverlay, spotlightInput);
    renderSpotlightResults("");
  } else {
    closeAccessibleModal(spotlightOverlay);
  }
}

export function renderSpotlightResults(query) {
  const spotlightResults = document.getElementById("spotlight-results");
  if (!spotlightResults) return;
  spotlightResults.textContent = "";
  
  const guardedOpen = (path) => {
    window.openFileInEditor(path);
  };

  const searchTargets = [
    { title: "Go/No-Go Checklist", subtitle: "go_no_go.md", action: () => guardedOpen("go_no_go.md") },
    { title: "QA Regression Suite", subtitle: "qa/regression.md", action: () => guardedOpen("qa/regression.md") },
    { title: "Go/No-Go Decision Room", subtitle: "View", action: () => { switchView("gonogo"); toggleSpotlight(false); } },
    ...(state.cachedIndex.blockers || []).map(b => ({
      title: `${b.id}: ${b.title}`,
      subtitle: `Blocker | ${b.severity} | ${b.status}`,
      action: () => guardedOpen(b.path)
    }))
  ];

  const filtered = searchTargets.filter(t => 
    t.title.toLowerCase().includes(query.toLowerCase()) || 
    t.subtitle.toLowerCase().includes(query.toLowerCase())
  );

  if (filtered.length === 0) {
    const el = document.createElement("div");
    el.style.cssText = "padding:12px;text-align:center;color:var(--text-muted);font-size:13px;";
    el.textContent = "No items found.";
    spotlightResults.appendChild(el);
    return;
  }

  filtered.forEach(item => {
    const itemEl = document.createElement("div");
    itemEl.className = "spotlight-item";

    const titleSpan = document.createElement("span");
    titleSpan.className = "spotlight-item-title";
    titleSpan.textContent = item.title;
    const metaSpan = document.createElement("span");
    metaSpan.className = "spotlight-item-meta";
    metaSpan.textContent = item.subtitle;
    itemEl.appendChild(titleSpan);
    itemEl.appendChild(document.createElement("br"));
    itemEl.appendChild(metaSpan);

    itemEl.addEventListener("click", () => {
      item.action();
      toggleSpotlight(false);
    });
    spotlightResults.appendChild(itemEl);
  });
}

export async function setNoVaultState(noVault) {
  const overlay = document.getElementById("no-vault-overlay");
  const hint = document.getElementById("no-vault-hint");
  const indicator = document.getElementById("watch-dot-indicator");
  
  if (!noVault) {
    if (overlay) overlay.style.display = "none";
    if (hint) hint.style.display = "none";
    if (indicator && window.hasTauri && !state.demoMode) {
      indicator.style.display = "inline-block";
    }
    return;
  }
  
  if (overlay) overlay.style.display = "flex";
  if (hint) hint.style.display = "block";
  if (indicator) indicator.style.display = "none";
  
  const status = await getWorkspaceStatus();
  const noFolderPanel = document.getElementById("no-folder-panel");
  const setupWizardPanel = document.getElementById("setup-wizard-panel");
  
  if (status.state === "noWorkspace" || (status.state === "demoMode" && !state.demoMode)) {
    if (noFolderPanel) noFolderPanel.style.display = "flex";
    if (setupWizardPanel) setupWizardPanel.style.display = "none";
  } else if (status.state === "workspaceWithoutVault") {
    if (noFolderPanel) noFolderPanel.style.display = "none";
    if (setupWizardPanel) setupWizardPanel.style.display = "flex";
    
    // stack autodetection
    const detected = await detectProjectType();
    const selectEl = document.getElementById("wizard-template-select");
    if (selectEl && detected) {
      selectEl.value = detected;
      renderWizardGates(detected);
    }
  } else {
    // vaultLoaded
    if (overlay) overlay.style.display = "none";
    if (hint) hint.style.display = "none";
    if (indicator && window.hasTauri && !state.demoMode) {
      indicator.style.display = "inline-block";
    }
  }
}

function renderWizardGates(templateType) {
  const container = document.getElementById("wizard-gates-container");
  if (!container) return;
  
  container.innerHTML = "";
  
  const defaultGates = {
    "saas-app": ["Engineering", "QA", "Product"],
    "ai-app": ["Engineering", "QA", "Product"],
    "backend-api": ["Engineering", "QA", "Security"],
    "desktop-app": ["Engineering", "QA", "Product"],
    "chrome-extension": ["Engineering", "QA", "Store"],
    "mobile-app": ["Engineering", "QA", "Product"]
  };
  
  const gates = defaultGates[templateType] || defaultGates["saas-app"];
  
  gates.forEach(gate => {
    const div = document.createElement("div");
    div.style.cssText = "display:flex; align-items:center; gap:8px;";
    
    const input = document.createElement("input");
    input.type = "checkbox";
    input.id = `gate-checkbox-${gate}`;
    input.value = gate;
    input.checked = true;
    input.style.cssText = "width:14px; height:14px; accent-color:var(--color-brand);";
    
    const label = document.createElement("label");
    label.setAttribute("for", `gate-checkbox-${gate}`);
    label.style.cssText = "font-size:12px; color:var(--text-secondary); cursor:pointer;";
    label.textContent = `${gate} Gate checklist`;
    
    div.appendChild(input);
    div.appendChild(label);
    container.appendChild(div);
  });
}

export function initOnboardingWizard({ onChangeFolder, onTryDemo } = {}) {
  const selectEl = document.getElementById("wizard-template-select");
  if (selectEl) {
    selectEl.addEventListener("change", (e) => {
      renderWizardGates(e.target.value);
    });
  }
  
  const btnChangeFolderWizard = document.getElementById("btn-change-folder-wizard");
  if (btnChangeFolderWizard) {
    btnChangeFolderWizard.addEventListener("click", async () => {
      if (onChangeFolder) {
        await onChangeFolder();
      }
    });
  }

  const btnWizardTryDemo = document.getElementById("btn-wizard-try-demo");
  if (btnWizardTryDemo) {
    btnWizardTryDemo.addEventListener("click", async () => {
      if (onTryDemo) {
        await onTryDemo();
      }
    });
  }

  // Pre-fill target launch date input to today + 14 days at 09:00 AM
  const dateInput = document.getElementById("wizard-date-input");
  const timeInput = document.getElementById("wizard-time-input");
  if (dateInput) {
    const today = new Date();
    today.setDate(today.getDate() + 14);
    const yyyy = today.getFullYear();
    const mm = String(today.getMonth() + 1).padStart(2, '0');
    const dd = String(today.getDate()).padStart(2, '0');
    dateInput.value = `${yyyy}-${mm}-${dd}`;
  }
  if (timeInput) {
    timeInput.value = "09:00";
  }

  // Pre-fill lead/QA owner defaults from git user info if possible
  const leadInput = document.getElementById("wizard-lead-owner-input");
  const qaInput = document.getElementById("wizard-qa-lead-input");
  getGitUser().then(user => {
    if (user && user.name) {
      const username = `@${user.name.toLowerCase().replace(/\s+/g, "_")}`;
      if (leadInput) leadInput.value = username;
      if (qaInput) qaInput.value = `${username}_qa`;
    }
  }).catch(() => {});

  const form = document.getElementById("onboarding-setup-form");
  if (form) {
    form.addEventListener("submit", async (e) => {
      e.preventDefault();
      
      const template = document.getElementById("wizard-template-select").value;
      const version = document.getElementById("wizard-version-input").value.trim();
      const releaseDateOnly = document.getElementById("wizard-date-input").value;
      const releaseTimeOnly = document.getElementById("wizard-time-input").value || "00:00";
      const releaseDate = `${releaseDateOnly}T${releaseTimeOnly}`;
      const leadOwner = document.getElementById("wizard-lead-owner-input").value.trim();
      const qaLead = document.getElementById("wizard-qa-lead-input").value.trim();
      const createQA = document.getElementById("wizard-create-qa-checkbox").checked;
      
      const enabledGates = Array.from(document.querySelectorAll("#wizard-gates-container input:checked"))
        .map(input => input.value);
        
      try {
        await initCustomizedVault({
          template_type: template,
          version,
          release_date: releaseDate,
          lead_owner: leadOwner,
          qa_lead: qaLead,
          enabled_gates: enabledGates,
          create_qa: createQA,
          force: false
        });
        
        showToast("Launch control initialized successfully!", "success");
        
        // Re-bind watcher using the global setupVaultWatcher helper
        if (window.setupVaultWatcher) {
          await window.setupVaultWatcher();
        }
        
        if (window.refreshUI) await window.refreshUI();
      } catch (err) {
        showToast(`Initialization failed: ${err.message || err}`, "error");
      }
    });
  }
}

export function showAuditPreviewModal(auditPreview, onConfirm) {
  const modal = document.getElementById("audit-preview-modal");
  if (!modal) return;

  const docsList = document.getElementById("preview-docs-list");
  const tasksCreateList = document.getElementById("preview-tasks-create-list");
  const tasksResolveList = document.getElementById("preview-tasks-resolve-list");
  const affectedList = document.getElementById("preview-affected-list");

  if (docsList) docsList.innerHTML = "";
  if (tasksCreateList) tasksCreateList.innerHTML = "";
  if (tasksResolveList) tasksResolveList.innerHTML = "";
  if (affectedList) affectedList.innerHTML = "";

  const docs = auditPreview?.documents_to_create || [];
  const tasksCreate = auditPreview?.tasks_to_create || [];
  const tasksResolve = auditPreview?.tasks_to_auto_resolve || [];
  const affected = auditPreview?.affected_files || [];

  const fillSection = (sectionId, listEl, items, transform = (x) => x) => {
    const section = document.getElementById(sectionId);
    if (!section || !listEl) return;
    if (items.length > 0) {
      section.style.display = "block";
      items.forEach(item => {
        const li = document.createElement("li");
        li.className = "modal-preview-item";
        li.textContent = transform(item);
        listEl.appendChild(li);
      });
    } else {
      section.style.display = "none";
    }
  };

  fillSection("preview-docs-section", docsList, docs, d => d.startsWith(".gantry/") ? d : `.gantry/${d}`);
  fillSection("preview-tasks-create-section", tasksCreateList, tasksCreate);
  fillSection("preview-tasks-resolve-section", tasksResolveList, tasksResolve);
  fillSection("preview-affected-section", affectedList, affected);

  const btnCancel = document.getElementById("btn-preview-cancel");
  const btnConfirm = document.getElementById("btn-preview-confirm");

  if (btnCancel && btnConfirm) {
    const newCancel = btnCancel.cloneNode(true);
    const newConfirm = btnConfirm.cloneNode(true);
    btnCancel.parentNode.replaceChild(newCancel, btnCancel);
    btnConfirm.parentNode.replaceChild(newConfirm, btnConfirm);

    newCancel.addEventListener("click", () => closeAccessibleModal(modal));
    newConfirm.addEventListener("click", () => {
      closeAccessibleModal(modal);
      onConfirm();
    });
    openAccessibleModal(modal, newCancel);
  } else {
    openAccessibleModal(modal);
  }
}

export function showCommitDetailsModal(details) {
  const modal = document.getElementById("commit-details-modal");
  if (!modal) return;

  const mHash = document.getElementById("commit-modal-hash");
  const mAuthor = document.getElementById("commit-modal-author");
  const mDate = document.getElementById("commit-modal-date");
  const mSubject = document.getElementById("commit-modal-subject");
  const mBodyText = document.getElementById("commit-modal-body-text");
  const mStat = document.getElementById("commit-modal-stat");

  if (mHash) mHash.textContent = details.hash ? details.hash.substring(0, 7) : "";
  if (mAuthor) mAuthor.textContent = `Author: ${details.author || ""}`;
  if (mDate) mDate.textContent = `Date: ${(details.date || "").split(" ")[0]}`;
  if (mSubject) mSubject.textContent = details.message || "";
  if (mBodyText) mBodyText.textContent = details.body || "No description provided.";
  if (mStat) mStat.textContent = details.stat || "No stat details.";

  const btnClose = document.getElementById("btn-commit-modal-close");
  if (btnClose) {
    const newClose = btnClose.cloneNode(true);
    btnClose.parentNode.replaceChild(newClose, btnClose);
    newClose.addEventListener("click", () => closeAccessibleModal(modal));
    openAccessibleModal(modal, newClose);
  } else {
    openAccessibleModal(modal);
  }
}

function nextBlockerId() {
  const max = (state.cachedIndex.blockers || []).reduce((highest, blocker) => {
    const match = String(blocker.id || "").match(/^BLK-(\d+)$/i);
    return match ? Math.max(highest, parseInt(match[1], 10)) : highest;
  }, 100);
  return `BLK-${max + 1}`;
}

function qaIdFromTitle(title) {
  const slug = (title || "New QA Plan")
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `QA-${slug || "PLAN"}`;
}

function openModal(id) {
  openAccessibleModal(document.getElementById(id));
}

function closeModal(id) {
  closeAccessibleModal(document.getElementById(id));
}

export function initCreatorWizards() {
  const blockerForm = document.getElementById("create-blocker-form");
  const qaForm = document.getElementById("create-qa-form");

  const openBlockerWizard = async () => {
    const idInput = document.getElementById("blocker-id-input");
    const titleInput = document.getElementById("blocker-title-input");
    const ownerInput = document.getElementById("blocker-owner-input");
    const sourcesInput = document.getElementById("blocker-source-files-input");
    if (idInput) idInput.value = nextBlockerId();
    if (titleInput) titleInput.value = "";
    if (sourcesInput) sourcesInput.value = "";
    try {
      const user = await getGitUser();
      if (ownerInput) ownerInput.value = user.name ? `@${user.name.toLowerCase().replace(/\s+/g, "_")}` : "@owner";
    } catch {
      if (ownerInput) ownerInput.value = "@owner";
    }
    openModal("create-blocker-modal");
    setTimeout(() => titleInput?.focus(), 0);
  };

  const openQaWizard = async () => {
    const idInput = document.getElementById("qa-id-input");
    const titleInput = document.getElementById("qa-title-input");
    const ownerInput = document.getElementById("qa-owner-input");
    if (titleInput) titleInput.value = "";
    if (idInput) idInput.value = "QA-PLAN";
    try {
      const user = await getGitUser();
      if (ownerInput) ownerInput.value = user.name ? `@${user.name.toLowerCase().replace(/\s+/g, "_")}` : "@qa_owner";
    } catch {
      if (ownerInput) ownerInput.value = "@qa_owner";
    }
    openModal("create-qa-modal");
    setTimeout(() => titleInput?.focus(), 0);
  };

  document.getElementById("btn-new-blocker")?.addEventListener("click", openBlockerWizard);
  document.getElementById("btn-editor-new-blocker")?.addEventListener("click", openBlockerWizard);

  document.getElementById("btn-new-qa")?.addEventListener("click", openQaWizard);
  document.getElementById("btn-editor-new-qa")?.addEventListener("click", openQaWizard);

  document.getElementById("qa-title-input")?.addEventListener("input", (event) => {
    const idInput = document.getElementById("qa-id-input");
    if (idInput) idInput.value = qaIdFromTitle(event.target.value);
  });

  document.getElementById("btn-create-blocker-cancel")?.addEventListener("click", () => closeModal("create-blocker-modal"));
  document.getElementById("btn-create-qa-cancel")?.addEventListener("click", () => closeModal("create-qa-modal"));

  blockerForm?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const id = document.getElementById("blocker-id-input")?.value.trim();
    const title = document.getElementById("blocker-title-input")?.value.trim();
    const severity = document.getElementById("blocker-severity-input")?.value || "P1";
    const owner = document.getElementById("blocker-owner-input")?.value.trim();
    const sourceFiles = (document.getElementById("blocker-source-files-input")?.value || "")
      .split(",")
      .map(v => v.trim())
      .filter(Boolean);
    try {
      const path = await createBlockerFile(id, title, severity, owner, sourceFiles);
      closeModal("create-blocker-modal");
      showToast(`Blocker ${id} created successfully!`, "success");
      if (window.refreshUI) await window.refreshUI();
      await openFileInEditor(path);
    } catch (err) {
      showToast(`Could not create blocker: ${err.message || err}`, "error");
    }
  });

  qaForm?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const id = document.getElementById("qa-id-input")?.value.trim();
    const title = document.getElementById("qa-title-input")?.value.trim();
    const owner = document.getElementById("qa-owner-input")?.value.trim();
    const platforms = Array.from(document.querySelectorAll("input[name='qa-platform']:checked")).map(input => input.value);
    try {
      const path = await createQAFile(id, title, owner, platforms);
      closeModal("create-qa-modal");
      showToast(`QA Plan ${id} created successfully!`, "success");
      if (window.refreshUI) await window.refreshUI();
      await openFileInEditor(path);
    } catch (err) {
      showToast(`Could not create QA plan: ${err.message || err}`, "error");
    }
  });
}
