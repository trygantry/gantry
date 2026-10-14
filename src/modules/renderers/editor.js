import { state, setActiveFilePath, setEditorLoaded, setEditorDirty, setEditorSaveStatus, setEditorPreviewMode, setOpenEditorTabs } from '../state.js';
import { loadFileContent, saveFileContent, openInIde, listVaultFiles, hasTauri, getWorkspacePath } from '../api.js';
import { mockVaultFiles } from '../constants.js';
import { closeAccessibleModal, openAccessibleModal } from '../modal.js';

// ── Debounce helpers ──────────────────────────────────────────────────────────
let statsTimer = null;
let actionTimer = null;
let inputListenerAttached = false;

function debounce(fn, ms) {
  let t;
  return (...args) => { clearTimeout(t); t = setTimeout(() => fn(...args), ms); };
}

// ── Workspace tabs persistence helpers ──────────────────────────────────────────
async function getTabsStorageKeys() {
  const ws = await getWorkspacePath();
  const wsKey = ws || "default";
  return {
    tabsKey: `gantry-tabs-${wsKey}`,
    activeTabKey: `gantry-active-tab-${wsKey}`
  };
}

async function persistOpenTabs() {
  const keys = await getTabsStorageKeys();
  const paths = state.openEditorTabs.map(t => t.path);
  localStorage.setItem(keys.tabsKey, JSON.stringify(paths));
}

async function persistActiveTab() {
  const keys = await getTabsStorageKeys();
  localStorage.setItem(keys.activeTabKey, state.activeFilePath);
}

// ── Public API ────────────────────────────────────────────────────────────────

export async function initEditorTabs() {
  const keys = await getTabsStorageKeys();
  const savedTabsJson = localStorage.getItem(keys.tabsKey);
  const savedActiveTab = localStorage.getItem(keys.activeTabKey);
  
  if (savedTabsJson) {
    try {
      const paths = JSON.parse(savedTabsJson);
      const existingFiles = await listVaultFiles();
      const validPaths = paths.filter(p => existingFiles.includes(p));
      
      const loadedTabs = [];
      for (const path of validPaths) {
        const content = await loadFileContent(path);
        loadedTabs.push({
          path,
          content,
          savedContent: content,
          dirty: false
        });
      }
      setOpenEditorTabs(loadedTabs);
      
      if (validPaths.includes(savedActiveTab)) {
        setActiveFilePath(savedActiveTab);
        const activeTab = loadedTabs.find(t => t.path === savedActiveTab);
        setEditorLoaded(activeTab.savedContent);
      } else if (validPaths.length > 0) {
        setActiveFilePath(validPaths[0]);
        setEditorLoaded(loadedTabs[0].savedContent);
      } else {
        setActiveFilePath("");
        setEditorLoaded("");
      }
    } catch (e) {
      console.error("Failed to restore open tabs:", e);
      setOpenEditorTabs([]);
      setActiveFilePath("");
      setEditorLoaded("");
    }
  } else {
    setOpenEditorTabs([]);
    setActiveFilePath("");
    setEditorLoaded("");
  }
}

export async function openFileInEditor(filePath) {
  window.switchView("editor");
  restoreEditorViewFromEmptyState();
  
  let tab = state.openEditorTabs.find(t => t.path === filePath);
  if (!tab) {
    const content = await loadFileContent(filePath);
    tab = {
      path: filePath,
      content,
      savedContent: content,
      dirty: false
    };
    const newTabs = [...state.openEditorTabs, tab];
    setOpenEditorTabs(newTabs);
    await persistOpenTabs();
  }
  
  await activateEditorTab(filePath);
}

export async function activateEditorTab(filePath) {
  setActiveFilePath(filePath);
  await persistActiveTab();
  
  const tab = state.openEditorTabs.find(t => t.path === filePath);
  if (!tab) return;
  
  const textarea = document.getElementById("editor-field");
  if (textarea) textarea.value = tab.content;
  
  const pathDisplay = document.getElementById("active-edit-path");
  if (pathDisplay) pathDisplay.textContent = filePath;
  
  setEditorLoaded(tab.savedContent);
  setEditorDirty(tab.dirty);
  updateDirtyIndicator(tab.dirty);
  
  updatePreviewModeButtons(filePath);
  renderPreview();
  scheduleStats();
  scheduleActionItems();
  await renderFileTree();
  renderEditorTabs();
}

