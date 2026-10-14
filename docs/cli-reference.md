# Gantry CLI Reference

A complete reference for every `gantry` command, flag, and exit code.

For installation and vault setup, see [Getting Started](getting-started.md).
For CI integration, see [GitHub Action](github-action.md).

---

## Global conventions

**Exit codes** are the primary interface for CI and scripting:

| Code | Meaning |
| :--- | :--- |
| `0` | Success / Go |
| `1` | Blocked / No-Go (launch is not ready) |
| `2` | Error (vault is malformed, required argument missing, or an I/O failure occurred) |

**Vault discovery** — every command looks for `.gantry/` starting from the current working directory and walking up to the repo root. Override with `--vault-path <path>` or the `GANTRY_VAULT_PATH` environment variable.

**`--env <name>`** — most read commands accept `--env` to evaluate against a named environment policy (e.g. `production`, `staging`). Can also be set via the `GANTRY_ENV` environment variable. Defaults to `defaultEnvironment` in `config.json`, or `production` if unset.

**Environment variables**

| Variable | Equivalent flag | Description |
| :--- | :--- | :--- |
| `GANTRY_VAULT_PATH` | `--vault-path` | Absolute path to the `.gantry/` directory |
| `GANTRY_ENV` | `--env` | Named environment to evaluate against |
| `GANTRY_ACTOR` | command-specific `--actor` / `--by` | Default accountable identity for durable release events |

---

## Commands

### `gantry init`

Scaffold a `.gantry/` vault in the current project.

```
gantry init [--auto] [--template <type>] [--mobile] [--force] [--dry-run] [--json]
```

| Flag | Description |
| :--- | :--- |
| `--auto` | Detect the tech stack and choose an appropriate template automatically |
| `--template <type>` | Use a specific template: `mobile-app`, `saas-app`, `ai-app`, `backend-api`, `desktop-app`, `chrome-extension` |
| `--interactive` | Step through an interactive wizard to choose a template |
| `--mobile` | Initialize the `mobile-app` vault and `.gantry/mobile-policy.json` together |
| `--force` | Overwrite an existing vault |
| `--dry-run` | Print what would be created without writing anything |
| `--json` | Output results as JSON |

> **Note:** `--auto`, `--template`, and `--interactive` are mutually exclusive.

**Example — auto-detect and scaffold:**
```bash
$ gantry init --auto
Detected stack: Rust / Tauri
Scaffolded .gantry/ vault with template: cli
  Created: .gantry/config.json
  Created: .gantry/go_no_go.md
  Created: .gantry/AGENTS.md
  Created: .gantry/tasks.md
  Created: .gantry/blockers/
  Created: .gantry/qa/
  Created: .gantry/schema/
```

Exit codes: `0` success · `2` error

---

### `gantry mobile`

Run mobile project detection and release governance. These commands do not require an
existing vault, except that a vault is needed to persist policy, packets, and evidence.

```text
gantry mobile detect [--json]
gantry mobile init [--force] [--json]
gantry mobile rules [--json]
gantry mobile audit [--artifact <path>] [--format json|markdown|html]
                    [--output <path>] [--write] [--ci] [--json]
```

| Command / flag | Description |
| :--- | :--- |
| `detect` | Detect first-party Flutter, React Native, iOS, and Android targets |
| `init` | Create `.gantry/mobile-policy.json` and its JSON Schema; preserve an existing policy unless `--force` is used |
| `rules` | Print stable rule IDs, categories, policy-pack version, and review date |
| `audit` | Inspect source and optionally the exact IPA/AAB/APK candidate |
| `--artifact <path>` | Candidate `.ipa`, `.aab`, or `.apk` to hash and inspect |
| `--format <type>` | Render canonical JSON, Markdown, or standalone HTML |
| `--output <path>` | Write the packet at an explicit path |
| `--write` | Write a timestamped packet under `.gantry/mobile/audits/` |
| `--ci` | Emit GitHub blocker annotations and deterministic exit status |
| `--json` | Print the stable schema 2.0 `{ schema_version, report, report_path }` envelope |

Examples:

```bash
gantry mobile detect --json
gantry mobile audit
gantry mobile audit --artifact app-release.aab --ci
gantry mobile audit --artifact MyApp.ipa --format html --output mobile-release.html
```

