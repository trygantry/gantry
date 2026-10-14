# Gantry Launch Gate — GitHub Action

Gate your launches in CI. The action runs `gantry validate` against the `.gantry/` vault
in your repository and fails the job when the launch is not ready. It also posts a sticky
audit report as a PR comment on every pull request.

The repository-root `action.yml` is the single canonical Action implementation. In
durable release mode the mobile audit executes once; the verified packet posted for
review and uploaded as a workflow artifact is assembled from that exact persisted audit.

## Usage

```yaml
# .github/workflows/launch-gate.yml
name: Launch Gate
on: [pull_request]

jobs:
  gantry:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write   # required to post PR comments
    steps:
      - uses: actions/checkout@v5
      - uses: trygantry/gantry@v1
        with:
          workspace: "."
          fail-on-no-go: true
          comment: true
```

The action generates the same local readiness report as the CLI: readiness state,
readiness score, top blocking causes, scorecard rule results, evidence paths, and active
waivers. Active waivers are shown as accepted risk; they can prevent a matched finding
from forcing **No-Go**, but they are never hidden from the report.

The job exits:

- **0** — readiness is **Go** (all checks pass)
- **1** — launch is **blocked** (open P0 blockers, incomplete required QA, missing
  sign-offs, stale or invalid required docs, failed scorecard rules, or missing required evidence)
- **2** — configuration or environment error

## Inputs

| Input | Default | Description |
| :--- | :--- | :--- |
| `cli-version` | Action version | Install a specific published Gantry CLI release; useful only for release bootstrapping. |
| `workspace` | `.` | Directory containing the `.gantry/` vault. |
| `token` | `github.token` | GitHub token for posting PR comments. |
| `fail-on-no-go` | `true` | Exit 1 when launch is blocked. Set `false` for reporting only. |
| `comment` | `true` | Post or update a sticky PR comment with the audit report. |
| `environment` | `""` | Optional environment policy to evaluate (for example `production` or `staging`). |
| `policy` | `""` | Optional path or URL for a shared general scorecard policy overlay. |
| `mobile-audit` | `false` | Run the mobile source/artifact audit and append it to the PR report. |
| `mobile-artifact` | `""` | Optional project-relative path to the exact IPA, AAB, or APK candidate. |
| `release-version` | `""` | Enable the durable workflow for this version/label. Requires `mobile-artifact`. |
| `release-name` | `""` | Name used only when the durable release is first created. |
| `release-owner` | `""` | Accountable owner used only when the durable release is first created. |
| `release-actor` | `""` | Candidate/audit event actor; defaults to `github-actions:<github.actor>`. |
| `packet-artifact-name` | `gantry-release-packet` | Name of the uploaded verified packet artifact. |

## Outputs

Every mode exposes `duration-ms` and `performance-budget-met`. These measure the complete
composite Action and report whether it stayed within Gantry's ten-minute onboarding
budget. Durable mode additionally exposes `release-id`, `candidate-id`, `audit-id`, `decision`,
`report-fingerprint`, `cache-hit`, `cache-key`, `packet-id`, and the
`packet-json-path`, `packet-markdown-path`, and `packet-html-path` values. These are
suitable for deployment summaries and later jobs; they are identities and verified
local paths, not approval claims.

## Mobile release gate

Build the signed candidate first, then point Gantry at that exact file:

```yaml
- name: Build Android release bundle
  run: flutter build appbundle --release

- uses: trygantry/gantry@v1
  with:
    workspace: "."
    mobile-audit: true
    mobile-artifact: build/app/outputs/bundle/release/app-release.aab
    release-version: 2.7.0
    release-name: July production release
    release-owner: release@example.com
    fail-on-no-go: true
    comment: true
```

With `release-version`, the action verifies or creates the release, SHA-binds or reuses
the exact candidate, persists or reuses the deterministic audit, writes the
content-addressed packet, and uploads its JSON, Markdown, and HTML renderings. The
packet upload runs even when the gate fails, preserving the failure evidence.

Retries are idempotent inside the restored workspace: Gantry reuses a candidate only
when artifact kind, SHA-256, byte size, platform, and Git commit match, and reuses an
audit only when candidate, policy digest, and report fingerprint match. Changed inputs
create normal immutable events and invalidate stale decisions using the core rules.

Every non-passing finding emits a GitHub annotation whose title contains its stable
`finding_instance_id` and rule ID. Blockers are errors; warnings, manual evidence, and
unavailable checks are warnings. The annotation message includes the remediation and,
when available, the first repository location.

Without `release-version`, `mobile-audit: true` retains the stateless source/artifact
mode. In either mode the gate fails for deterministic mobile No-Go findings; At Risk
behavior follows `fail_on_warnings`, `fail_on_manual`, and `fail_on_unavailable` in
`.gantry/mobile-policy.json`.

The Action restores and saves only `.gantry/cache/mobile/`, using the exact cache
identity reported by the CLI. That identity covers Git commit, artifact SHA/size/type,
Gantry and report versions, policy digest, and the installed platform-tool executables.
Dirty source inputs are ineligible. Every cache entry re-verifies its identity and report
fingerprint; corrupt or mismatched entries are ignored and regenerated.

The Action does not treat GitHub cache data as authoritative governance state.
For history across fresh jobs, keep `.gantry/releases/` in the repository or restore it
from a trusted workflow artifact before invoking Gantry. Every restored record is fully
verified before reuse. The uploaded packet is evidence output; it is not automatically
merged back into the next checkout.

Install the platform authority tools on the runner before enabling strict unavailable
gating: `apksigner`/`jarsigner` and optionally `bundletool`/`apkanalyzer` for Android;
`codesign` on a macOS runner for IPA verification. Without a verifier, Gantry reports
signature verification as `Unavailable`, never `Pass`.

## Example: validate a vault in a subdirectory

```yaml
- uses: actions/checkout@v5
- uses: trygantry/gantry@v1
  with:
    workspace: apps/backend
    fail-on-no-go: true
    comment: true
```

## Example: report only, never fail the build

```yaml
- uses: trygantry/gantry@v1
  with:
    fail-on-no-go: false
    comment: true
```

## Performance note

The Gantry CLI (`gantry-cli`) is a standalone, GUI-free binary. The Action downloads the
prebuilt binary matching its own published Gantry version and verifies the release
checksum before use. Consumer repositories do not install Rust or GUI system
dependencies (`libwebkit2gtk`, GTK, etc.).

Every run publishes its total wall-clock duration, cache state, and ten-minute budget
result in the GitHub job summary. See [Mobile CI performance](mobile-ci-performance.md)
for the cold/warm measurement contract and an optional enforcement example.

Each tagged release publishes raw CLI binaries for the installers, normalized tar/ZIP
archives for package managers, per-asset checksum files, and one aggregate
`SHA256SUMS` manifest.
