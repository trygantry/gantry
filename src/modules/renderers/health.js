import { state } from '../state.js';
import { hasTauri, openInIde } from '../api.js';

export function renderDocsHealth() {
  const { cachedIndex } = state;
  renderMissingDocs(cachedIndex);
  renderStaleDocs(cachedIndex);
  renderValidationErrors(cachedIndex);
  renderBrokenLinks(cachedIndex);
}

function makeEmptyState(text) {
  const el = document.createElement("div");
  el.className = "empty-state text-go";
  el.textContent = `✓ ${text}`;
  return el;
}

function renderMissingDocs(cachedIndex) {
  const container = document.getElementById("health-missing-docs");
  if (!container) return;
  container.textContent = "";

  const missing = cachedIndex.missing_docs || [];
  if (missing.length === 0) {
    container.appendChild(makeEmptyState("All required documents present"));
    return;
  }

  missing.forEach(doc => {
    const el = document.createElement("div");
    el.className = "checklist-item";

    const icon = document.createElement("span");
    icon.className = "checklist-item-icon";
    icon.style.cssText = "font-family: var(--font-mono); font-size: 9px; font-weight: 700; padding: 2px 6px; border-radius: 2px; background: rgba(239, 68, 68, 0.1); border: 1px solid rgba(239, 68, 68, 0.2); color: var(--color-blocker); min-width: 54px; text-align: center;";
    icon.textContent = "MISSING";

    const text = document.createElement("span");
    text.className = "health-mono text-pending";
    text.textContent = doc;

    el.appendChild(icon);
    el.appendChild(text);
    container.appendChild(el);
  });
}

function renderStaleDocs(cachedIndex) {
  const container = document.getElementById("health-stale-docs");
  if (!container) return;
  container.textContent = "";

  const stale = cachedIndex.stale_docs || [];
  if (stale.length === 0) {
    container.appendChild(makeEmptyState("All documents are up to date"));
    return;
  }

  stale.forEach(doc => {
    const el = document.createElement("div");
    el.className = "checklist-item";
    if (hasTauri) el.style.cursor = "pointer";

    const icon = document.createElement("span");
    icon.className = "checklist-item-icon";
    icon.style.cssText = "font-family: var(--font-mono); font-size: 9px; font-weight: 700; padding: 2px 6px; border-radius: 2px; background: rgba(245, 158, 11, 0.1); border: 1px solid rgba(245, 158, 11, 0.2); color: var(--color-pending); min-width: 54px; text-align: center;";
    icon.textContent = "STALE";

    const text = document.createElement("span");
    text.className = "health-mono text-pending";
    text.textContent = doc;

    el.appendChild(icon);
    el.appendChild(text);

    if (hasTauri) {
      el.addEventListener("click", async () => {
        try { await openInIde(doc); } catch (e) { console.warn("Open in IDE failed:", e); }
      });
    }

    container.appendChild(el);
  });
}

function renderValidationErrors(cachedIndex) {
  const container = document.getElementById("health-validation-errors");
  if (!container) return;
  container.textContent = "";

  const errors = cachedIndex.validation_errors || [];
  if (errors.length === 0) {
    container.appendChild(makeEmptyState("No schema validation issues"));
    return;
  }

  errors.forEach(err => {
    const el = document.createElement("div");
    el.className = "checklist-item";

    const icon = document.createElement("span");
    icon.className = "checklist-item-icon";
    const isError = err.severity === "error";
    icon.style.cssText = `font-family: var(--font-mono); font-size: 9px; font-weight: 700; padding: 2px 6px; border-radius: 2px; background: ${isError ? "rgba(239, 68, 68, 0.1)" : "rgba(245, 158, 11, 0.1)"}; border: 1px solid ${isError ? "rgba(239, 68, 68, 0.2)" : "rgba(245, 158, 11, 0.2)"}; color: ${isError ? "var(--color-blocker)" : "var(--color-pending)"}; min-width: 54px; text-align: center;`;
    icon.textContent = isError ? "ERROR" : "WARN";

    const text = document.createElement("span");
    text.className = `health-text ${err.severity === "error" ? "text-blocker" : "text-pending"}`;
    text.textContent = err.message;

    el.appendChild(icon);
    el.appendChild(text);

    if (err.file) {
      const file = document.createElement("span");
      file.className = "health-file text-muted";
      file.textContent = err.file;
      el.appendChild(file);
    }

    container.appendChild(el);
  });
}

function renderBrokenLinks(cachedIndex) {
  const container = document.getElementById("health-broken-links");
  if (!container) return;
  container.textContent = "";

  const links = cachedIndex.broken_links || [];
  if (links.length === 0) {
    container.appendChild(makeEmptyState("No broken internal links"));
    return;
  }

  links.forEach(link => {
    const el = document.createElement("div");
    el.className = "checklist-item";

    const icon = document.createElement("span");
    icon.className = "checklist-item-icon";
    icon.style.cssText = "font-family: var(--font-mono); font-size: 9px; font-weight: 700; padding: 2px 6px; border-radius: 2px; background: rgba(239, 68, 68, 0.1); border: 1px solid rgba(239, 68, 68, 0.2); color: var(--color-blocker); min-width: 54px; text-align: center;";
    icon.textContent = "LINK";

    const text = document.createElement("span");
    text.className = "health-mono text-blocker";
    text.textContent = typeof link === "string" ? link : (link.message || link.path || String(link));

    el.appendChild(icon);
    el.appendChild(text);
    container.appendChild(el);
  });
}
