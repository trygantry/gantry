# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.2] - 2026-08-09

### Fixed — crates.io distribution

- Made the published `gantry-core` package self-contained by shipping its embedded mobile
  policy and audit-report schemas inside the crate.
- Added a source-of-truth drift test for the packaged schema copies.
- Fixed manual crate publication to use the requested release tag instead of GitHub's
  reserved workflow ref name.

## [0.5.1] - 2026-08-08

### Changed — installation and distribution

- Added checksum-verifying macOS/Linux and PowerShell installers that normalize the
  platform-specific downloads to `gantry`/`gantry.exe` without requiring administrator
  access.
- Changed the first-party GitHub Action to download its matching prebuilt CLI instead of
  installing Rust and compiling Gantry inside every consumer repository.
- Added normalized tar/ZIP release archives, per-archive checksums, and one aggregate
  `SHA256SUMS` manifest with fail-closed release verification.
- Preserved the explicit unsigned macOS distribution path while keeping optional Apple
  signing, notarization, and signed updater verification available when credentials are
  deliberately configured.
- Prepared `gantry-core` and `gantry-cli` package metadata for crates.io and added a
  manual registry publication workflow.
- Replaced the long GitHub front page and stale imagery with direct install paths, a
  real current-product screenshot, and precise trust boundaries.

## [0.5.0] - 2026-08-06

Gantry 0.5.0 is the mobile release governance release. It turns the local-first
readiness engine into a candidate-bound workflow for iOS and Android source, signed
IPA/AAB/APK artifacts, durable evidence, accountable decisions, and portable release
packets. Gantry Community remains fully local and Apache-2.0.

### Changed — Release distribution and operator hardening

- Replaced recoverable production panic paths with actionable errors across Rust core,
  CLI, APK parsing, watchers, and desktop startup.
- Added a fail-closed tag-release quality gate, version agreement check, CLI checksums,
  optional Apple/updater signing configuration, macOS-only desktop publication, and
  signed/notarized/updater verification when credentials are configured. Credential-free
  release runs produce unsigned DMGs without updater artifacts and still verify launch.
- Added real rendered-DOM tests for engineers, release managers, administrators, and
  hidden-dialog accessibility; removed the remote font dependency for offline startup.
- Added adversarial permission, archive, transaction, and representative-artifact tests
  plus retained CI branch-coverage evidence.
- Moved redacted support-bundle generation into shared core code and added Desktop
  Settings controls for vault health, repair, migration preview, and support bundles.

### Changed — Final product-truth and UX pass

- Replaced the Policies navigation dead-end with a dedicated policy-as-code workspace covering
  enforcement, candidate integrity, reviewable exceptions, and policy identity.
- Replaced legacy CI snippets with the root Action's candidate-bound workflow contract and a
  portable durable-release CLI sequence.
- Made hidden actions fail closed at the CSS foundation, removed duplicate workspace tabs and
  the visible legacy navigation, added compact completed onboarding, and improved text contrast
  and narrow effective-viewport reflow.
- Extracted the native runtime transport boundary and removed production fallbacks that could
  replace failed real reads with demo data.
- Expanded rendered UI coverage for Policies, canonical CI inputs, product edition copy, hidden
  audit cancellation, WCAG contrast, and 480px reflow.

### Added — Release durability and operability

- Checksummed, recoverable multi-file release transactions, cross-process commit locking,
  stale event-head rejection, and parent-directory durability syncing.
- `gantry vault check [--repair]`, `gantry vault migrate [--dry-run]`, and a redacted
  `gantry vault support-bundle` diagnostic.
- Cooperative cancellation for desktop mobile audits.
- Generated TypeScript contracts, runtime IPC schema guards, and automated mobile
  workflow/accessibility acceptance coverage.
- A canonical user and administrator manual plus documentation index and audit.

### Added — Mobile Release Governance

- Versioned mobile report schema `2.0` and policy pack `2026.07` in the new
  GUI-free `gantry-core::mobile` module.
- Flutter, React Native, native iOS, and native Android first-party project detection.
- iOS plist/privacy/entitlement metadata inspection and Android Gradle/manifest policy
  checks for identity, SDK, versioning, permissions, signing configuration, debug state,
  cleartext traffic, and production placeholders.
- Portable, bounded IPA/AAB/APK inspection with candidate SHA-256, packaged identity,
  versions, dependency/component inventory, permissions, privacy coverage, and platform
  signature verification through `codesign`, `apksigner`, or `jarsigner`.
