# Mobile CI performance contract

Status date: 2026-07-16

Gantry's engineer-onboarding budget is ten minutes from the start of the composite
GitHub Action to its final governance output. Performance evidence is part of the
release contract; a fast local scanner alone is not enough if installation, compilation,
cache restoration, artifact verification, or packet upload makes CI impractical.

## Published measurements

The root Action exposes two outputs on every run, including failed gates:

- `duration-ms`: wall-clock time from the first composite step through cache save and
  packet upload;
- `performance-budget-met`: `true` when that duration is at most 600,000 ms.

The Action also writes duration, mobile-cache state, and the budget result to the GitHub
job summary. Durable mobile runs continue to expose `cache-hit` and `cache-key`, allowing
cold and warm observations to be separated without trusting timing heuristics.

```yaml
- id: gantry
  uses: trygantry/gantry@v1
  with:
    mobile-artifact: build/MyApp.ipa
    release-version: 2.7.0

- name: Enforce the onboarding performance budget
  if: ${{ always() }}
  env:
    DURATION_MS: ${{ steps.gantry.outputs.duration-ms }}
    BUDGET_MET: ${{ steps.gantry.outputs.performance-budget-met }}
    CACHE_HIT: ${{ steps.gantry.outputs.cache-hit }}
  run: test "$BUDGET_MET" = true
```

Teams may observe the budget before enforcing it. Gantry reports the measurement but
does not fail a release solely for exceeding the performance budget.

## iOS contract measurement

The macOS `mobile-ios-contract` job builds an ad-hoc signed bundle-shaped IPA, verifies
it with Apple's `codesign`, runs the durable audit twice, and uploads
`build/performance.json`. That record contains cold and warm audit durations, the warm
cache result, runner OS, and budget status. The same job rejects a warm run that is not
a cache hit or changes the audit ID, report fingerprint, or policy digest.

Local development evidence on 2026-07-16 was 471 ms cold and 209 ms warm for the signed
iOS audit portion. This proves the cache path and authority verification are useful, but
it excludes hosted-runner provisioning, Rust compilation, network actions, and artifact
upload. It is therefore not presented as the final Action SLA result.

## Release evidence still required

Before tagging the release, run the canonical Action on GitHub and retain at least one
cold and one warm job summary. Both must remain under 600,000 ms. If a run exceeds the
budget, inspect the step timings in this order:

1. Rust toolchain and CLI compilation cache;
2. candidate build time outside Gantry;
3. mobile derived-cache restoration and authority-tool identity changes;
4. artifact signature verification;
5. packet upload and network latency.

The candidate application's own build time is intentionally outside the Gantry Action
measurement because teams build the exact signed artifact before invoking Gantry.
