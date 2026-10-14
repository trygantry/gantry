# Gantry User and Administrator Manual

Version 0.5 · Updated 2026-08-08

Gantry is a local-first mobile release-governance system for engineers and release
managers. It verifies source configuration and the exact IPA, AAB, or APK intended for
release, then keeps the audit, evidence, accepted risk, and accountable human decision
bound to that candidate. Gantry is delivered as a command-line tool, a GitHub Action,
and a macOS desktop release room. It is not currently a hosted website or phone app.

## 1. Who this manual is for

- **Engineers** use Gantry to detect mobile targets, audit source and signed artifacts,
  resolve findings, and gate CI.
- **Release managers** use the desktop portfolio to prioritize releases, verify evidence,
  record accountable decisions, and export a handoff packet.
- **Administrators and maintainers** configure policy, authority tools, protected CI,
  vault health, upgrades, support bundles, and packaged desktop releases.

## 2. The Gantry mental model

Gantry separates four things that release processes often blur together:

1. **Policy** says what must be true.
2. **Candidate** identifies the exact bytes intended for distribution.
3. **Audit and evidence** show what Gantry verified and what humans or connected
   authorities attested.
4. **Decision** records who approved or rejected the active audited candidate and why.

A new candidate invalidates the previous active audit, decision, and candidate-scoped
waivers. A new audit invalidates the previous active human decision. Historical records
remain available in the event chain.

Gantry uses three automated decisions:

- **Go**: every enforced automated requirement passes.
- **At Risk**: no enforced blocker remains, but warnings, manual evidence, or unavailable
  checks require review.
- **No-Go**: at least one blocker or policy-enforced warning/manual/unavailable check
  prevents approval.

Human approval never converts a No-Go audit into Go. A waiver records accepted risk but
does not rewrite the deterministic audit result.

## 3. Product surfaces

### Product editions and data boundary

Version 0.5 is **Gantry Community**. The audit engine, CLI, GitHub Action, public
schemas, local desktop, durable release workflow, evidence, waivers, decisions, and
packet export are available in this repository under Apache-2.0. No Gantry account is
required, and Community does not transmit release data to a hosted Gantry service.

### CLI

The CLI is the authoritative automation interface. Use it locally, in CI, or from scripts.
It works without the desktop application.

### Desktop application

The macOS Tauri application is the primary release-manager interface.

It provides a release portfolio, prioritized queues, candidate audit, and evidence or
resolution actions.

Approvals, schedules, candidate comparisons, and packet export complete the workflow.

The primary navigation is the single workspace switcher: **Release Portfolio**, **Audit
Candidate**, **Policies**, **CI & Integrations**, and **Settings**. The Policies workspace
explains enforced decision behavior, candidate integrity, reviewable exceptions, and policy
identity; it can safely create/verify or open the repository policy.

### GitHub Action

The composite Action runs the same Rust core in CI and can publish a durable verified
release packet. Configure it as a required check for enforcement.

### Hosted and mobile surfaces

There is no hosted Gantry service or native phone application in 0.5. Settings identifies
the current Apache-2.0 Community edition and does not expose a connection flow.

Those capabilities are not implied by the local application.

## 4. Install and open Gantry

### Install the CLI

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/trygantry/gantry/main/install.sh | sh
```

Homebrew users can run `brew install trygantry/tap/gantry`. Windows users can run
`irm https://raw.githubusercontent.com/trygantry/gantry/main/install.ps1 | iex` in
PowerShell.

### Install the desktop application

