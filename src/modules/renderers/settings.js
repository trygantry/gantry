import { loadFileContent, saveFileContent } from '../api.js';
import { state } from '../state.js';
import { showToast } from '../checklist.js';
import { startCountdown } from '../utils.js';
import { mockLaunchIndex } from '../constants.js';
import { runUpdateCheck } from '../updater.js';
import { initializeVaultOperations } from '../vault-operations.js';

let _initialized = false;

export async function renderSettings() {
  const rawContent = await loadFileContent("config.json");
  let config = {};
  
  try {
    if (rawContent && rawContent.trim() !== "") {
      config = JSON.parse(rawContent);
    }
  } catch (e) {
    console.error("Failed to parse config.json:", e);
  }

  // Populate metadata fields
  const vaultNameInput = document.getElementById("settings-vault-name");
  const versionInput = document.getElementById("settings-version");
  const releaseDateInput = document.getElementById("settings-release-date");
  const releaseTimeInput = document.getElementById("settings-release-time");
  const reportUrlInput = document.getElementById("settings-report-url");
  const sentryToggle = document.getElementById("settings-sentry-toggle");
  const sentryFields = document.getElementById("settings-sentry-fields");
  const sentryOrgInput = document.getElementById("settings-sentry-org");
  const sentryProjectInput = document.getElementById("settings-sentry-project");
  const sentryCrashRateInput = document.getElementById("settings-sentry-crash-rate");
  const themeSelect = document.getElementById("settings-theme-select");

  if (vaultNameInput) vaultNameInput.value = config.vaultName || "";
  if (versionInput) versionInput.value = config.version || "";
  if (releaseDateInput) {
    let rawVal = config.releaseDate || "";
    let dateVal = "";
    let timeVal = "00:00";
    if (rawVal) {
      if (rawVal.includes("T")) {
        const parts = rawVal.split("T");
        dateVal = parts[0];
        timeVal = parts[1].substring(0, 5); // HH:MM
      } else {
        dateVal = rawVal;
      }
    }
    releaseDateInput.value = dateVal;
    if (releaseTimeInput) {
      releaseTimeInput.value = timeVal;
    }
  }
  if (reportUrlInput) reportUrlInput.value = config.reportUrl || "";

  const hasSentry = !!config.sentry;
  if (sentryToggle) sentryToggle.checked = hasSentry;
  if (sentryFields) sentryFields.style.display = hasSentry ? "flex" : "none";

  if (config.sentry) {
    if (sentryOrgInput) sentryOrgInput.value = config.sentry.organization || "";
    if (sentryProjectInput) sentryProjectInput.value = config.sentry.project || "";
    if (sentryCrashRateInput) {
      sentryCrashRateInput.value = config.sentry.targetCrashFreeRate !== undefined 
        ? config.sentry.targetCrashFreeRate 
        : "";
    }
  } else {
    if (sentryOrgInput) sentryOrgInput.value = "";
    if (sentryProjectInput) sentryProjectInput.value = "";
    if (sentryCrashRateInput) sentryCrashRateInput.value = "";
  }

  if (themeSelect) {
    themeSelect.value = localStorage.getItem("gantry-theme") || "default";
  }

  // Guard against duplicate listener binding
  if (_initialized) {
    return;
  }
  _initialized = true;
  initializeVaultOperations();

  // Toggle Sentry inputs collapse/expand
  if (sentryToggle) {
    sentryToggle.addEventListener("change", (e) => {
      if (sentryFields) {
        sentryFields.style.display = e.target.checked ? "flex" : "none";
      }
    });
  }

  // Theme switcher selection binding
  if (themeSelect) {
    themeSelect.addEventListener("change", (e) => {
      const value = e.target.value;
      // Strip out existing theme classes
      document.body.className = document.body.className.replace(/\btheme-\S+/g, '').trim();
      
      if (value !== 'default') {
        document.body.classList.add(`theme-${value}`);
      }
      localStorage.setItem("gantry-theme", value);
    });
  }

  // Check for updates button
  const updateBtn = document.getElementById("btn-check-updates");
  const updateStatus = document.getElementById("check-updates-status");
  if (updateBtn) {
    updateBtn.addEventListener("click", async () => {
      updateBtn.disabled = true;
      if (updateStatus) updateStatus.textContent = "Checking…";
      try {
        await runUpdateCheck({ silent: false });
        if (updateStatus) updateStatus.textContent = "";
      } catch (err) {
        if (updateStatus) updateStatus.textContent = `Failed: ${err.message || err}`;
      } finally {
        updateBtn.disabled = false;
      }
    });
  }

  // Save Config action
  const saveBtn = document.getElementById("btn-save-config");
  if (saveBtn) {
    saveBtn.addEventListener("click", async (e) => {
      e.preventDefault();
      
      try {
        const originalRaw = await loadFileContent("config.json");
        let payload = {};
        
        try {
          if (originalRaw && originalRaw.trim() !== "") {
            payload = JSON.parse(originalRaw);
          }
        } catch (err) {
          console.warn("Original config file parsing failed during save, starting fresh.");
        }

        // Bind standard metadata inputs
        payload.vaultName = (vaultNameInput?.value || "").trim();
        payload.version = (versionInput?.value || "").trim();
        
        const dateVal = (releaseDateInput?.value || "").trim();
        const timeVal = (releaseTimeInput?.value || "00:00").trim();
        if (dateVal) {
          payload.releaseDate = `${dateVal}T${timeVal}`;
        } else {
          payload.releaseDate = "";
        }
        payload.reportUrl = (reportUrlInput?.value || "").trim();

        const sentryOn = sentryToggle ? sentryToggle.checked : false;
        if (sentryOn) {
          payload.sentry = {
            organization: (sentryOrgInput?.value || "").trim(),
            project: (sentryProjectInput?.value || "").trim(),
            targetCrashFreeRate: parseFloat(sentryCrashRateInput?.value || "0") || 0
          };
        } else {
          // Delete completely to prevent string match on .contains("sentry")
          delete payload.sentry;
        }

        // Save serialized config
        await saveFileContent("config.json", JSON.stringify(payload, null, 2));

        // Demo Mode score/state mutation
        if (state.demoMode) {
          mockLaunchIndex.category_scores.Monitoring = sentryOn ? 100 : 70;
          mockLaunchIndex.score_reasons.Monitoring = [
            sentryOn
              ? "Sentry session telemetry and crash monitoring active."
              : "-30: Sentry crash monitoring organization or project is not configured in config.json."
          ];
        }

        // Refresh dynamic UI elements
        await window.refreshUI();

        showToast("Settings saved.", "success");
      } catch (err) {
        showToast(`Save failed: ${err.message || err}`, "warning");
      }
    });
  }
}