Exit codes: `0` Go or non-blocking At Risk · `1` No-Go · `2` error

Written packets use atomic publication and collision-resistant names. Production CI can
make warnings, manual evidence, and unavailable verifiers independently blocking through
the mobile policy.

See [Mobile Release Governance](mobile-release-governance.md) for the policy, rules,
artifact limitations, and evidence-binding contract.

---

### `gantry validate`

Evaluate the vault and print a readiness verdict. The primary gate command.

```
gantry validate [--ci] [--env <name>] [--policy <url|path>] [--report-url <url>]
```

| Flag | Description |
| :--- | :--- |
| `--ci` | CI mode: structured output, deterministic exit codes, no interactive prompts |
| `--env <name>` | Evaluate against this environment's policy |
| `--policy <url\|path>` | Fetch or read a shared scorecard JSON and overlay it on the local scorecard |
| `--report-url <url>` | Attach a report URL to the output (Gantry Cloud hook) |

**Example — local check:**
```bash
$ gantry validate
Readiness State: No-Go (26/100)
Open Blockers:   1
QA Completion:   0/2 tests passed
```

**Example — CI mode:**
```bash
$ gantry validate --ci
=====================================
        Gantry CI Validation
=====================================
Vault Name:      My App 1.2.0
Release Target:  1.2.0
Readiness State: No-Go
Readiness Score: 26/100
-------------------------------------
Open Blockers:   1
QA Completion:   0/2 tests passed
Top Actionable Blockers:
  - [BLOCKER] BLK-101: Auto-updater fails on Windows (@dev_jane)
  - [GATE] UNCHECKED: Engineering: installer packages signed
  - [SIGNOFF] PENDING: @dev_jane (Engineering)
=====================================
$ echo $?
1
```

**Example — shared policy overlay:**
```bash
$ gantry validate --ci --policy https://policy.myorg.com/scorecard.json
Policy loaded from: https://policy.myorg.com/scorecard.json
# ... standard CI output ...
```

The fetched policy is cached to `.gantry/.policy_cache.json` so CI doesn't fail if the policy server is temporarily unreachable.

Exit codes: `0` Go · `1` No-Go · `2` error

---

### `gantry summarize`

Print a plain-language summary of the current vault state.

```
gantry summarize [--env <name>]
```

**Example:**
```bash
$ gantry summarize
=====================================
 Gantry Launch Control Summary
=====================================
Vault Name:      My App 1.2.0
Release Target:  1.2.0
Readiness State: No-Go
Readiness Score: 26/100
-------------------------------------
Open Blockers:   1
QA Completion:   0/2 tests passed
Sign-offs:       0/3 pending
=====================================
```

Exit codes: `0` success · `2` error

---

### `gantry index`

Rebuild `index.json`, write a timestamped snapshot, and update the release manifest. Run this after changing vault files to keep the machine-readable state current.

```
gantry index [--env <name>]
```

**Example:**
```bash
$ gantry index
Gantry index updated. Snapshot written.
```

`gantry index` skips rewriting the release manifest if the meaningful manifest content has not changed, so it is safe to run repeatedly (e.g. in a post-commit hook or CI step).

Exit codes: `0` success · `2` error

---

### `gantry audit`

Score readiness, generate a repair task list, or export an audit certificate.

```
gantry audit [--dry-run] [--json] [--format markdown] [--output <path>] [--env <name>]
gantry audit export [--format html|json|md] [--output <path>]
```

#### `gantry audit` — repair task generation

| Flag | Description |
| :--- | :--- |
| `--dry-run` | Show what tasks would be created without writing to `tasks.md` |
| `--json` | Output results as JSON |
| `--format markdown` | Output a PR-ready markdown report |
| `--output <path>` | Write output to a file instead of stdout |

**Example:**
```bash
$ gantry audit --dry-run
[DRY RUN] Would create 3 repair tasks:
  - MISSING_DOC: release-notes.md
  - STALE_DOC: blockers/BLK-101-purchase-crash.md (last verified 32 days ago)
  - UNCHECKED_GATE: QA: auto-update mechanism verified
```

#### `gantry audit export` — audit certificate

Generates a self-contained proof artifact containing launch gates, sign-offs, waivers, evidence records, readiness score, and snapshot chain verification.