export async function saveActiveEditor() {
  const textarea = document.getElementById("editor-field");
  if (!textarea) return;
  
  const filePath = state.activeFilePath;
  if (!filePath) return;
  
  const tab = state.openEditorTabs.find(t => t.path === filePath);
  if (!tab) return;
  
  const content = textarea.value;
  setEditorSaveStatus("saving");
  updateSaveStatus("saving");
  try {
    await saveFileContent(filePath, content);
    
    tab.savedContent = content;
    tab.content = content;
    tab.dirty = false;
    
    setEditorLoaded(content);
    setEditorDirty(false);
    updateDirtyIndicator(false);
    setEditorSaveStatus("saved");
    updateSaveStatus("saved");
    
    renderEditorTabs();
    await renderFileTree();
    
    const editorDirtyBadge = document.getElementById("sidebar-editor-dirty-badge");
    if (editorDirtyBadge) {
      editorDirtyBadge.style.display = hasDirtyEditorTabs() ? "inline-flex" : "none";
    }
    
    setTimeout(() => {
      setEditorSaveStatus("idle");
      updateSaveStatus("idle");
    }, 1800);
  } catch (err) {
    setEditorSaveStatus("error");
    updateSaveStatus("error");
    alert(`Save failed: ${err.message || err}`);
  }
}

export function closeEditorTab(filePath) {
  const tabIndex = state.openEditorTabs.findIndex(t => t.path === filePath);
  if (tabIndex === -1) return;
  
  const tab = state.openEditorTabs[tabIndex];
  if (tab.dirty) {
    confirmCloseEditorTab(filePath).then(async (choice) => {
      if (choice === "save") {
        try {
          await saveFileContent(tab.path, tab.content);
          tab.savedContent = tab.content;
          tab.dirty = false;
        } catch (err) {
          alert(`Failed to save before closing: ${err.message || err}`);
          return; // Don't close
        }
      } else if (choice === "cancel") {
        return; // Don't close
      }
      
      await removeEditorTab(filePath, tabIndex);
    });
  } else {
    removeEditorTab(filePath, tabIndex);
  }
}

async function removeEditorTab(filePath, tabIndex) {
  const newTabs = state.openEditorTabs.filter(t => t.path !== filePath);
  setOpenEditorTabs(newTabs);
  await persistOpenTabs();
  
  if (state.activeFilePath === filePath) {
    if (newTabs.length > 0) {
      const nextActiveIndex = Math.min(tabIndex, newTabs.length - 1);
      await activateEditorTab(newTabs[nextActiveIndex].path);
    } else {
      setActiveFilePath("");
      setEditorLoaded("");
      setEditorDirty(false);
      const textarea = document.getElementById("editor-field");
      if (textarea) textarea.value = "";
      updateEditorViewForEmptyState();
    }
  } else {
    renderEditorTabs();
  }
  
  const editorDirtyBadge = document.getElementById("sidebar-editor-dirty-badge");
  if (editorDirtyBadge) {
    editorDirtyBadge.style.display = hasDirtyEditorTabs() ? "inline-flex" : "none";
  }
}

export function confirmCloseEditorTab(filePath) {
  return new Promise((resolve) => {
    const modal = document.getElementById("close-tab-modal");
    const desc = document.getElementById("close-tab-modal-desc");
    const btnSave = document.getElementById("btn-close-tab-save");
    const btnDiscard = document.getElementById("btn-close-tab-discard");
    const btnCancel = document.getElementById("btn-close-tab-cancel");
    
    if (!modal || !desc || !btnSave || !btnDiscard || !btnCancel) {
      const result = confirm(`Save changes to ${filePath} before closing?\nOK = Save, Cancel = Discard/Cancel`);
      resolve(result ? "save" : "discard");
      return;
    }
    
    desc.textContent = `Document "${filePath}" has unsaved changes. Do you want to save them before closing?`;
    const cleanup = (val) => {
      closeAccessibleModal(modal);
      resolve(val);
    };
    
    const newSave = btnSave.cloneNode(true);
    const newDiscard = btnDiscard.cloneNode(true);
    const newCancel = btnCancel.cloneNode(true);
    
    btnSave.parentNode.replaceChild(newSave, btnSave);
    btnDiscard.parentNode.replaceChild(newDiscard, btnDiscard);
    btnCancel.parentNode.replaceChild(newCancel, btnCancel);
    
    newSave.addEventListener("click", () => cleanup("save"));
    newDiscard.addEventListener("click", () => cleanup("discard"));
    newCancel.addEventListener("click", () => cleanup("cancel"));
    openAccessibleModal(modal, newCancel, () => cleanup("cancel"));
  });
}