Download the appropriate macOS DMG from [GitHub Releases](https://github.com/trygantry/gantry-distribution/releases/latest) and drag Gantry to Applications.
The current v0.5.2 DMGs are intentionally unsigned and require the documented macOS
first-launch override.

### Initialize a mobile repository

Run Gantry from the repository root:

```bash
gantry init --mobile
gantry mobile detect
gantry mobile audit
```

Initialization creates the `.gantry/` vault and `.gantry/mobile-policy.json` without
moving or replacing the application source already stored in GitHub.

## 5. Engineer quickstart

### Step 1: Verify target detection

```bash
gantry mobile detect --json
```

Review the first-party iOS and Android targets. Dependency lockfiles are inventoried
separately and do not become application targets.

### Step 2: Run a source-readiness audit

```bash
gantry mobile audit
```

Use this during development. It is useful, but it is not a shipping decision because no
exact release candidate is bound.

### Step 3: Build and audit the exact candidate

```bash
gantry mobile audit \
  --artifact build/app/outputs/bundle/release/app-release.aab \
  --write
```

Supported artifact types are IPA, AAB, and APK. Gantry streams the candidate SHA-256,
checks bounded archive metadata, invokes available platform signature authorities, and
records any limitation or unavailable authority explicitly.

### Step 4: Start a durable release

```bash
gantry release ensure 2.7.0 \
  --name "July production release" \
  --owner release@example.com \
  --actor engineer@example.com

gantry release candidate ensure 2.7.0 \
  --artifact build/app/outputs/bundle/release/app-release.aab \
  --actor ci@example.com

gantry release audit 2.7.0 \
  --reuse-unchanged \
  --actor engineer@example.com
```

`ensure` operations are safe for CI retries. Reuse occurs only when the verified artifact,
Git, policy, report fingerprint, and authority-tool identity still match.

### Step 5: Inspect what needs work

```bash
gantry release show 2.7.0 --json
gantry release portfolio
```

Use each finding's `finding_instance_id`, not only its general `rule_id`, when recording
evidence, a resolution, or accepted risk.

## 6. Resolve findings and attach evidence

Attach candidate-bound evidence to an exact active-audit finding:

```bash
gantry release evidence add 2.7.0 \
  --finding MOB-EVD-001:release_owner_approval \
  --kind release_owner_approval \
  --title "Release owner approval" \
  --issuer release@example.com \
  --actor release@example.com
```

Record accountable workflow resolution:

```bash
gantry release finding resolve 2.7.0 \
  MOB-CFG-006:android_target_sdk \
  --reason "Target SDK updated and candidate rebuilt" \
  --actor engineer@example.com
```

Then build/bind a corrected candidate when required and rerun the audit. Resolution text
alone never changes an automated finding.

## 7. Release-manager workflow

### Portfolio

Open **Release Portfolio** first. It answers:

- Which releases are overdue or due soon?
- Which exact candidates are blocked or rejected?
- Which candidates are waiting for human approval?
- Which approvals still carry risk?
- Which evidence or waivers are expiring?
- What is the next verified action and who owns it?

Use list view for triage and calendar view for schedule pressure. Search and queue filters
change presentation only; they do not alter release authority.

### Candidate review

Open a release and verify, in order:

1. Release owner and target/submission/rollout dates.
2. Active platform candidate ID and SHA-256.
3. Active audit ID, automated decision, policy digest, and report fingerprint.
4. New or changed findings since the previous candidate.
5. Evidence, resolutions, and active/expiring waivers.
6. The exact accountable identity, role, and rationale for the decision.

### Approve or reject

```bash
gantry release approve 2.7.0 \
  --by release@example.com \
  --role release_manager \
  --rationale "Candidate, rollback evidence, and rollout plan reviewed"
```

```bash
gantry release reject 2.7.0 \
  --by release@example.com \
  --role release_manager \
  --rationale "Production smoke evidence is incomplete"
```

Gantry refuses approval of a No-Go audit. In the desktop application, at-risk approval
requires an explicit acknowledgement before submission.

### Accept time-bounded risk

```bash
gantry release waive 2.7.0 \
  --finding MOB-CFG-006:android_target_sdk \
  --reason "Store-approved transition window" \
  --approved-by release@example.com \
  --expires-at 2026-08-01T00:00:00Z
```

Waivers remain visible after expiry and are invalidated from active context when the
candidate or audit changes.

### Schedule and packet

```bash
gantry release schedule 2.7.0 \
  --submission-date 2026-07-18 \
  --target-date 2026-07-20 \
  --rollout-date 2026-07-20 \
  --actor release@example.com

gantry release packet 2.7.0
```

Packet export writes verified JSON, Markdown, and standalone HTML. Treat the JSON packet
as the canonical machine record and the Markdown/HTML files as derived human renderings.

## 8. CI and GitHub configuration

Build the signed candidate before invoking Gantry:

```yaml
- name: Gantry mobile release gate
  uses: trygantry/gantry@v1
  with:
    mobile-artifact: build/app/outputs/bundle/release/app-release.aab
    release-version: 2.7.0
    release-name: July production release
    release-owner: release@example.com
    fail-on-no-go: true
```

Configure the job as a required check.

The desktop **CI & Integrations** screen contains the candidate-bound Action contract and a
portable CLI sequence. The Action example supplies `mobile-artifact`, `release-version`,
`release-name`, `release-owner`, and `fail-on-no-go`; it does not present the legacy general
`gantry validate` command as the mobile release workflow.

Retain `.gantry/releases/` through Git or a trusted artifact restore when durable history
must survive clean runners.

A remote cache is never treated as approval authority.

Required authority tools depend on the candidate:

- IPA: `codesign` on macOS.
- APK: `apksigner`, with `jarsigner` only for supported JAR/v1 verification.
- AAB: `jarsigner`; `bundletool` can enrich metadata.

Missing authority tools produce `Unavailable`, not `Pass`.

## 9. Policy administration

The mobile policy is `.gantry/mobile-policy.json`.

Review it in source control like code.

Important controls include:

- minimum supported platform requirements;
- required manual evidence kinds;
- whether warnings, manual items, or unavailable checks block;
- trusted evidence issuers;
- approved signing identities or certificate fingerprints;
- production-placeholder scanning and scoped exceptions.

Use a scoped placeholder annotation only with a reason:

```text
gantry: allow-placeholder(reason=sample fixture used only in tests)
```

Do not weaken policy merely to turn a red result green.

Record a reviewed policy change or a time-bounded waiver with the actual rationale.

## 10. Vault health, recovery, and upgrades

### Verify the vault

```bash
gantry vault check
gantry vault check --json
```

The check verifies every durable release, its event chain, stored audit reports, active
bindings, and pending transaction journals.

In the desktop application, open **Settings → Vault Health & Recovery**. The same core
operations are available as **Check vault**, **Repair transactions**, **Preview
migration**, and **Create support bundle**. Results remain visible in an announced status
region so administrators can copy the actionable outcome without opening a terminal.

### Recover an interrupted commit

```bash
gantry vault check --repair
```

Release mutations are staged in a checksummed transaction journal and published under a
cross-process lock. Repair only rolls a complete, checksummed journal forward. It does not
invent or discard governance records.

### Check migrations before upgrading

```bash
gantry vault migrate --dry-run
gantry vault migrate
```

Version 0.4 writes workflow release schema 1.0, so a current vault produces a verified
no-op migration result. Unknown versions fail closed until a reviewed migration is
registered. Future state-changing migrations must create a timestamped backup first.

### Create a redacted support bundle

```bash
gantry vault support-bundle
gantry vault support-bundle --output /tmp/gantry-support.json
```

The JSON bundle includes version, OS/architecture, vault health, stage counts, policy
presence, and authority-tool availability. It excludes source/file names, artifact
identities and hashes, credentials, evidence payloads, and actor identities. Review the
file before sharing it.

## 11. Storage and transaction safety

Durable workflow releases live under `.gantry/releases/REL-*/`. Each event contains the
previous event hash, sequence, details, and its own SHA-256 hash. `release.json` records
the expected event head.

Every multi-file mutation now:

1. Captures the expected event head.
2. Stages checksummed files in `.gantry/transactions/TXN-*/`.
3. Acquires the vault commit lock.
4. Rejects the operation if another writer changed the event head.
5. Publishes record files and events before the release head.
6. Synchronizes files and their parent directories.
7. Removes the journal only after the complete write is durable.

Never edit durable JSON or `events.jsonl` by hand. Use the CLI or desktop application.

## 12. Security and privacy boundaries

Gantry is local-first and does not upload source or artifacts as part of the core audit.
The desktop CSP limits content and IPC origins, archive reads are bounded, unsafe IPA paths
and symlinks are rejected, and candidate hashing is streamed.

Local records do not independently authenticate a declared human identity. For strong
identity and enforcement, combine Gantry with protected branches, signed commits, required
CI checks, code review, and an external identity/approval system.

Gantry verifies artifact signatures when an authority tool is available. It does not claim
App Store Connect or Google Play acceptance, compliance approval, or rollout state without
connected-provider evidence.

## 13. Performance and cancellation

The GitHub Action reports wall-clock duration, cache status, and whether it stayed within
the ten-minute onboarding budget. Large artifact qualification must use representative
production candidates before release.

Desktop audits run on a blocking worker rather than the UI thread. While an audit is
running, **Cancel check** requests cooperative cancellation. Gantry checks cancellation
between source, artifact, evidence, and decision phases and during streamed artifact
hashing and archive-limit scans. An already-running external authority process may finish
before cancellation is observed.

## 14. Troubleshooting

### “Signature verifier unavailable”

Install the platform authority tool on the runner, confirm it is on `PATH`, and rerun the
exact candidate audit. Do not convert unavailable verification into a pass.

### “Release changed while this operation was running”

Another process committed a newer event head. Reload the release and repeat the action.
No files from the stale operation were published.

### “Pending transaction”

Run `gantry vault check --repair`, then run `gantry vault check` again. Preserve the vault
before manual intervention and include a redacted support bundle with a bug report.

### Candidate audit became stale

The artifact path, candidate, policy, or audit context changed. Rerun the exact candidate
audit before tracking or approving it.

### Desktop cannot open the project

Launch Gantry from the repository directory or choose the repository in the desktop app.
Confirm `.gantry/` is readable and that the candidate remains inside the project boundary.

### Read-only or permission failure

Gantry should report the failing path without partially publishing a mutation. Correct
filesystem permissions, then run the vault health check before retrying.

## 15. Verifying a Gantry build

To check a build of Gantry itself:

- Run `cargo test --workspace --all-targets`.
- Run `cargo clippy --workspace --all-targets -- -D warnings`.
- Run `cargo fmt --all --check`.
- Run `npm run test:frontend`.
- Run `npm run test:ui` to exercise the rendered engineer, release-manager, administrator,
  and dialog-accessibility workflows.

## 16. Command quick reference

```text
gantry init --mobile
gantry mobile detect [--json]
gantry mobile audit [--artifact <ipa|aab|apk>] [--write] [--ci] [--json]
gantry release ensure <version> [--owner <identity>]
gantry release candidate ensure <release> --artifact <path>
gantry release audit <release> [--platform ios|android] [--reuse-unchanged]
gantry release portfolio [--json]
gantry release show <release> [--json]
gantry release schedule <release> [--target-date YYYY-MM-DD ...]
gantry release evidence add <release> --finding <instance> --kind <kind> ...
gantry release finding resolve <release> <finding> --reason <text>
gantry release waive <release> --finding <instance> --expires-at <RFC3339> ...
gantry release approve|reject <release> --by <identity> --role <role> --rationale <text>
gantry release diff <release> [--platform ios|android]
gantry release packet <release>
gantry vault check [--repair] [--json]
gantry vault migrate [--dry-run] [--json]
gantry vault support-bundle [--output <path>] [--json]
```

For every flag and exit code, see `docs/cli-reference.md`.

## 17. Documentation map

- `docs/manual.md`: user and administrator manual.
- `docs/getting-started.md`: short onboarding walkthrough.
- `docs/cli-reference.md`: command and flag reference.
- `docs/mobile-release-governance.md`: mobile policy and rule semantics.
- `docs/github-action.md`: CI and Action configuration.
- `docs/architecture.md`: contributor architecture and storage boundaries.
- `schemas/README.md`: schema compatibility and migration policy.