| Flag | Description |
| :--- | :--- |
| `--format html\|json\|md` | Output format (default: `html`) |
| `--output <path>` | Write to this file (default: `./gantry-audit.<ext>`) |

**Example — HTML certificate for auditors:**
```bash
$ gantry audit export --format html --output ./audit-cert.html
Audit certificate written to ./audit-cert.html
```

Open the HTML file in any browser and print to PDF. The certificate is fully self-contained — no external assets required.

**Example — JSON for downstream tooling:**
```bash
$ gantry audit export --format json --output ./audit-cert.json
```

Exit codes: `0` success · `2` error

---

### `gantry verify`

Verify the tamper-evident snapshot chain. Each snapshot carries a SHA-256 `prev_hash` and `entry_hash`; this command walks the chain and confirms no snapshot has been edited after the fact.

```
gantry verify [--json]
```

**Example — intact chain:**
```bash
$ gantry verify
=====================================
     Gantry History Verification
=====================================
Snapshots checked: 12
Status:            INTACT
The snapshot history has not been tampered with.
=====================================
$ echo $?
0
```

**Example — broken chain:**
```bash
$ gantry verify
=====================================
     Gantry History Verification
=====================================
Snapshots checked: 12
Status:            BROKEN
First broken at:   2026-06-13T14-22-00Z.json
Reason:            entry_hash mismatch — content does not match recorded hash
=====================================
$ echo $?
1
```

Exit codes: `0` intact · `1` broken · `2` error

---

### `gantry install-hooks`

Install a git hook that blocks commits or pushes when readiness is No-Go. Appends a guarded block to the hook file — any existing hook content is preserved.

```
gantry install-hooks [--stage commit|push] [--force]
```

| Flag | Description |
| :--- | :--- |
| `--stage commit\|push` | Which hook to install (default: `push`) |
| `--force` | Replace the hook file entirely instead of appending |

**Example:**
```bash
$ gantry install-hooks
Gantry pre-push hook installed at .git/hooks/pre-push.
Pushes will be blocked when readiness is No-Go.
Use 'git push --no-verify' to bypass (will be visible in 'gantry verify').
```

The installed block looks like this:
```sh
# --- gantry launch gate ---
if command -v gantry >/dev/null 2>&1; then
  gantry validate --ci
  _gantry_exit=$?
  if [ "$_gantry_exit" -ne 0 ]; then
    echo "Gantry: launch gate blocked (exit $_gantry_exit). Run 'gantry audit' for details."
    exit $_gantry_exit
  fi
fi
# --- end gantry launch gate ---
```

Exit codes: `0` success · `2` error

---

### `gantry uninstall-hooks`

Remove the Gantry gate block from all git hooks. Any hook content outside the guarded block is preserved.

```
gantry uninstall-hooks
```

**Example:**
```bash
$ gantry uninstall-hooks
Removed Gantry block from .git/hooks/pre-push.
```

Exit codes: `0` success · `2` error

---

### `gantry watch`

File-watcher daemon that re-validates on every `.gantry/` save and prints a timestamped status line. Debounces bursts at 500ms.

```
gantry watch [--env <name>]
```

**Example:**
```bash
$ gantry watch
Gantry watch started. Monitoring .gantry/ for changes.
Press Ctrl+C to stop.
=====================================
[14:02:31] GO — score 87/100, 0 blocker(s), 4/4 QA
[14:03:45] AT RISK — score 72/100, 1 blocker(s), 4/4 QA
         ↳ Launch is at risk: 1 open P1 blocker remains.
[14:05:12] GO — score 91/100, 0 blocker(s), 4/4 QA
```

Useful while authoring vault files or reviewing CI output locally. Run in a second terminal alongside your editor.

Exit codes: `0` on Ctrl+C · `2` error

---

### `gantry impact`

Show which modified files map to open blockers, QA suites, and uncovered folders.

```
gantry impact [--base <ref>] [--json] [--markdown]
```

| Flag | Description |
| :--- | :--- |
| `--base <ref>` | Git ref to diff against (default: `HEAD~1`) |
| `--json` | Output as JSON |
| `--markdown` | Output a markdown table (used by the GitHub Action for PR comments) |

