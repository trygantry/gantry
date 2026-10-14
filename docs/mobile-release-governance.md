# Mobile Release Governance

Gantry 0.5 adds a local-first mobile release control plane for Flutter, native iOS,
and native Android projects. It turns first-party source metadata, the exact candidate
artifact, policy provenance, and human evidence into one deterministic **Go / At Risk /
No-Go** decision.

This is an implementation guide to the mobile policy, evidence, and decision model.

## Intended product and UX

The ideal workflow has three surfaces over the same report contract:

1. **CLI and GitHub Action — enforcement.** A developer runs a source audit at any time.
   CI reruns it against the exact IPA, AAB, or APK candidate, attaches a release packet,
   and blocks only deterministic No-Go findings.
2. **Desktop app — local release room.** The Mobile Release view shows the candidate,
   decision, counts, and findings in remediation order. It can initialize policy, run a
   source-only audit, accept a project-relative artifact path, and save the packet.
3. **Hosted website — later team authority.** The future website should reconcile App
   Store Connect, Google Play, RevenueCat, Sentry, CI, and authenticated approvals across
   apps. It should consume the same versioned report rather than introduce a second
   readiness engine.

The installed desktop app is therefore part of the product now; the hosted website is
not required for the open-source audit path. A mobile companion app is not planned:
release owners need a dense desktop/web review surface, while phone notifications can
deep-link into the hosted release room later.

### Ideal release journey

```text
Open repository
  → Gantry detects Flutter / iOS / Android targets
  → Review versioned mobile policy
  → Run source audit while coding
  → Build the signed release candidate
  → Audit the exact IPA / AAB / APK
  → Resolve deterministic blockers
  → Attach store, smoke-test, rollback, and owner evidence
  → CI publishes the artifact-bound packet
  → Release owner approves or records explicit risk
```

The UX deliberately distinguishes:

- **Fail:** a deterministic blocker, such as target SDK below policy, an explicitly
  debuggable app, invalid iOS privacy manifest, unidentified artifact, or missing
  or cryptographically invalid signature.
- **Warning:** a credible risk that needs attention but is not safe to classify as a
  universal blocker.
- **Manual:** evidence a human or connected provider must attest.
- **Unavailable:** a value the portable inspector cannot decode safely. Gantry names the
  platform tool required instead of guessing.
- **Pass:** a directly observed condition, with location and policy provenance.

## Quick start

Initialize the normal mobile vault and mobile policy together:

```bash
gantry init --mobile
```

Create a persistent workflow release and bind the exact candidate:

```bash
gantry release create 2.7.0 --owner release@example.com
gantry release candidate add 2.7.0 \
  --artifact build/app/outputs/bundle/release/app-release.aab
gantry release audit 2.7.0
gantry release show 2.7.0
```

These records live under `.gantry/releases/<REL-id>/` with immutable candidate metadata
and a verified hash-chained event history. They coexist with the generated version
manifests written by `gantry index`. A persistent audit saves the canonical schema-2
report and re-verifies it whenever the release is opened. Evidence actions, finding
resolutions, and accountable approvals are recorded separately so human workflow never
silently overrides deterministic policy. Approval/rejection/waiver events and packet
generation are the remaining Stage 1 delivery groups.

Attach evidence from an exact active-audit finding, then rerun the audit:

```bash
gantry release evidence add 2.7.0 \
  --finding "$FINDING_INSTANCE_ID" \
  --kind release_owner_approval \
  --title "Production candidate approved" \
  --issuer release@example.com

gantry release finding resolve 2.7.0 "$FINDING_INSTANCE_ID" \
  --reason "Release captain reviewed the exact candidate" \
  --evidence "$EVIDENCE_ID"

gantry release audit 2.7.0
```

Use the actual `finding_instance_id` and evidence ID printed by the preceding commands.
Resolution is workflow state only and cannot override a deterministic No-Go.

Or add mobile policy to an existing Gantry repository:

```bash
gantry mobile init
```

Detect the first-party project boundary before a vault exists:

```bash
gantry mobile detect
gantry mobile detect --json
```

Run a source-only audit during development:

```bash
gantry mobile audit
```

Audit the exact release candidate and write portable packets:

```bash
gantry mobile audit \
  --artifact build/app/outputs/bundle/release/app-release.aab \
  --format markdown \
  --output mobile-release.md

gantry mobile audit \
  --artifact build/ios/ipa/MyApp.ipa \
  --write
```

Use deterministic CI mode:

```bash
gantry mobile audit --artifact "$RELEASE_ARTIFACT" --ci
```

Exit codes are `0` for Go or permitted At Risk results, `1` for No-Go, and `2` for a
command, policy, decoding, or I/O error. Policy can independently enable
`fail_on_warnings`, `fail_on_manual`, and `fail_on_unavailable`. The generated policy
enables `fail_on_unavailable` by default so a missing platform verifier cannot silently
permit a candidate release.

## Policy and report contracts

`gantry mobile init` creates `.gantry/mobile-policy.json`. The built-in pack is versioned
as `2026.07` and records a `reviewed_at` date. The initial Android defaults are:

- General apps: target SDK 35 or newer.
- Wear apps: target SDK 34 or newer.

