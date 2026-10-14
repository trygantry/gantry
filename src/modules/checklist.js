import { state } from './state.js';
import { loadFileContent } from './api.js';

export function showToast(message, tone = "warning") {
  let toast = document.getElementById("gantry-toast");
  if (!toast) {
    toast = document.createElement("div");
    toast.id = "gantry-toast";
    toast.className = "gantry-toast";
    document.body.appendChild(toast);
  }
  toast.textContent = message;
  toast.className = `gantry-toast ${tone} active`;
  clearTimeout(showToast._timer);
  showToast._timer = setTimeout(() => toast.classList.remove("active"), 4200);
}

export function canWriteChecklistFile(path) {
  const tab = state.openEditorTabs.find(t => t.path === path);
  if (tab?.dirty) {
    showToast(`Please save or discard your unsaved changes in the Editor for ${path} before toggling items.`);
    return false;
  }
  return true;
}

export async function syncOpenEditorTab(path) {
  const tab = state.openEditorTabs.find(t => t.path === path);
  if (!tab || tab.dirty) return;
  const content = await loadFileContent(path);
  tab.content = content;
  tab.savedContent = content;
  if (state.activeFilePath === path) {
    const textarea = document.getElementById("editor-field");
    if (textarea) textarea.value = content;
  }
}

export function statusForChecked(checked) {
  return {
    status: checked ? "checked" : "unchecked",
    label: checked ? "Passed" : "Pending",
    className: checked ? "completed" : "pending"
  };
}