**Example:**
```bash
$ gantry impact --markdown
## Change Impact

| | Count |
| :--- | ---: |
| Files changed | 7 |
| Blockers affected | 1 |
| QA suites affected | 1 |
| Uncovered folders | 2 |

**Affected blockers:** BLK-101 (Auto-updater fails on Windows)
**Affected QA suites:** QA-REGRESSION (CLI Regression Suite)

**Suggested tasks:**
- Cover modified component with QA/Blocker: docs
- Cover modified component with QA/Blocker: .github/workflows
```

Exit codes: `0` success · `2` error

---

### `gantry diff`

Compare two readiness snapshots by git commit refs.

```
gantry diff <base>..<head> [--json]
```

**Example:**
```bash
$ gantry diff abc1234..def5678
=====================================
        Gantry Snapshot Diff
=====================================
Base:  abc1234 (2026-06-10, score 62/100, At Risk)
Head:  def5678 (2026-06-13, score 91/100, Go)
-------------------------------------
Readiness:     At Risk → Go
Score:         62 → 91 (+29)
Blockers:      2 → 0 (-2)
  Resolved: BLK-101, BLK-102
QA Completion: 3/8 → 8/8
  Completed: QA-REGRESSION
Sign-offs:     0/3 → 3/3
  Completed: @pm_dan, @dev_jane, @qa_bob
=====================================
```

Exit codes: `0` success · `2` error

---

### `gantry trends`

Summarize readiness score, blockers, QA completion, waivers, and evidence counts over time from local snapshots.

```
gantry trends [--json]
```

**Example:**
```bash
$ gantry trends
=====================================
        Gantry Readiness Trends
=====================================
Snapshot: 2026-06-10T10-00-00Z | Score: 62/100 | Readiness: At Risk | Blockers: 2 | QA: 37.5% | Waivers: 0 | Evidence: 1
Snapshot: 2026-06-12T09-15-00Z | Score: 78/100 | Readiness: At Risk | Blockers: 1 | QA: 75.0% | Waivers: 1 | Evidence: 2
Snapshot: 2026-06-13T14-00-00Z | Score: 91/100 | Readiness: Go      | Blockers: 0 | QA: 100.0% | Waivers: 0 | Evidence: 3
=====================================
```

Exit codes: `0` success · `2` error

---

### `gantry tasks`

List pending repair tasks generated by `gantry audit`.

```
gantry tasks [--json]
```

Exit codes: `0` success · `2` error

### `gantry task complete <id>`

Mark a repair task as complete.

```
gantry task complete <id>
gantry task ignore <id> <reason>
```

Exit codes: `0` success · `2` error

---

### `gantry waiver`

Manage time-bounded accepted-risk waivers.

#### `gantry waiver list`

```
gantry waiver list [--json]
```

#### `gantry waiver add`

```
gantry waiver add \
  --target-type <blocker|qa|gate|signoff|document|validation> \
  --reason <text> \
  --approved-by <@handle> \
  --expires-at <YYYY-MM-DD> \
  [--target-id <id>] \
  [--target-path <path>]
```

**Example:**
```bash
$ gantry waiver add \
    --target-type blocker \
    --target-id BLK-101 \
    --reason "Known edge case; hotfix ships in 1.2.1 within 48h." \
    --approved-by @release-lead \
    --expires-at 2026-06-20
Waiver WAV-20260613-001 created.
```

Waivers allow a matched finding to stop forcing No-Go, but the waived item remains visible in all reports and release manifests.

#### `gantry waiver expire <id>`

```
gantry waiver expire <id>
```

Exit codes: `0` success · `2` error

---

### `gantry evidence`

Attach structured evidence records to a release.

#### `gantry evidence add`

```
gantry evidence add \
  --kind <kind> \
  --title <text> \
  [--path <local-path>] \
  [--url <url>] \
  [--artifact-sha <sha>] \
  [--mobile-evidence-kind <requirement>] \
  [--evidence-issuer <identity>] \
  [--expires-at <rfc3339>] \
  [--env <name>] \
  [--release <version>]
```

Supported `kind` values: `test_report`, `security_scan`, `telemetry_pull`, `approval`, `artifact`, `app_store_build`, `ci_run`, `manual_note`.

Mobile governance evidence must set `--mobile-evidence-kind` to the exact configured
requirement and bind to the current commit or artifact SHA. Use `--expires-at` for
time-bounded approvals. Policies can restrict issuers through `trusted_evidence_issuers`.