Store rules change. Updating a minimum or authority URL requires a policy-pack version
and review-date change, plus a test fixture.

Every schema `2.0` audit report contains:

- schema/tool version and UTC generation time;
- relative repository root, Git commit, branch, and dirty-worktree state;
- effective-policy SHA-256 and report fingerprint;
- explicit decision reasons, including status classes elevated to No-Go by policy;
- complete effective mobile policy;
- detected stacks, platforms, iOS targets, and Android application modules;
- artifact type, SHA-256, byte size, identity, version/build, signature material and
  cryptographic verification, signer fingerprints, embedded components, permissions,
  SDK metadata where decodable, and inspector limitations;
- stable rule IDs plus target-specific finding instance IDs, category, severity, status,
  observed and expected values,
  locations, rationale, remediation, authority URL, and authority review date;
- summary counts and the final decision.

Default written packets live under `.gantry/mobile/audits/` and are published atomically.
JSON is the canonical machine contract; Markdown and semantic standalone HTML are
portable reviewer formats. The policy and report schemas are published under `schemas/`.

## Rules in the 2026.07 pack

| ID | Area | What is checked |
| :--- | :--- | :--- |
| `MOB-PRJ-001` | Project | Supported first-party mobile project detected |
| `MOB-PRJ-002` | Project | Source scan coverage and limitations |
| `MOB-AND-001` | Android policy | Source target SDK meets policy |
| `MOB-AND-002` | Security | App is not explicitly debuggable |
| `MOB-AND-003` | Security | Cleartext production traffic is not explicitly enabled |
| `MOB-AND-004` | Identity | Stable Android application ID |
| `MOB-AND-005` | Signing | Non-debug release signing path is reviewed |
| `MOB-AND-006` | Privacy | Sensitive permissions receive product/store review |
| `MOB-IOS-001` | Identity | iOS bundle identity present |
| `MOB-IOS-002` | Versioning | Marketing version and build present |
| `MOB-IOS-003` | Privacy | Privacy manifest validity and applicability |
| `MOB-IOS-004` | Privacy | Protected-resource purpose strings reviewed |
| `MOB-CFG-001` | Configuration | No configured production placeholder patterns |
| `MOB-STR-001` | Store metadata | Required public privacy/support URLs use HTTPS or have review evidence |
| `MOB-ART-001` | Artifact | Exact candidate inspected and SHA-256-bound |
| `MOB-ART-002` | Identity | Packaged application identity resolved |
| `MOB-ART-003` | Versioning | Packaged version/build resolved |
| `MOB-ART-004` | Signing | Platform signature cryptographically verified and approved when configured |
| `MOB-ART-005` | Android policy | Packaged target SDK meets policy |
| `MOB-ART-006` | Privacy | iOS privacy manifest is packaged when applicable |

List the machine-readable catalog with `gantry mobile rules --json`.

## Evidence binding

Mobile audits read existing `.gantry/evidence/*.json` records. Required evidence passes
only when an exact `metadata.mobile_evidence_kind` identifies the configured requirement,
the timestamps are valid and current, and the record is bound to a known current Git
commit or exact artifact SHA-256. Invalid records are rejected visibly through
`MOB-EVD-INVALID`; titles, substrings, and the `unknown` commit sentinel are never trusted.

The default manual evidence kinds are:

- `store_listing_review`
- `production_smoke_test`
- `release_owner_approval`
- `rollback_plan`

Example:

```bash
gantry evidence add \
  --kind approval \
  --title "Release owner approval" \
  --mobile-evidence-kind release_owner_approval \
  --evidence-issuer release-captain@example.com \
  --artifact-sha "$ARTIFACT_SHA" \
  --expires-at 2026-08-01T00:00:00Z
```

## GitHub Action

The first-party composite Action accepts two new inputs:

```yaml
- uses: trygantry/gantry@v1
  with:
    workspace: "."
    mobile-audit: true
    mobile-artifact: build/app/outputs/bundle/release/app-release.aab
    fail-on-no-go: true
    comment: true
```

The root `action.yml` is the only canonical Action. It runs the mobile audit once in CI
mode, appends that exact Markdown packet to the sticky PR comment, and retains the same
exit status for enforcement. Generate the artifact in an earlier step and point
`mobile-artifact` at that exact file.

## Honest limitations and external launch blockers

The portable inspector is release-useful but does not overstate platform authority:

- IPA inspection safely extracts the root app and invokes `codesign` when available.
  Profile/entitlement policy, notarization, and App Store acceptance still require the
  actual Apple export workflow or a connected provider.
- APK/AAB inspection invokes `apksigner` or `jarsigner` when available and can enforce
  approved signer identities. Play App Signing state remains connected-provider evidence.
- AAB manifests use protobuf encoding. Gantry extracts safe string-pool data and reports
  numeric fields unavailable when official `bundletool` evidence is required.
- The rule pack does not guarantee store approval, privacy-law compliance, or security.
- App Store Connect, Google Play, RevenueCat, Sentry symbol status, authenticated
  approvals, hosted policy signing, RBAC, and independent retention are not yet managed
  integrations.

These are explicit limitations, not hidden placeholders in the current decision.