- Artifact- or commit-bound manual evidence reconciliation using existing
  `.gantry/evidence/*.json` records.
- `gantry mobile detect`, `audit`, `rules`, and `init`, plus `gantry init --mobile`.
- Canonical JSON and portable Markdown/HTML mobile release packets under
  `.gantry/mobile/audits/`.
- GitHub Action `mobile-audit` and `mobile-artifact` inputs, PR packet output, and No-Go
  enforcement.
- Desktop Mobile Release workspace and Tauri APIs for policy setup and source/artifact
  audits.
- Core fixture coverage and end-to-end CLI tests for mobile detection, artifacts,
  reporting, initialization, and exit codes.
- Published policy/report JSON Schemas, policy and report fingerprints, exact finding
  instances, signer allowlists, strict evidence validation, and real JDK signing tests.
- Durable workflow releases with `release create`, `list`, `show`, `candidate add`, and
  `release audit`; candidate-bound reports are persisted, re-verified, and invalidated
  when a replacement candidate becomes active.
- Candidate/finding-scoped `release evidence add` and `release finding resolve` workflows
  with evidence payload digests, event verification, stale-candidate semantics, and
  deterministic re-audit enforcement.
- Accountable `release approve`, `release reject`, and
  `release waive` persist exact audit/report bindings and become stale on re-audit or
  candidate replacement. No-Go approvals fail closed and waivers never override policy.
- Deterministic `release packet` exports the complete verified workflow as content-addressed
  JSON, Markdown, and HTML; `release diff` compares immutable candidates, latest audits,
  finding transitions, policy changes, and their verified events.
- Desktop Saved releases workspace with verified list/detail reopening, platform candidate
  and audit context, accepted-risk/timeline review, packet generation, candidate comparison,
  and accountable approve/reject operations backed by the same core release store.
- Core-owned `release portfolio` projection with owner/stage/age/platform context,
  deterministic next actions, and prioritized blocker, approval, rejection, accepted-risk,
  waiver-expiry, and active-evidence-expiry queues shared by CLI and desktop.
- Release-manager cockpit metrics, queue filters/search, queue-to-release drill-down, Git
  identity defaults, and in-context verified handoff summaries for generated packets.
- Handoff-first Markdown/HTML packets summarizing status, exact candidates, accountable
  decisions, open findings, accepted risks, and the next action before the detailed record.
- Release-first desktop navigation for Portfolio, Audit Candidate, Policies, CI &
  Integrations, and Settings, with the former launch-readiness tools preserved in a
  collapsed compatibility section.
- One-step promotion of a completed candidate audit into a durable release record and
  candidate-bound workflow audit.
- Explicit `approved_with_risk` portfolio truth, counts, review work, and handoff status
  so human approval never hides unresolved automated risk.
- Verified release target, submission, and rollout dates with urgency queues, CLI/Tauri
  schedule operations, calendar/list portfolio presentation, and packet handoff context.
- Guided six-step first-release progress, richer finding remediation controls, decision
  readiness checks, and verified “since last review” summaries.

### Changed — Mobile Release Governance

- Signature material alone no longer passes `MOB-ART-004`; unavailable verifier tooling
  is explicit and can be made blocking by policy.
- Consolidated the GitHub Action to root `action.yml` and use one mobile audit result for
  both the PR packet and enforcement.
- Mobile desktop audits run off the UI thread, return the saved packet path, and use a
  native project-scoped artifact picker.
- Candidate results become visibly stale when the selected artifact or prepared policy
  changes and cannot be promoted until rerun.
- Release search keeps the list and detail selection synchronized; candidate replacement
  and At Risk approval now require explicit in-context review and confirmation.
- Hidden dialogs are removed from the accessibility tree, trap focus while open, support
  Escape cancellation, and restore focus to the invoking control.
- Tightened portfolio typography, contrast, metric semantics, status badges, and rendered
  Markdown handoff presentation for engineer and release-manager scanning.

- Updated package and desktop presentation versions to `0.5.0`.
- Renamed the internal desktop executable to `gantry-desktop` (the installed product
  remains “Gantry”) to prevent case-insensitive macOS build collisions with the
  `gantry` CLI binary.
- Reframed the primary product entry point around artifact-bound iOS/Android release
  governance while retaining backward compatibility with existing Gantry vaults and
  general readiness commands.