**Example:**
```bash
$ gantry evidence add \
    --kind test_report \
    --title "Release regression suite — all 49 tests passed" \
    --url https://github.com/myorg/myapp/actions/runs/9876543 \
    --env production \
    --release 1.2.0
Successfully added evidence record 'EVI-A1B2C3D4'.
```

Exit codes: `0` success · `2` error

---

### `gantry completions`

```text
gantry completions <bash|zsh|fish|powershell|elvish>
```

Writes a complete shell-specific command definition to stdout. Install it using the
normal mechanism for your shell, for example:

```bash
gantry completions zsh > "${fpath[1]}/_gantry"
gantry completions bash > ~/.local/share/bash-completion/completions/gantry
gantry completions fish > ~/.config/fish/completions/gantry.fish
```

PowerShell users can source the generated script from their profile. Completion output
is generated from the live Clap command tree, so new release and mobile flags cannot
silently drift from the CLI.

---

### `gantry release`

Create and inspect durable release workflow records. Workflow releases are separate from
the generated readiness manifests retained for backward compatibility.

#### `gantry release create`

```text
gantry release create <version> [--name <label>] [--owner <identity>]
                      [--actor <identity>] [--json]
```

Creates `.gantry/releases/<release-id>/release.json` and the first entry in its
hash-chained `events.jsonl`. A vault permits one workflow release per version.

```bash
gantry release create 2.7.0 \
  --name "July production release" \
  --owner release@example.com
```

#### `gantry release ensure`

```text
gantry release ensure <version> [--name <label>] [--owner <identity>]
                      [--actor <identity>] [--json]
```

Creates the workflow release once or returns the existing release after full integrity
verification. Retry metadata never overwrites the existing name, owner, or history. This
is the idempotent CI counterpart to `release create`.

#### `gantry release list`

```text
gantry release list [--json]
```

Lists workflow releases by most recently updated. Legacy manifest JSON files are not
included.

#### `gantry release schedule`

```text
gantry release schedule <REL-id|version>
  [--target-date <YYYY-MM-DD>]
  [--submission-date <YYYY-MM-DD>]
  [--rollout-date <YYYY-MM-DD>]
  [--actor <identity>] [--json]
```

Records at least one operating date in the verified release and appends a
`release_scheduled` event. Later schedule changes retain the history, and out-of-band
edits to the current dates fail release verification.

#### `gantry release portfolio`

```text
gantry release portfolio [--json]
```

Verifies every workflow release and derives one prioritized operating view with owner,
stage, age, active platforms, audit counts, automated decision, active decisions,
target urgency, accepted risk, expiring evidence, expiring waivers, and the next best action. Its review queue distinguishes
missing owners/candidates/audits, automated blockers, rejected candidates, pending
approvals, overdue or seven-day release targets, accepted risk, and current
candidate/audit evidence or waivers expiring within seven days. Already-expired evidence
is elevated to a blocker.

JSON conforms to `schemas/release-portfolio.schema.json`. This is the machine interface
used by the desktop cockpit; agents and CI receive the same core-owned projection.

#### `gantry release show`

```text
gantry release show <REL-id|version> [--json]
```

Loads the release, candidates, audits, evidence links, resolutions, decisions, waivers,
and event history. Human output labels records active, stale, or expired. The command
rejects the record if event sequence, hashes, release head, payload digests, report
fingerprints, or candidate/audit/policy bindings do not verify.

#### `gantry release candidate add`

```text
gantry release candidate add <REL-id|version> --artifact <app.ipa|app.aab|app.apk>
                             [--platform ios|android] [--actor <identity>] [--json]
```

Streams the candidate through SHA-256, records byte size and current Git commit, infers
platform from the file type, and makes it the active candidate for that platform. The
artifact itself is not copied into `.gantry/`; the immutable identity is its recorded
hash. Repository-local artifact paths are stored relative to the project root.

Set `GANTRY_ACTOR` to provide the event actor without repeating `--actor`.

`gantry release candidate ensure` accepts the same arguments. It returns the active
candidate without adding an event when artifact kind, SHA-256, byte size, platform, and
Git commit are unchanged; otherwise it binds a normal replacement candidate.

