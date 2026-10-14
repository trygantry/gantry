import assert from 'assert';
import fs from 'fs';

globalThis.window = {};

const api = await import('../../src/modules/api.js');

console.log('Starting Gantry mobile workflow acceptance test...');

const initial = await api.listReleaseWorkflows();
assert(initial.length >= 2, 'demo portfolio should include representative releases');
assert.strictEqual(initial[0].version, '3.4.0');
assert.strictEqual(initial[0].name, 'Android production rollout');
assert.ok(initial[0].target_date >= new Date().toISOString().slice(0, 10), 'sample release dates must not already be stale');
assert.strictEqual(initial[1].name, 'iOS beta candidate');

const release = await api.ensureReleaseWorkflow(
  '9.9.0-test',
  'Acceptance candidate',
  'release@example.test',
  'engineer@example.test'
);
assert.strictEqual(release.stage, 'draft');

const scheduled = await api.scheduleRelease(
  release.id,
  '2026-08-10',
  '2026-08-08',
  '2026-08-10',
  'release@example.test'
);
assert.strictEqual(scheduled.target_date, '2026-08-10');

const candidate = await api.addReleaseCandidate(
  release.id,
  'build/acceptance-release.aab',
  'android',
  'ci@example.test'
);
assert.strictEqual(candidate.platform, 'android');

const audit = await api.auditReleaseCandidate(release.id, 'android', 'engineer@example.test');
assert.strictEqual(audit.schema_version, '1.0');
assert.strictEqual(audit.report.schema_version, '2.0');
assert.ok(['go', 'at_risk', 'no_go'].includes(audit.report.decision));
assert.strictEqual(audit.audit.candidate_sha256, candidate.artifact_sha256);
assert.strictEqual(audit.report.artifact.sha256, candidate.artifact_sha256);

const view = await api.showReleaseWorkflow(release.id);
assert.strictEqual(view.release.id, release.id);
assert.strictEqual(view.release.active_candidates.android, candidate.id);
assert.ok(view.audits.some(item => item.id === audit.audit.id));

const evidence = await api.addReleaseEvidence(
  release.id,
  'android',
  'MOB-EVD-001:release_owner_approval',
  'release_owner_approval',
  'Release owner approved the exact candidate',
  'release@example.test'
);
assert.strictEqual(evidence.link.candidate_id, candidate.id);
assert.strictEqual(evidence.link.source_audit_id, audit.audit.id);

const decision = await api.decideRelease(
  release.id,
  'android',
  'approved',
  'release@example.test',
  'release_owner',
  'Reviewed the artifact-bound audit and evidence.'
);
assert.strictEqual(decision.candidate_id, candidate.id);
assert.strictEqual(decision.audit_id, audit.audit.id);

const portfolio = await api.getReleasePortfolio();
assert.strictEqual(portfolio.schema_version, '1.0');
assert.ok(portfolio.releases.some(item => item.id === release.id));

const health = await api.checkVaultHealth(false);
assert.strictEqual(health.schema_version, '1.0');
assert.strictEqual(health.healthy, true);
const migration = await api.previewVaultMigration();
assert.strictEqual(migration.dry_run, true);
assert.strictEqual(migration.migrations_required, 0);
const support = await api.writeSupportBundle();
assert.strictEqual(support.schema_version, '1.0');
assert.match(String(support.path), /gantry-support/);

const html = fs.readFileSync(new URL('../../src/index.html', import.meta.url), 'utf8');
assert.match(html, /data-mobile-workspace="releases"[^>]*aria-label="Open Release Portfolio"/);
assert.match(html, /data-mobile-workspace="audit"[^>]*aria-label="Open Candidate Audit"/);
assert.match(html, /data-mobile-workspace="policy"[^>]*aria-label="Open Release Policies"/);
assert.match(html, /id="mobile-policy-workspace"[^>]*hidden/);
assert.doesNotMatch(html, /aria-label="Mobile release workspace"/, 'sidebar navigation is the single workspace switcher');
assert.match(html, /id="mobile-release-review-queue"[^>]*aria-live="polite"/);
assert.match(html, /role="dialog"[^>]*aria-modal="true"[^>]*aria-hidden="true"[^>]*hidden inert/);
assert.match(html, /id="btn-vault-check"/);
assert.match(html, /id="btn-vault-support"/);
assert.doesNotMatch(html, /tabindex="[1-9]/, 'positive tabindex values break natural keyboard order');

const mainSource = fs.readFileSync(new URL('../../src/main.js', import.meta.url), 'utf8');
assert.match(mainSource, /switchView\("mobile"\);\s*await openMobileWorkspace\("releases", "releases"\);/, 'sample entry must open the primary release portfolio');

const modalSource = fs.readFileSync(new URL('../../src/modules/modal.js', import.meta.url), 'utf8');
assert.match(modalSource, /event\.key === 'Escape'/);
assert.match(modalSource, /previous\.focus\(\)/);
assert.match(modalSource, /event\.key !== 'Tab'/);

console.log('Mobile workflow and accessibility acceptance tests passed.');