- Added `plist` and `zip` dependencies to the core for portable artifact inspection.

See [docs/mobile-release-governance.md](docs/mobile-release-governance.md) for usage,
architecture, and limitations.

## [0.4.0] - 2026-06-13

Gantry 0.4.0 is the "control plane" release — a policy-and-evidence gate between AI
coding agents and production, not just a launch checklist. Six new capabilities
complete the local enforcement story.

### Added

- **`gantry verify [--json]`** — Tamper-evident SHA-256 hash chain over the
  snapshot history (`prev_hash` + `entry_hash` fields stamped into every new
  snapshot). Walks chronologically, verifies each link, exits `0` (intact) or
  `1` (broken). Back-compatible: pre-0.4.0 snapshots without hash fields are
  silently skipped. Surfaced inside `audit export` certificates.
- **`gantry audit export [--format html|json|md] [--output <path>]`** —
  Generates a self-contained audit certificate bundling launch gates, sign-offs,
  active waivers, evidence records, readiness score, and a chain-verification
  status block. The HTML output is print-ready for auditors, boards, and
  app-store reviewers. Implemented via `gantry_core::export_audit_certificate`.
- **`gantry install-hooks [--stage commit|push] [--force]`** — Writes a
  `# --- gantry launch gate ---` block into `.git/hooks/pre-push` (or
  `pre-commit`). Pushes/commits are blocked when readiness is No-Go; agents and
  humans hit the same local gate.
- **`gantry uninstall-hooks`** — Surgically strips the Gantry block from any
  installed hooks, preserving unrelated hook content. Removes the file entirely
  if only the Gantry block remains.
- **`gantry validate --policy <url|path>`** — Fetches (HTTP) or reads (file)
  a shared scorecard JSON and overlays it on top of the local
  `.gantry/scorecard.json`: same `id` → overlay rule wins; new `id` →
  appended. Policy path is forwarded to the sub-process via
  `GANTRY_POLICY_OVERLAY`. This is the OSS seam for the hosted policy
  registry.
- **`gantry impact --markdown`** — Emits a compact Markdown table of files
  changed → affected blockers/QA suites. The GitHub Action now appends this
  delta table to every PR sticky comment automatically (PR context only).
- **`gantry watch`** — File-watcher daemon (powered by the `notify` crate)
  that monitors `.gantry/` recursively, debounces bursts (500 ms window), and
  prints a timestamped readiness line on every meaningful change. Runs an
  initial validation immediately on startup.

### Changed

- **GitHub Action `policy` input** — New `policy` input on `action.yml`;
  forwarded as `INPUT_POLICY` to the shell step. The build-gate step now passes
  `--policy "${INPUT_POLICY}"` when the input is set.
- **GitHub Action: PR comment includes impact delta** — On pull-request runs
  the action appends the `gantry impact --markdown` table after the audit
  report, separated by a horizontal rule. Failures are silently swallowed so a
  missing baseline snapshot never breaks the comment step.
- **Snapshot hash chain** — `GantrySnapshot` gains two `#[serde(default)]`
  fields (`prev_hash`, `entry_hash`); `GantryState` gains `chain_head`.
  `write_snapshot` computes and seals the entry hash before writing;
  `update_state_after_snapshot` advances `chain_head` in `state.json`.
- **README** — Updated the CLI command table with all new commands.
- **`crates/gantry-core`** — Added `sha2` and `hex` dependencies for hashing;
  bumped to `0.4.0`.
- **`crates/gantry-cli`** — Added `notify` dependency for watch mode;
  bumped to `0.4.0`.

## [0.3.0] - 2026-06-05

Gantry 0.3.0 is a stabilization release focused on reliability hardening,
pipeline integration safeguards, supply-chain visibility, and developer
onboarding.

### Added

- **Integration Test Runner** — Introduced an end-to-end integration test suite (`cli_integration.rs`) that executes the compiled `gantry` binary in temporary sandboxes, verifying `init --auto -> validate -> index -> summarize -> audit` commands and exit codes.
- **Robust Unit Testing** — Added 12 new core edge-case unit tests to `lib.rs` verifying trends compilation, commit prefix matching, Sentry telemetry rules evaluation, and Slack digest event filtering.
- **CycloneDX SBOM Generation** — Integrates `cargo-cyclonedx` into CI and release pipelines, generating JSON Software Bill of Materials (SBOM) artifacts for both core/CLI and desktop packages.
- **Contract Verification Tests** — Created JavaScript contract smoke tests (`tests/js/contract.test.js`) to assert index payload normalization and status mappings, ensuring structural compatibility between Rust core and the web GUI.
- **JS Syntax Checker** — Built a zero-dependency npm script (`npm run check:js`) to run node syntax checks against all non-vendored frontend JS files in the project.
- **Security Audits** — Added `rustsec/audit-check` cargo audits to the CI pipeline.
- **Onboarding docs** — Created developer onboarding documentation (`docs/getting-started.md`) and module separation guides (`docs/architecture.md`).