#### `gantry release audit`

```text
gantry release audit <REL-id|version> [--platform ios|android]
                     [--actor <identity>] [--reuse-unchanged] [--ci] [--json]
```

Runs the existing schema-2 mobile engine against the active candidate and always saves
the canonical JSON report under the workflow release. `--platform` is optional when the
release has only one active platform candidate and required when it has both.

The command verifies the current artifact hash, byte size, platform, and candidate-bound
Git commit before recording the audit. It exits `1` for No-Go and `0` for Go or permitted
At Risk. Opening the release re-verifies the report fingerprint and its candidate,
policy, decision, counts, and Git binding. Binding a replacement candidate retains the
old audit as history but removes it from the active decision.

`--reuse-unchanged` returns the verified active audit without adding another audit/event
when candidate ID, report fingerprint, and policy digest match the newly evaluated
inputs. `--ci` emits GitHub annotations keyed by stable finding instance and rule IDs,
with remediation and the first known repository location.

Eligible `--reuse-unchanged` audits use the disposable cache under
`.gantry/cache/mobile/<cache-key>.json`. The key covers Git commit, artifact SHA/size/type,
Gantry/report versions, policy digest, and installed platform-tool executable identity.
Dirty source trees bypass caching. Cache corruption is a miss, never a trusted result.

#### `gantry mobile cache-key`

```text
gantry mobile cache-key --artifact <app.ipa|app.aab|app.apk> [--json]
```

Prints the exact derived-cache identity and path without running the audit. CI systems use
this to restore the correct cache entry. JSON includes `eligible`, `cache_key`, and
`cache_path`; ineligible dirty or non-Git workspaces return null identity values.

#### `gantry release evidence add`

```text
gantry release evidence add <release> [--platform ios|android]
  --finding <finding-instance-id>
  --kind <mobile-evidence-kind>
  --title <title>
  --issuer <identity>
  [--path <path>] [--url <url>] [--expires-at <rfc3339>]
  [--actor <identity>] [--json]
```

Creates a normal `.gantry/evidence/EVI-*.json` record bound to the active candidate SHA
and Git commit, then records a release-local evidence link to the exact source audit and
finding. The evidence kind must match that finding's mobile evidence rule. The link also
stores a digest of the evidence payload, so later edits fail release verification.

Adding evidence does not mutate the existing decision. Run `gantry release audit` again
to let the deterministic engine accept or reject it.

#### `gantry release finding resolve`

```text
gantry release finding resolve <release> <finding-instance-id>
  [--platform ios|android]
  --reason <reason>
  [--evidence <EVI-id>]...
  [--actor <identity>] [--json]
```

Records accountable workflow state for a non-passing finding. Every referenced evidence
ID must already be linked to the active candidate. A resolution never overrides the
audit result; it records what a person addressed while the next audit remains the source
of truth. Evidence links and resolutions become stale when their candidate is replaced.

#### `gantry release approve` and `gantry release reject`

```text
gantry release approve <release> [--platform ios|android]
  --by <identity> --role <role> --rationale <text> [--json]

gantry release reject <release> [--platform ios|android]
  --by <identity> --role <role> --rationale <text> [--json]
```

Both commands bind the accountable record to the exact active candidate, audit, report
fingerprint, and policy digest. `GANTRY_ACTOR` can supply `--by`. Approval fails closed
when the deterministic audit is No-Go. A release with both platforms becomes Approved
only after every active candidate has a current approval; any current rejection keeps
the release Rejected. A new audit or candidate makes the affected platform decision
stale automatically.

#### `gantry release waive`

```text
gantry release waive <release> --finding <finding-instance-id>
  [--platform ios|android]
  --reason <text> --approved-by <identity> --expires-at <rfc3339> [--json]
```

Records explicit, time-bounded accepted risk for a non-passing finding in the active
audit. The expiry must be in the future; after it passes, `release show` labels the
waiver expired and a replacement waiver can be recorded. Waivers are context-active
only for their candidate and audit, become stale on re-audit or replacement, and never
turn a finding into Pass or override a No-Go decision.

#### `gantry release packet`

```text
gantry release packet <release> [--json]
```

