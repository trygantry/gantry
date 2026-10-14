import { getCIStatus } from '../api.js';

export async function renderCiSetup() {
  const ci = await getCIStatus();

  const heroCard = document.getElementById("ci-hero-status-card");
  const heroTitle = document.getElementById("ci-hero-title");
  const heroDesc = document.getElementById("ci-hero-desc");

  if (!heroCard || !heroTitle || !heroDesc) return;

  if (ci.configured) {
    heroCard.classList.add("configured");
    const providerLabel = ci.provider === "github" ? "GitHub Actions" : ci.provider === "gitlab" ? "GitLab CI" : ci.provider.toUpperCase();
    heroTitle.textContent = `Gantry Workflow Detected (${providerLabel})`;
    heroDesc.innerHTML = `Review <code class="hash-pill">${ci.file_path}</code> against the candidate-bound example below, then require its release-gate job in branch protection.`;
  } else {
    heroCard.classList.remove("configured");
    heroTitle.textContent = "Mobile Release Gate Not Detected";
    heroDesc.textContent = "Add the canonical Action, then make its job a required check on the protected release branch.";
  }
}
