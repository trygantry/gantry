import { normalizeIndexPayload } from '../../src/modules/contract.js';
import {
  validateMobileRelease,
  validateReleasePortfolio
} from '../../src/modules/mobile-contracts.js';
import assert from 'assert';

console.log("Starting Gantry contract smoke test...");

// 1. Test empty payload defaults
const emptyNorm = normalizeIndexPayload(null);
assert.strictEqual(emptyNorm.vault_name, "Gantry Control Room");
assert.strictEqual(emptyNorm.readiness, "At Risk");
assert.strictEqual(emptyNorm.readiness_score, 0);
assert.deepStrictEqual(emptyNorm.stale_docs, []);
assert.deepStrictEqual(emptyNorm.blockers, []);
assert.deepStrictEqual(emptyNorm.qa_suites, []);
assert.deepStrictEqual(emptyNorm.validation_errors, []);
assert.deepStrictEqual(emptyNorm.go_no_go_gates, []);
assert.deepStrictEqual(emptyNorm.signoffs, []);
assert.deepStrictEqual(emptyNorm.impact_summary.changed_files, []);

// 2. Test immutability
const rawInput = {
  vault_name: "Original Vault",
  blockers: null,
  environment_results: {
    production: {
      readiness: "Go",
      readiness_score: 95
    }
  }
};

const norm1 = normalizeIndexPayload(rawInput, "production");
assert.strictEqual(norm1.vault_name, "Original Vault");
assert.strictEqual(norm1.readiness, "Go");
assert.strictEqual(norm1.readiness_score, 95);
// Ensure input was not mutated
assert.strictEqual(rawInput.readiness, undefined);
assert.strictEqual(rawInput.blockers, null);

// 3. Test active environment resolution
const rawWithActiveEnv = {
  active_environment: "staging",
  environment_results: {
    staging: {
      readiness: "No-Go",
      readiness_score: 40
    },
    production: {
      readiness: "Go",
      readiness_score: 95
    }
  }
};
// If activeEnv parameter is omitted, it should fallback to active_environment from payload
const normStaging = normalizeIndexPayload(rawWithActiveEnv);
assert.strictEqual(normStaging.active_environment, "staging");
assert.strictEqual(normStaging.readiness, "No-Go");
assert.strictEqual(normStaging.readiness_score, 40);

// If activeEnv parameter is provided, it should override active_environment from payload
const normProd = normalizeIndexPayload(rawWithActiveEnv, "production");
assert.strictEqual(normProd.active_environment, "production");
assert.strictEqual(normProd.readiness, "Go");
assert.strictEqual(normProd.readiness_score, 95);

// 4. Test telemetry overrides
const telemetryInput = {
  environment_results: {
    production: {
      telemetry_status: "fresh",
      observed_telemetry: {
        crash_free_rate: 99.99,
        source: "sentry"
      }
    }
  }
};
const normTelemetry = normalizeIndexPayload(telemetryInput, "production");
assert.strictEqual(normTelemetry.telemetry_status, "fresh");
assert.deepStrictEqual(normTelemetry.observed_telemetry, {
  crash_free_rate: 99.99,
  source: "sentry"
});

// 5. Test getReadinessMeta mapping
import { getReadinessMeta } from '../../src/modules/contract.js';

const metaGo = getReadinessMeta("Go");
assert.strictEqual(metaGo.readinessStatusClass, "go");
assert.strictEqual(metaGo.isBlockingStatus, false);
assert.strictEqual(metaGo.statusLabel, "GO FOR LAUNCH");

const metaGoMixed = getReadinessMeta("  gO ");
assert.strictEqual(metaGoMixed.readinessStatusClass, "go");
assert.strictEqual(metaGoMixed.isBlockingStatus, false);
assert.strictEqual(metaGoMixed.statusLabel, "GO FOR LAUNCH");

const metaNoGo = getReadinessMeta("No-Go");
assert.strictEqual(metaNoGo.readinessStatusClass, "no-go");
assert.strictEqual(metaNoGo.isBlockingStatus, true);
assert.strictEqual(metaNoGo.statusLabel, "NO-GO — LAUNCH BLOCKED");

const metaNoGoAlt = getReadinessMeta("nogo");
assert.strictEqual(metaNoGoAlt.readinessStatusClass, "no-go");
assert.strictEqual(metaNoGoAlt.isBlockingStatus, true);
assert.strictEqual(metaNoGoAlt.statusLabel, "NO-GO — LAUNCH BLOCKED");

const metaAtRisk = getReadinessMeta("At Risk");
assert.strictEqual(metaAtRisk.readinessStatusClass, "at-risk");
assert.strictEqual(metaAtRisk.isBlockingStatus, false);
assert.strictEqual(metaAtRisk.statusLabel, "AT RISK — REVIEW REQUIRED");

const metaDefault = getReadinessMeta(null);
assert.strictEqual(metaDefault.readinessStatusClass, "at-risk");
assert.strictEqual(metaDefault.isBlockingStatus, false);
assert.strictEqual(metaDefault.statusLabel, "AT RISK — REVIEW REQUIRED");

// 6. Schema-backed IPC guards fail closed on drift.
const validRelease = {
  schema_version: '1.0', id: 'REL-TEST', version: '1.0.0', name: null, owner: null,
  stage: 'draft', created_at: '2026-07-16T00:00:00Z', updated_at: '2026-07-16T00:00:00Z',
  candidate_ids: [], active_candidates: {}, audit_ids: [], active_audits: {},
  evidence_link_ids: [], resolution_ids: [], decision_ids: [], active_decisions: {},
  waiver_ids: [], active_waiver_ids: [], latest_event_sequence: 1, latest_event_hash: 'a'.repeat(64)
};
assert.strictEqual(validateMobileRelease(validRelease), validRelease);
assert.throws(
  () => validateMobileRelease({ ...validRelease, schema_version: '2.0' }),
  /expected schema 1\.0/
);
assert.throws(
  () => validateReleasePortfolio({ schema_version: '1.0', counts: {}, releases: [] }),
  /missing 'generated_at'/
);

console.log("All contract smoke tests passed successfully!");