Loads and verifies the complete persistent release, then writes the same canonical packet
as `.gantry/releases/<REL-id>/packets/<PKT-id>.json`, `.md`, and `.html`. The packet
contains active-platform summaries, all immutable candidates and audit reports, evidence,
resolutions, decisions, waivers, and the verified event history. Its timestamp comes from
the release state and its ID comes from a SHA-256 digest; unchanged state therefore
regenerates the same packet and paths. Markdown/HTML begin with a release-manager handoff
summary covering status, owner, exact candidate identities, pending decisions, open
findings, accepted risk, and the next action. `--json` returns the packet, all three paths,
and the same handoff summary.

#### `gantry release diff`

```text
gantry release diff <release> [--platform ios|android]
  [--from <CAN-id>] [--to <CAN-id>] [--json]
```

Compares two candidates on one platform. By default, `--to` is the active candidate and
`--from` is its prior same-platform candidate. Output includes artifact size and SHA,
Git, latest audit decision, policy, introduced/resolved/changed findings, and related
verified events. If either candidate lacks an audit, audit-dependent finding changes are
left empty rather than inferred.

#### Legacy generated manifests

`gantry release manifest` and `gantry release history` continue to inspect the generated
readiness manifests written by `gantry index`:

```
gantry release manifest [--json]
gantry release history [--json]
```

Exit codes: `0` success · `2` error

---

### `gantry vault`

Verify, recover, migrate, and diagnose durable workflow records.

```text
gantry vault check [--repair] [--json]
gantry vault migrate [--dry-run] [--json]
gantry vault support-bundle [--output <path>] [--json]
```

#### `gantry vault check`

Verifies every workflow release, event chain/head, schedule event, stored audit report,
candidate/audit/evidence/decision/waiver binding, and pending transaction count. Exit `0`
means healthy, `1` means operator attention is required, and `2` means the check itself
could not complete.

`--repair` acquires the release commit lock and rolls each complete checksummed interrupted
transaction forward before verification. It never invents a missing record or silently
removes a published governance event.

#### `gantry vault migrate`

Verifies the source vault and runs registered schema migrations. `--dry-run` returns the
same migration plan without publication. Gantry 0.4 writes workflow release schema 1.0,
so a compatible vault returns a verified no-op. Unknown versions fail closed until a
reviewed migration is shipped.

#### `gantry vault support-bundle`

Writes a redacted JSON diagnostic under `.gantry/support/` unless `--output` is supplied.
It contains Gantry/OS/architecture versions, vault health, release stage counts, policy
presence, and platform-authority tool availability. It excludes source/file names,
artifact paths/hashes/identities, credentials, evidence payloads, environment values, and
actor identities. Review the output before sharing it.

---

### `gantry notify`

Build and send a Slack digest for the latest snapshot state transition.

```
gantry notify [--dry-run]
```

Requires `notifications.slack` configured in `config.json` and the env var named by `webhookEnv` to be set. `--dry-run` prints the payload without sending.

Exit codes: `0` success · `2` error

---

### `gantry telemetry pull`

Pull Sentry crash-free session stats into `.gantry/observed.json` for scoring.

```
gantry telemetry pull [--env <name>]
```

Requires `sentry` configured in `config.json` and `GANTRY_SENTRY_TOKEN` set.

Exit codes: `0` success · `2` error

---

### `gantry mcp`

Launch the Gantry stdio MCP server for AI agent read access to the vault.

```
gantry mcp
```

Speaks the Model Context Protocol over stdin/stdout. Add to your agent's MCP config to give it structured, read-only access to launch state without granting write access to the `.gantry/` filesystem directly.

Exit codes: `0` on clean shutdown · `2` error

---

## Scripting and CI patterns

**Block a deploy job:**
```yaml
- name: Gantry launch gate
  run: gantry validate --ci
```

**Gate on a shared org policy:**
```yaml
- name: Gantry launch gate
  run: gantry validate --ci --policy ${{ vars.GANTRY_POLICY_URL }}
```

**Snapshot after every merge:**
```yaml
- name: Update Gantry index
  run: gantry index
```

**Verify history hasn't been tampered with:**
```yaml
- name: Verify snapshot chain
  run: gantry verify
```

**Post impact summary on PRs** — use the first-party [GitHub Action](github-action.md) instead of calling `gantry impact --markdown` directly; it handles PR comment posting and permissions automatically.
