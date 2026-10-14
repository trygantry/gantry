# Getting Started with Gantry

This guide will help you install, configure, and run Gantry in under 5 minutes. You'll learn how to initialize a launch vault, manage blockers and QA checklists, run verification checks locally and in CI, and open the desktop dashboard.

For the complete engineer, release-manager, troubleshooting, and administration workflow,
continue with the [Gantry User and Administrator Manual](manual.md).

---

## 1. Install Gantry

### macOS or Linux CLI (recommended)

This installs the latest prebuilt binary as `~/.local/bin/gantry` after verifying its
published SHA-256 checksum:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/trygantry/gantry/main/install.sh | sh
```

Homebrew users can run `brew install trygantry/tap/gantry`.

### Windows CLI

Run this in PowerShell. It installs without administrator access and updates the user
`PATH`:

```powershell
irm https://raw.githubusercontent.com/trygantry/gantry/main/install.ps1 | iex
```

### macOS desktop

Download the Apple Silicon or Intel DMG from
[GitHub Releases](https://github.com/trygantry/gantry-distribution/releases/latest). The desktop app
supports macOS 14 or newer. The current v0.5.2 DMGs are intentionally unsigned and
require Control-click → **Open** on first launch. Automatic desktop updates are not
included in unsigned releases.

Cargo and source builds are documented in [CONTRIBUTING.md](../CONTRIBUTING.md) for
contributors who need a development build.

From the desktop project picker, **Explore a mobile release sample** opens the in-memory
Northstar Mobile portfolio. It is safe to audit, schedule, attach sample evidence, and
approve or reject; the scenario resets without changing files in a real repository.

---

## 2. Initializing Your Launch Vault

Navigate to your project's repository root and run:
```bash
gantry init --auto
```

For an iOS/Android repository, use the mobile path to create the general release vault
and mobile policy together:

```bash
gantry init --mobile
gantry mobile detect
gantry mobile audit
```

Gantry will inspect your codebase to detect your technology stack (e.g., Rust, Node.js, Flutter, Python) and scaffold a default launch vault in a `.gantry/` folder.

**Choosing a template manually** — use `--template` with one of the following values:

| Template | Best for |
| :--- | :--- |
| `mobile-app` | iOS / Android apps (Flutter, Swift, Kotlin) |
| `saas-app` | Web SaaS products with Stripe, analytics, and DB migrations |
| `ai-app` | AI-native products with model versioning and evaluation gates |
| `backend-api` | REST / GraphQL / gRPC backend services |
| `desktop-app` | Desktop apps (Tauri, Electron, Qt) |
| `chrome-extension` | Chrome / Edge extensions using Manifest V3 |

```bash
gantry init --template saas-app
```

**Interactive wizard** — prompts you to choose a template:

```bash
gantry init --interactive
```

> **Note:** `--auto`, `--template`, and `--interactive` are mutually exclusive.

The `.gantry/` folder contains:
*   `config.json`: The core settings, environments, and gating policies.
*   `go_no_go.md`: Stakeholder sign-off and launch-day checklist gates.
*   `tasks.md`: The shared work queue for repair tasks.
*   `AGENTS.md`: Guidelines for AI coding assistants.
*   `schema/`: Local JSON schemas that enforce formatting rules on all vault files.
*   `blockers/` and `qa/`: Directory templates for tracking issues and test runs.
*   `mobile-policy.json`: Versioned target-SDK, placeholder, warning, and manual-evidence policy.
*   `mobile/audits/`: Candidate-bound JSON, Markdown, or HTML release packets.

### Mobile candidate audit

Run a source-only audit while developing, then audit the exact signed candidate in CI:

```bash
gantry mobile audit
gantry mobile audit --artifact build/app/outputs/bundle/release/app-release.aab --ci
```

Use an `.ipa`, `.aab`, or `.apk`. The resulting packet records the Git commit, artifact
SHA-256, packaged identity/version data, signing-material status, policy version, and
every finding. See [Mobile Release Governance](mobile-release-governance.md).

After an upgrade or interrupted machine/process, verify durable records with:

```bash
gantry vault check
# Only when a complete interrupted transaction is reported:
gantry vault check --repair
```

---

## 3. Managing Blockers, QA, and Sign-offs

All launch decisions and states are tracked in flat markdown files. This makes editing them extremely easy for both humans and AI agents.

### A. Blockers
Blockers are markdown files stored in `.gantry/blockers/`. Each blocker must have YAML frontmatter declaring its status, severity, and owner:
```markdown
---
type: blocker
id: BLK-101
title: App Crashes on Purchasing Monthly Subscription
severity: P0
status: open
owner: "@dev_jane"
source_files:
  - src/purchases/paywall.ts
last_verified: 2026-06-01
---
# Blocker details and reproduction steps here.
```
*   **To resolve a blocker**: Change `status: open` to `status: resolved` in the frontmatter.
*   **P0 Blockers**: A single open P0 blocker will immediately block a launch (resulting in a `No-Go` status).

### B. QA Checklists
QA verification checks are tracked as checklists in `.gantry/qa/` files:
```markdown
---
type: qa
id: QA-REGRESSION
title: Release Regression Plan
owner: "@qa_bob"
platforms:
  - ios
  - android
---
# Test Cases
- [x] **Authentication:** Log in via Google and Email. <!-- @qa_bob 2026-06-01 -->
- [ ] **Onboarding Flow:** Verify questionnaire screens.
```
Checklist items have attribution comments like `<!-- @owner YYYY-MM-DD by:@author -->`. Toggling items in the GUI dashboard automatically signs them with your local `git config` username.

### C. Go/No-Go Sign-offs
Stakeholder approvals are tracked under the gates section in `.gantry/go_no_go.md`:
```markdown
---
type: go_no_go
release: 1.0.0
target_date: 2026-06-15
---
# Gates
- [x] **Product:** Approved. <!-- @pm_dan 2026-06-01 by:@pm_dan -->
- [ ] **Engineering:** DB migration validation.
```

---

## 4. Checking Readiness Status and CI Gating

To check your launch-readiness state and audit logs locally, run:
```bash
gantry summarize
```
This prints a plain-language summary of your current blockers, checklists, and sign-offs.

### Running in CI
Add the validation step to your CI pipeline:
```bash
gantry validate --ci
```
*   **Exit Code `0` (Go)**: The build passes if all required gates are checked, QA completion criteria are met, and no open P0 blockers remain.
*   **Exit Code `1` (No-Go)**: The build fails if the launch is blocked.
*   **Exit Code `2` (Error)**: The build fails if there are syntax or structural issues inside the vault (e.g. malformed JSON).

---

## 5. Opening the Desktop Dashboard

Open Gantry from Applications, then choose a repository from the project picker. You can
also choose **Explore a mobile release sample** to evaluate the workflow without writing
to a real repository.

The desktop dashboard provides a rich, responsive interface where you can:
*   **Monitor Readiness Score**: View the overall readiness percentage, color-coded status, and reasons.
*   **Accept Risk with Waivers**: Temporarily waive a blocker or checklist requirement by adding a time-bounded waiver (`waivers.json`).
*   **Attach Evidence**: Link test run logs, build URLs, or audit receipts (`evidence/`) as proof of quality.
*   **View Release History & Manifests**: Review content-addressed release records and track stability trends.
*   **Govern Mobile Releases**: Run source/artifact audits, review prioritized findings, and persist release packets from the Mobile Release workspace.