export function canCloseEditorTab(filePath) {
  const tab = state.openEditorTabs.find(t => t.path === filePath);
  return tab ? !tab.dirty : true;
}

export function hasDirtyEditorTabs() {
  return state.openEditorTabs.some(t => t.dirty);
}

export function canLeaveEditor() {
  if (!hasDirtyEditorTabs()) return true;
  return confirm("You have unsaved changes in one or more tabs. Leave without saving?");
}

export function setPreviewMode(mode) {
  setEditorPreviewMode(mode);
  const body = document.querySelector(".editor-body");
  if (body) {
    body.classList.remove("mode-edit", "mode-split", "mode-preview");
    body.classList.add(`mode-${mode}`);
  }
  document.querySelectorAll(".btn-mode").forEach(btn => {
    btn.classList.toggle("active", btn.dataset.mode === mode);
  });
  const preview = document.getElementById("editor-preview");
  if (preview) {
    if (mode === "edit") {
      preview.style.display = "none";
    } else {
      preview.style.display = "";
      renderPreview();
    }
  }
  localStorage.setItem("editor-preview-mode", mode);
}

export function renderEditor() {
  if (state.openEditorTabs.length === 0) {
    updateEditorViewForEmptyState();
    return;
  }
  
  restoreEditorViewFromEmptyState();
  
  const pathDisplay = document.getElementById("active-edit-path");
  if (pathDisplay) pathDisplay.textContent = state.activeFilePath;
  
  const tab = state.openEditorTabs.find(t => t.path === state.activeFilePath);
  if (tab) {
    const textarea = document.getElementById("editor-field");
    if (textarea && textarea.value !== tab.content) {
      textarea.value = tab.content;
    }
    setEditorLoaded(tab.savedContent);
    setEditorDirty(tab.dirty);
    updateDirtyIndicator(tab.dirty);
  }
  
  updatePreviewModeButtons(state.activeFilePath);
  scheduleStats();
  scheduleActionItems();
  renderFileTree();
  renderEditorTabs();
}

function updateDirtyIndicator(dirty) {
  const pathEl = document.getElementById("active-edit-path");
  if (!pathEl) return;
  pathEl.classList.toggle("is-dirty", dirty);
}

function updateSaveStatus(status) {
  const btn = document.getElementById("btn-save-file");
  if (!btn) return;
  if (status === "saving") {
    btn.textContent = "Saving…";
    btn.disabled = true;
  } else if (status === "saved") {
    btn.textContent = "✓ Saved";
    btn.disabled = false;
  } else if (status === "error") {
    btn.textContent = "Save Failed";
    btn.disabled = false;
  } else {
    btn.textContent = "💾 Save";
    btn.disabled = false;
  }
}

export function syncActiveTabFromTextarea() {
  const textarea = document.getElementById("editor-field");
  if (!textarea) return;
  const activeTab = state.openEditorTabs.find(t => t.path === state.activeFilePath);
  if (activeTab) {
    activeTab.content = textarea.value;
    activeTab.dirty = activeTab.content !== activeTab.savedContent;
    setEditorDirty(activeTab.dirty);
    updateDirtyIndicator(activeTab.dirty);
    renderEditorTabs();
    
    const editorDirtyBadge = document.getElementById("sidebar-editor-dirty-badge");
    if (editorDirtyBadge) {
      editorDirtyBadge.style.display = hasDirtyEditorTabs() ? "inline-flex" : "none";
    }
  }
}

export function attachInputListener() {
  if (inputListenerAttached) return;
  inputListenerAttached = true;
  const textarea = document.getElementById("editor-field");
  if (!textarea) return;
  textarea.addEventListener("input", () => {
    syncActiveTabFromTextarea();
    scheduleStats();
    scheduleActionItems();
    if (state.editorPreviewMode !== "edit") renderPreview();
  });
}

// ── File Tree ─────────────────────────────────────────────────────────────────