### Changed

- **Strict Config Failures** — Modified `compile_launch_index_for_environment` to fail with a clear compile error when syntax errors are detected in `config.json`, causing validation to exit with code `2` rather than continuing silently.
- **Waiver Validation Reporting** — Malformed `waivers.json` now surfaces structured validation errors instead of being ignored during index compilation.
- **Whitespace Linting** — Configured PR and push checks for trailing whitespace and line endings using `git diff --check`.
- **Release Pipeline Hardening** — Release builds now attach generated CycloneDX SBOM assets alongside CLI binaries and desktop bundles.

## [0.2.0] - 2026-06-01

### Added

- **Snapshot persistence** — `gantry index` now writes timestamped launch-state snapshots
  to `.gantry/snapshots/` and tracks latest state in `.gantry/state.json`.
- **Snapshot diff command** — `gantry diff <base>..<head> [--json]` compares readiness
  state between two commit-linked snapshots.
- **Slack digest notifications** — `gantry notify [--dry-run]` builds/sends transition
  digests with idempotency tracking through `state.json`.
- **Generalized telemetry rules** — environment policies can define `telemetryRules`
  over arbitrary metrics in `.gantry/observed.json`.
- **Impact graph API + UI** — new core/Tauri graph endpoint with desktop audit view
  toggle (`Trace` / `Graph`) and interactive node inspector.

### Changed

- **Core structure** — split major launch-cockpit logic into `graph`, `snapshot`,
  `notify`, and `telemetry` modules while preserving `gantry-core` public exports.
- **Watcher safety** — Tauri vault watcher now ignores `.gantry/state.json` and
  `.gantry/snapshots/` writes to prevent self-triggered rebuild loops.

## [0.1.0] - 2026-05-29

### Added

- **Desktop app** — Tauri + Rust launch-readiness control room with seven views:
  Launch Dashboard, Docs Health, Critical Issues, QA Readiness, Go/No-Go Room,
  What's New (Timeline), and Vault Editor.
- **`.gantry/` file contract** — markdown + YAML frontmatter + JSON vault with
  `AGENTS.md` (AI agent contract), JSON schemas, and a generated `index.json` for
  machine-readable launch state.
- **CLI** — `validate [--ci]`, `index`, `summarize`, `init [--auto]`, `audit [--dry-run] [--json] [--format markdown]`,
  `impact [--base <ref>]`, `tasks`, `task complete/ignore`, `mcp`. Stable exit codes: `0` Go · `1` blocked · `2` error.
- **GUI-free Cargo workspace** — `gantry-core` (all launch-readiness logic, zero
  Tauri deps) + `gantry-cli` (standalone CLI binary, no system deps to build) +
  `src-tauri` (thin Tauri desktop shim).
- **MCP server** — `gantry mcp` launches a stdio read-only Model Context Protocol
  server so AI coding assistants can query launch state directly.
- **Impact mapping** — `gantry impact` correlates recent git changes to open
  blockers and QA suites to surface uncovered modifications.
- **GitHub Action** (`action.yml`) — reusable `uses: trygantry/gantry@v1` action
  that validates a `.gantry/` vault and posts a sticky audit report as a PR comment.
- **Readiness engine** — Go / At Risk / No-Go computed from blockers, QA completion,
  sign-offs, stale docs, and configured telemetry gates.
- **Docs health** — detects missing required docs, invalid frontmatter, broken wiki
  links, and stale documents via `source_files` + `last_verified` freshness tracking.
- **Audit engine** — scores readiness by category, writes repair tasks to
  `.gantry/tasks.md` for AI agents or humans to action.
- Apache-2.0 license.
- CI/CD: build+test, blocking lint (fmt + clippy), and multi-platform release
  workflow publishing prebuilt CLI binaries and Tauri desktop bundles.
