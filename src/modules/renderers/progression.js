import { state, setCachedIndex } from '../state.js';
import { normalizeIndexPayload } from '../contract.js';
import { getReadinessMeta } from '../contract.js';

/**
 * Renders the release progression strip (environments list).
 * 
 * @param {HTMLElement} containerEl - Container element
 * @param {import('../contract.js').IndexPayload} payload - Cloned index payload
 */
export function renderReleaseProgression(containerEl, payload) {
  if (!containerEl) return;
  containerEl.textContent = "";

  const container = document.createElement("div");
  container.className = "progression-container";

  const envResults = payload.environment_results || {};
  const activeEnv = state.activeEnv || payload.active_environment || "production";

  // Ensure active environment is present in results or keys list
  const envKeys = Object.keys(envResults).sort();
  if (envKeys.length === 0 && !envKeys.includes(activeEnv)) {
    envKeys.push(activeEnv);
  }

  envKeys.forEach(env => {
    const isCurrentlyActive = env === activeEnv;
    const envData = envResults[env] || {
      readiness: payload.readiness,
      readiness_score: payload.readiness_score,
      readiness_reasons: payload.readiness_reasons,
      telemetry_status: payload.telemetry_status || "not_required"
    };

    const { readinessStatusClass } = getReadinessMeta(envData.readiness);

    const step = document.createElement("div");
    step.className = `progression-step ${isCurrentlyActive ? "active" : ""}`;

    // Header (Env name + Score)
    const header = document.createElement("div");
    header.className = "progression-header";

    const nameEl = document.createElement("span");
    nameEl.className = "progression-name";
    nameEl.textContent = env;

    const scoreEl = document.createElement("span");
    scoreEl.className = `progression-score ${readinessStatusClass}`;
    scoreEl.textContent = `${envData.readiness_score || 0}%`;

    header.appendChild(nameEl);
    header.appendChild(scoreEl);

    // Meta (Readiness status + Telemetry freshness)
    const meta = document.createElement("div");
    meta.className = "progression-meta";

    const statusEl = document.createElement("span");
    statusEl.className = `progression-status ${readinessStatusClass}`;
    statusEl.textContent = envData.readiness || "At Risk";

    const telemetryEl = document.createElement("span");
    telemetryEl.className = "progression-telemetry";
    telemetryEl.textContent = (envData.telemetry_status || "not_required").toUpperCase();

    meta.appendChild(statusEl);
    meta.appendChild(telemetryEl);

    // Reason
    const reasonEl = document.createElement("div");
    reasonEl.className = "progression-reason";
    const topReason = envData.readiness_reasons && envData.readiness_reasons.length > 0
      ? envData.readiness_reasons[0]
      : "All systems nominal.";
    reasonEl.textContent = topReason;
    reasonEl.title = topReason;

    step.appendChild(header);
    step.appendChild(meta);
    step.appendChild(reasonEl);

    // Interactive switcher
    step.addEventListener("click", () => {
      if (state.activeEnv !== env) {
        state.activeEnv = env;
        
        // Re-normalize using the selected environment
        const sourceIndex = state.realIndexRaw || state.realIndex || payload;
        const normalized = normalizeIndexPayload(sourceIndex, env);
        setCachedIndex(normalized);

        // Update dropdown selector if visible
        const envSelect = document.getElementById("header-release-env-select");
        if (envSelect) {
          envSelect.value = env;
          envSelect.className = "release-env " + env.toLowerCase();
        }

        // Re-render UI
        if (window.refreshUI) {
          window.refreshUI();
        }
      }
    });

    container.appendChild(step);
  });

  containerEl.appendChild(container);
}