export async function renderFileTree() {
  const treeEl = document.getElementById("file-tree");
  if (!treeEl) return;

  const files = await listVaultFiles();
  treeEl.textContent = "";

  // Group: top-level files first, then subdirectory groups
  const topLevel = files.filter(f => !f.includes("/"));
  const byDir = {};
  files.filter(f => f.includes("/")).forEach(f => {
    const dir = f.substring(0, f.lastIndexOf("/"));
    (byDir[dir] = byDir[dir] || []).push(f);
  });

  const makeItem = (filePath) => {
    const btn = document.createElement("button");
    btn.className = "tree-item";
    if (filePath === state.activeFilePath) btn.classList.add("active");
    
    const filename = filePath.includes("/") ? filePath.substring(filePath.lastIndexOf("/") + 1) : filePath;
    const isMarkdown = filename.endsWith(".md");
    const isJson = filename.endsWith(".json");
    const emoji = isMarkdown ? "📄 " : (isJson ? "⚙️ " : "📝 ");
    
    btn.textContent = emoji + filename;
    btn.title = filePath;
    btn.addEventListener("click", async () => {
      if (filePath === state.activeFilePath) return;
      await openFileInEditor(filePath);
    });
    return btn;
  };

  topLevel.sort().forEach(f => treeEl.appendChild(makeItem(f)));

  Object.keys(byDir).sort().forEach(dir => {
    const details = document.createElement("details");
    details.open = true;
    
    const summary = document.createElement("summary");
    summary.className = "tree-dir";
    
    // Add category-specific css classes
    const dirLower = dir.toLowerCase();
    if (dirLower === "blockers") {
      summary.classList.add("dir-blockers");
    } else if (dirLower === "qa") {
      summary.classList.add("dir-qa");
    }
    
    summary.textContent = "📂 " + dir; // Default to open
    details.appendChild(summary);
    
    // Dynamic open/closed folder toggle
    details.addEventListener("toggle", () => {
      summary.textContent = (details.open ? "📂 " : "📁 ") + dir;
    });

    byDir[dir].sort().forEach(f => details.appendChild(makeItem(f)));
    treeEl.appendChild(details);
  });
}

// ── Sidebar toggle ────────────────────────────────────────────────────────────

export function initSidebarToggle() {
  const btn = document.getElementById("btn-sidebar-toggle");
  const body = document.querySelector(".editor-body");
  if (!btn || !body) return;

  const collapsed = localStorage.getItem("editor-sidebar-collapsed") === "true";
  if (collapsed) {
    body.classList.add("sidebar-collapsed");
    btn.textContent = "›";
  }

  btn.addEventListener("click", () => {
    const isCollapsed = body.classList.toggle("sidebar-collapsed");
    btn.textContent = isCollapsed ? "›" : "‹";
    localStorage.setItem("editor-sidebar-collapsed", isCollapsed);
  });
}

// ── Preview mode ──────────────────────────────────────────────────────────────

function updatePreviewModeButtons(filePath) {
  const isMarkdown = filePath && filePath.endsWith(".md");
  document.querySelectorAll(".btn-mode").forEach(btn => {
    if (btn.dataset.mode === "edit") {
      btn.disabled = false;
      btn.title = "";
    } else {
      btn.disabled = !isMarkdown;
      btn.title = isMarkdown ? "" : "Preview available for .md files only";
    }
  });

  const body = document.querySelector(".editor-body");
  const preview = document.getElementById("editor-preview");

  if (!isMarkdown) {
    // Visually force edit mode layout (single full-width pane) for non-markdown files
    // without permanently overwriting the user's preferred layout in state/localStorage
    if (body) {
      body.classList.remove("mode-split", "mode-preview");
      body.classList.add("mode-edit");
    }
    document.querySelectorAll(".btn-mode").forEach(btn => {
      btn.classList.toggle("active", btn.dataset.mode === "edit");
    });
    if (preview) {
      preview.style.display = "none";
    }
  } else {
    // Restore the user's preferred view layout when switching back to a markdown file
    const preferredMode = state.editorPreviewMode || "edit";
    if (body) {
      body.classList.remove("mode-edit", "mode-split", "mode-preview");
      body.classList.add(`mode-${preferredMode}`);
    }
    document.querySelectorAll(".btn-mode").forEach(btn => {
      btn.classList.toggle("active", btn.dataset.mode === preferredMode);
    });
    if (preview) {
      if (preferredMode === "edit") {
        preview.style.display = "none";
      } else {
        preview.style.display = "";
        renderPreview();
      }
    }
  }
}

