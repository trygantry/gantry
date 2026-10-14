import { state, setSelectedQASuitePath } from '../state.js';
import { loadFileContent, toggleChecklistItem } from '../api.js';
import { parseTextBoldFormatting, stripComments } from '../utils.js';
import { canWriteChecklistFile, showToast, syncOpenEditorTab } from '../checklist.js';

export async function renderQARegression() {
  const container = document.getElementById("qa-regression-list");
  const title = document.getElementById("qa-card-title");
  const tabs = document.getElementById("qa-suite-tabs");
  if (!container) return;

  const suites = state.cachedIndex.qa_suites || [];
  if (suites.length === 0) {
    if (title) title.textContent = "QA Suite Toggles";
    if (tabs) tabs.textContent = "";
    container.innerHTML = `<div class="empty-state text-muted">No QA plans found in the vault.</div>`;
    return;
  }

  const selectedPath = suites.some(s => s.path === state.selectedQASuitePath)
    ? state.selectedQASuitePath
    : suites[0].path;
  if (selectedPath !== state.selectedQASuitePath) setSelectedQASuitePath(selectedPath);
  const selectedSuite = suites.find(s => s.path === selectedPath) || suites[0];

  if (title) title.textContent = selectedSuite.title || selectedSuite.path;
  renderSuiteTabs(tabs, suites, selectedPath);
  await renderSuiteChecklist(container, selectedSuite);
}

function renderSuiteTabs(tabs, suites, selectedPath) {
  if (!tabs) return;
  tabs.textContent = "";
  if (suites.length <= 1) return;

  suites.forEach(suite => {
    const btn = document.createElement("button");
    btn.className = `qa-suite-tab${suite.path === selectedPath ? " active" : ""}`;
    btn.textContent = suite.title || suite.id || suite.path;
    btn.title = suite.path;
    btn.addEventListener("click", async () => {
      setSelectedQASuitePath(suite.path);
      await renderQARegression();
    });
    tabs.appendChild(btn);
  });
}

async function renderSuiteChecklist(container, suite) {
  const path = suite.path;
  const md = await loadFileContent(path);
  const lines = md.split("\n");
  container.textContent = "";

  let rendered = 0;
  lines.forEach((line, index) => {
    const checklistMatch = line.match(/^\s*-\s*\[([ x/])\]\s*(.+)$/i);
    if (!checklistMatch) return;
    rendered += 1;

    const checked = checklistMatch[1].toLowerCase() === "x";
    const text = checklistMatch[2];

    const itemEl = document.createElement("div");
    itemEl.className = "checklist-item";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.className = "checklist-checkbox";
    checkbox.checked = checked;

    checkbox.addEventListener("change", async () => {
      const previous = !checkbox.checked;
      if (!canWriteChecklistFile(path)) {
        checkbox.checked = previous;
        return;
      }

      checkbox.disabled = true;
      try {
        await toggleChecklistItem(path, index, checkbox.checked);
        await syncOpenEditorTab(path);
        if (window.refreshUI) await window.refreshUI();
      } catch (err) {
        checkbox.checked = previous;
        showToast(`Could not update ${path}: ${err.message || err}`, "error");
      } finally {
        checkbox.disabled = false;
      }
    });

    const content = document.createElement("div");
    content.className = "checklist-content";

    const textEl = document.createElement("div");
    textEl.className = "checklist-text";
    parseTextBoldFormatting(textEl, stripComments(text));

    content.appendChild(textEl);
    itemEl.appendChild(checkbox);
    itemEl.appendChild(content);
    container.appendChild(itemEl);
  });

  if (rendered === 0) {
    const emptyEl = document.createElement("div");
    emptyEl.className = "empty-state text-muted";
    emptyEl.textContent = `No checklist items found in ${path}.`;
    container.appendChild(emptyEl);
  }
}
