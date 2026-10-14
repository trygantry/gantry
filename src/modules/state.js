import { mockLaunchIndex } from './constants.js';

export const state = {
  activeView: 'mobile',
  activeFilePath: 'go_no_go.md',
  demoMode: false,
  activeEnv: null,
  cachedIndex: mockLaunchIndex,
  realIndex: null,
  editorDirty: false,
  editorLoadedContent: "",
  editorSaveStatus: "idle",   // "idle" | "saving" | "saved" | "error"
  editorPreviewMode: "edit",  // "edit" | "split" | "preview"
  openEditorTabs: [],
  selectedQASuitePath: "",
  simConfig: {
    active: false,
    crashRate: 99.8,
    qaPct: 43,
    waivedBlocker: "",
    simulateFile: "",
    PM: false,
    Dev: false,
    QA: false
  },
  listeners: []
};

export function subscribe(callback) {
  state.listeners.push(callback);
  return () => {
    state.listeners = state.listeners.filter(l => l !== callback);
  };
}

export function notify() {
  state.listeners.forEach(l => l(state));
}

export function updateState(newState) {
  Object.assign(state, newState);
  notify();
}

export function updateSimConfig(newConfig) {
  Object.assign(state.simConfig, newConfig);
  notify();
}

export function setCachedIndex(index) {
  state.cachedIndex = index;
  notify();
}

export function setActiveView(view) {
  state.activeView = view;
  notify();
}

export function setActiveFilePath(path) {
  state.activeFilePath = path;
  notify();
}

export function setDemoMode(active) {
  state.demoMode = active;
  if (active) {
    state.cachedIndex = mockLaunchIndex;
  }
  notify();
}

export function setEditorLoaded(content) {
  state.editorLoadedContent = content;
  state.editorDirty = false;
  state.editorSaveStatus = "idle";
  notify();
}

export function setEditorDirty(val) {
  state.editorDirty = val;
  notify();
}

export function setEditorSaveStatus(val) {
  state.editorSaveStatus = val;
  notify();
}

export function setEditorPreviewMode(mode) {
  state.editorPreviewMode = mode;
  notify();
}

export function setOpenEditorTabs(tabs) {
  state.openEditorTabs = tabs;
  notify();
}

export function setSelectedQASuitePath(path) {
  state.selectedQASuitePath = path;
  notify();
}