function renderPreview() {
  const preview = document.getElementById("editor-preview");
  const textarea = document.getElementById("editor-field");
  if (!preview || !textarea) return;
  if (state.editorPreviewMode === "edit") return;
  if (!state.activeFilePath?.endsWith(".md")) return;

  const md = textarea.value;
  if (window.marked && window.DOMPurify) {
    preview.innerHTML = window.DOMPurify.sanitize(window.marked.parse(md));
  } else {
    preview.textContent = md;
  }

  // Remove the 'disabled' attribute GFM markdown parsers append by default
  const checkboxes = preview.querySelectorAll('input[type="checkbox"]');
  checkboxes.forEach(cb => cb.removeAttribute("disabled"));
}

function toggleTextareaCheckbox(checkboxIndex, isChecked) {
  const textarea = document.getElementById("editor-field");
  if (!textarea) return;

  const lines = textarea.value.split("\n");
  let checklistCount = 0;

  for (let i = 0; i < lines.length; i++) {
    // Matches bullet checklists: - [ ], - [x], or - [/]
    const match = lines[i].match(/^(\s*-\s*\[)([ x\/])(\].*)$/i);
    if (match) {
      if (checklistCount === checkboxIndex) {
        // Toggle character state
        const newChar = isChecked ? "x" : " ";
        lines[i] = match[1] + newChar + match[3];
        break;
      }
      checklistCount++;
    }
  }

  textarea.value = lines.join("\n");
  
  // Sync tab working set, update indicators, stats, extraction items
  syncActiveTabFromTextarea();
  scheduleStats();
  scheduleActionItems();
  
  // Re-render preview panel immediately so everything stays completely in-sync
  renderPreview();
}

export function attachPreviewCheckboxListener() {
  const preview = document.getElementById("editor-preview");
  if (!preview) return;

  // Single delegated click listener to handle all preview checkbox clicks
  preview.addEventListener("click", (e) => {
    if (e.target && e.target.type === "checkbox") {
      const checkbox = e.target;
      const allCheckboxes = Array.from(preview.querySelectorAll('input[type="checkbox"]'));
      const index = allCheckboxes.indexOf(checkbox);
      if (index === -1) return;

      toggleTextareaCheckbox(index, checkbox.checked);
    }
  });
}

export function initPreviewModeButtons() {
  const saved = localStorage.getItem("editor-preview-mode") || "edit";
  document.querySelectorAll(".btn-mode").forEach(btn => {
    btn.classList.toggle("active", btn.dataset.mode === saved);
    btn.addEventListener("click", () => setPreviewMode(btn.dataset.mode));
  });
  // Apply saved mode (without re-rendering; file not loaded yet)
  setEditorPreviewMode(saved);
  const body = document.querySelector(".editor-body");
  if (body) {
    body.classList.remove("mode-edit", "mode-split", "mode-preview");
    body.classList.add(`mode-${saved}`);
  }
}

// ── Stats / Completion Meter ──────────────────────────────────────────────────

const scheduleStats = debounce(_updateStats, 150);

function stripFrontmatter(text) {
  if (!text.startsWith("---")) return text;
  const end = text.indexOf("\n---", 3);
  return end === -1 ? text : text.slice(end + 4).trimStart();
}

function _updateStats() {
  const textarea = document.getElementById("editor-field");
  const bar = document.getElementById("editor-statusbar");
  if (!textarea || !bar) return;

  const raw = textarea.value;
  const body = stripFrontmatter(raw)
    .replace(/<!--[\s\S]*?-->/g, "")           // strip HTML comments
    .replace(/^```[\s\S]*?^```/gm, "");        // strip fenced code blocks

  const words = body.trim().split(/\s+/).filter(Boolean).length;

  // Checklist progress
  const done    = (raw.match(/^- \[x\]/gim) || []).length;
  const partial = (raw.match(/^- \[\/\]/gim) || []).length;
  const open    = (raw.match(/^- \[ \]/gim) || []).length;
  const total   = done + partial + open;

  // Section fill: prefer ## headings, fallback to #
  const h2Matches = [...body.matchAll(/^## .+$/gm)];
  const headingRe = h2Matches.length > 0 ? /^## .+$/m : /^# .+$/m;
  const sections = body.split(headingRe).slice(1); // content after each heading
  const filledSections = sections.filter(s => /\S/.test(s.replace(/^#+ .+$/gm, "").replace(/<!--[\s\S]*?-->/g, "")));

  const parts = [`${words} words`];
  if (sections.length > 0) parts.push(`${filledSections.length}/${sections.length} sections`);
  if (total > 0) parts.push(`${done} done, ${partial} partial, ${open} open`);

  bar.textContent = parts.join(" · ");
}

// ── Action Items ──────────────────────────────────────────────────────────────

const scheduleActionItems = debounce(_updateActionItems, 300);

function stableId(text) {
  // Simple stable hash for task identity
  let h = 0;
  for (let i = 0; i < text.length; i++) h = (Math.imul(31, h) + text.charCodeAt(i)) | 0;
  return Math.abs(h).toString(36);
}

function extractActionItems(text) {
  const items = [];
  const lines = text.split("\n");
  lines.forEach((line, idx) => {
    // Unchecked checklist — skip already-tracked lines
    const checkMatch = line.match(/^- \[ \] (.+)/);
    if (checkMatch && !line.includes("<!-- gantry:tracked -->")) {
      const rawText = checkMatch[1];
      const displayText = rawText.replace(/<!--.*?-->/g, "").trim();
      items.push({ line: idx + 1, text: rawText, displayText, type: "checklist" });
    }
    // TODO comments (skip lines that are HTML comments themselves)
    const todoMatch = !line.trimStart().startsWith("<!--") && line.match(/TODO:\s*(.+)/i);
    if (todoMatch) {
      const rawText = todoMatch[1].replace(/-->/g, "").trim();
      items.push({ line: idx + 1, text: rawText, displayText: rawText, type: "todo" });
    }
  });
  return items;
}

async function isAlreadyTracked(id) {
  try {
    const tasksContent = await loadFileContent("tasks.md");
    return tasksContent.includes(`gantry:id:editor-${id}`);
  } catch {
    return false;
  }
}

async function promoteToTask(item, btn) {
  const id = stableId(item.text);
  if (await isAlreadyTracked(id)) {
    btn.textContent = "Already tracked";
    btn.disabled = true;
    return;
  }

  const today = new Date().toISOString().split("T")[0];
  const taskEntry = `\n<!-- gantry:id:editor-${id} source:editor severity:P2 evidence:${state.activeFilePath}:L${item.line} created_at:${today} -->\n- [ ] ${item.text}\n`;

  // Append to tasks.md
  const currentTasks = await loadFileContent("tasks.md");
  await saveFileContent("tasks.md", currentTasks + taskEntry);

  // Mark source line with <!-- gantry:tracked --> (do NOT change [ ] to [x])
  const textarea = document.getElementById("editor-field");
  if (textarea && item.type === "checklist") {
    const lines = textarea.value.split("\n");
    const lineIdx = item.line - 1;
    if (lines[lineIdx] && !lines[lineIdx].includes("<!-- gantry:tracked -->")) {
      lines[lineIdx] = lines[lineIdx].replace(/(- \[ \] .+?)(\s*(?:<!--.*?-->)?\s*)$/, "$1 <!-- gantry:tracked -->");
      textarea.value = lines.join("\n");
      
      const activeTab = state.openEditorTabs.find(t => t.path === state.activeFilePath);
      if (activeTab) {
        activeTab.content = textarea.value;
        activeTab.dirty = activeTab.content !== activeTab.savedContent;
        setEditorDirty(activeTab.dirty);
        updateDirtyIndicator(activeTab.dirty);
        renderEditorTabs();
      }
      
      const editorDirtyBadge = document.getElementById("sidebar-editor-dirty-badge");
      if (editorDirtyBadge) {
        editorDirtyBadge.style.display = hasDirtyEditorTabs() ? "inline-flex" : "none";
      }
      
      scheduleStats();
    }
  }

  btn.textContent = "Tracked ✓";
  btn.disabled = true;
  _updateActionItems();
}

async function _updateActionItems() {
  const panel = document.getElementById("editor-task-extract");
  if (!panel) return;
  const textarea = document.getElementById("editor-field");
  if (!textarea) return;

  const items = extractActionItems(textarea.value);

  if (items.length === 0) {
    panel.innerHTML = `<p class="action-items-empty">No open action items found.</p>`;
    return;
  }

  panel.textContent = "";
  for (const item of items) {
    const row = document.createElement("div");
    row.className = "action-item-row";

    const label = document.createElement("button");
    label.className = "action-item-text";
    label.title = `Line ${item.line}`;
    label.textContent = item.displayText;
    label.addEventListener("click", () => scrollToLine(item.line));

    const promote = document.createElement("button");
    promote.className = "btn btn-promote";
    promote.textContent = "→ Track";

    // Check already-tracked asynchronously
    const id = stableId(item.text);
    isAlreadyTracked(id).then(tracked => {
      if (tracked) {
        promote.textContent = "Tracked ✓";
        promote.disabled = true;
      }
    });

    promote.addEventListener("click", () => promoteToTask(item, promote));

    row.appendChild(label);
    row.appendChild(promote);
    panel.appendChild(row);
  }
}

function scrollToLine(lineNumber) {
  const textarea = document.getElementById("editor-field");
  if (!textarea) return;
  const lines = textarea.value.split("\n");
  let pos = 0;
  for (let i = 0; i < lineNumber - 1 && i < lines.length; i++) {
    pos += lines[i].length + 1;
  }
  textarea.focus();
  textarea.setSelectionRange(pos, pos + (lines[lineNumber - 1] || "").length);
  // Scroll textarea to show the line
  const lineHeight = parseFloat(getComputedStyle(textarea).lineHeight) || 22;
  textarea.scrollTop = (lineNumber - 3) * lineHeight;
}

export function renderEditorTabs() {
  const tabsStrip = document.getElementById("editor-tabs-strip");
  if (!tabsStrip) return;
  
  tabsStrip.textContent = "";
  
  state.openEditorTabs.forEach(tab => {
    const tabEl = document.createElement("div");
    tabEl.className = `editor-tab${tab.path === state.activeFilePath ? " active" : ""}`;
    
    const filename = tab.path.includes("/") ? tab.path.substring(tab.path.lastIndexOf("/") + 1) : tab.path;
    
    const labelSpan = document.createElement("span");
    labelSpan.className = "tab-label";
    labelSpan.textContent = filename;
    labelSpan.title = tab.path;
    tabEl.appendChild(labelSpan);
    
    if (tab.dirty) {
      const dirtySpan = document.createElement("span");
      dirtySpan.className = "tab-dirty-indicator";
      dirtySpan.textContent = "*";
      tabEl.appendChild(dirtySpan);
    }
    
    const closeBtn = document.createElement("button");
    closeBtn.className = "tab-close-btn";
    closeBtn.innerHTML = "&times;";
    closeBtn.title = "Close tab";
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      closeEditorTab(tab.path);
    });
    tabEl.appendChild(closeBtn);
    
    tabEl.addEventListener("click", () => {
      activateEditorTab(tab.path);
    });
    
    tabsStrip.appendChild(tabEl);
  });
}

function updateEditorViewForEmptyState() {
  const textarea = document.getElementById("editor-field");
  const preview = document.getElementById("editor-preview");
  const emptyState = document.getElementById("editor-empty-state");
  const pathDisplay = document.getElementById("active-edit-path");
  const btnSave = document.getElementById("btn-save-file");
  const btnOpenIde = document.getElementById("btn-open-ide");
  const modeBtns = document.querySelectorAll(".btn-mode");
  
  if (textarea) textarea.style.display = "none";
  if (preview) preview.style.display = "none";
  if (emptyState) emptyState.style.display = "flex";

  // Revert body to clean edit layout so the empty state spans the full single-pane width
  const body = document.querySelector(".editor-body");
  if (body) {
    body.classList.remove("mode-split", "mode-preview");
    body.classList.add("mode-edit");
  }

  if (pathDisplay) {
    pathDisplay.textContent = "No file open";
    pathDisplay.classList.remove("is-dirty");
  }
  if (btnSave) btnSave.disabled = true;
  if (btnOpenIde) btnOpenIde.disabled = true;
  modeBtns.forEach(btn => {
    btn.disabled = true;
    btn.classList.remove("active");
  });
  
  const statusbar = document.getElementById("editor-statusbar");
  if (statusbar) statusbar.textContent = "Select a file to start editing";
  
  const taskExtract = document.getElementById("editor-task-extract");
  if (taskExtract) {
    taskExtract.innerHTML = `<p class="action-items-empty">No open file.</p>`;
  }
  
  renderEditorTabs();
  renderFileTree();
}

function restoreEditorViewFromEmptyState() {
  const textarea = document.getElementById("editor-field");
  const emptyState = document.getElementById("editor-empty-state");
  const btnSave = document.getElementById("btn-save-file");
  const btnOpenIde = document.getElementById("btn-open-ide");
  
  if (textarea) textarea.style.display = "";
  if (emptyState) emptyState.style.display = "none";
  if (btnSave) btnSave.disabled = false;
  if (btnOpenIde) btnOpenIde.disabled = false;
}

export function attachSmartListListener() {
  const textarea = document.getElementById("editor-field");
  if (!textarea) return;

  textarea.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      const value = textarea.value;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;

      const beforeCursor = value.substring(0, start);
      const afterCursor = value.substring(end);
      const lineStart = beforeCursor.lastIndexOf("\n") + 1;
      const currentLine = beforeCursor.substring(lineStart);

      // 1. Checklist item with text: - [ ] Some text
      const checklistTextMatch = currentLine.match(/^(\s*-\s*\[[ x\/]\]\s+)(.+)$/i);
      if (checklistTextMatch) {
        e.preventDefault();
        const prefix = currentLine.match(/^(\s*-\s*\[[ x\/]\]\s*)/i)[1];
        // Reset checklist check in new item
        const newPrefix = prefix.replace(/\[[x\/]\]/i, "[ ]");
        
        textarea.value = beforeCursor + "\n" + newPrefix + afterCursor;
        const newCursorPos = start + 1 + newPrefix.length;
        textarea.setSelectionRange(newCursorPos, newCursorPos);
        syncActiveTabFromTextarea();
        if (state.editorPreviewMode !== "edit") renderPreview();
        return;
      }

      // 2. Empty checklist item: - [ ]
      const checklistEmptyMatch = currentLine.match(/^(\s*-\s*\[[ x\/]\]\s*)$/i);
      if (checklistEmptyMatch) {
        e.preventDefault();
        // Clear checklist prefix to cleanly exit the list
        const clearedLine = beforeCursor.substring(0, lineStart);
        textarea.value = clearedLine + "\n" + afterCursor;
        const newCursorPos = lineStart + 1;
        textarea.setSelectionRange(newCursorPos, newCursorPos);
        syncActiveTabFromTextarea();
        if (state.editorPreviewMode !== "edit") renderPreview();
        return;
      }

      // 3. Simple bullet item with text: - Some text
      const bulletTextMatch = currentLine.match(/^(\s*-\s+)(.+)$/);
      if (bulletTextMatch) {
        e.preventDefault();
        const prefix = bulletTextMatch[1];
        textarea.value = beforeCursor + "\n" + prefix + afterCursor;
        const newCursorPos = start + 1 + prefix.length;
        textarea.setSelectionRange(newCursorPos, newCursorPos);
        syncActiveTabFromTextarea();
        if (state.editorPreviewMode !== "edit") renderPreview();
        return;
      }

      // 4. Empty bullet item: - 
      const bulletEmptyMatch = currentLine.match(/^(\s*-\s*)$/);
      if (bulletEmptyMatch) {
        e.preventDefault();
        const clearedLine = beforeCursor.substring(0, lineStart);
        textarea.value = clearedLine + "\n" + afterCursor;
        const newCursorPos = lineStart + 1;
        textarea.setSelectionRange(newCursorPos, newCursorPos);
        syncActiveTabFromTextarea();
        if (state.editorPreviewMode !== "edit") renderPreview();
        return;
      }
    }
  });
}

export function attachMarkdownShortcutListener() {
  const textarea = document.getElementById("editor-field");
  if (!textarea) return;

  textarea.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && (e.key === "b" || e.key === "i")) {
      e.preventDefault();
      const value = textarea.value;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const selectedText = value.substring(start, end);
      
      let wrapper = "";
      if (e.key === "b") {
        wrapper = "**";
      } else if (e.key === "i") {
        wrapper = "*";
      }
      
      const newText = wrapper + selectedText + wrapper;
      textarea.value = value.substring(0, start) + newText + value.substring(end);
      
      // Keep selection or place cursor inside wrapper
      let newStart, newEnd;
      if (selectedText.length === 0) {
        newStart = start + wrapper.length;
        newEnd = newStart;
      } else {
        newStart = start + wrapper.length;
        newEnd = end + wrapper.length;
      }
      textarea.setSelectionRange(newStart, newEnd);
      textarea.focus();
      
      syncActiveTabFromTextarea();
      if (state.editorPreviewMode !== "edit") renderPreview();
    }
  });
}
