/** Runtime guards for the schema-backed mobile governance IPC boundary. */

const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);

function fail(contract, message) {
  throw new TypeError(`${contract} contract violation: ${message}`);
}

function requireObject(value, contract) {
  if (!object(value)) fail(contract, 'expected an object');
  return value;
}

function requireFields(value, contract, fields) {
  const record = requireObject(value, contract);
  for (const field of fields) {
    if (!(field in record)) fail(contract, `missing '${field}'`);
  }
  return record;
}

function requireVersion(value, contract, version) {
  const record = requireObject(value, contract);
  if (record.schema_version !== version) {
    fail(contract, `expected schema ${version}, received ${String(record.schema_version)}`);
  }
  return record;
}

export function validateMobileRelease(value) {
  const release = requireVersion(value, 'mobile release', '1.0');
  requireFields(release, 'mobile release', [
    'id', 'version', 'stage', 'created_at', 'updated_at', 'candidate_ids',
    'active_candidates', 'audit_ids', 'active_audits', 'evidence_link_ids',
    'resolution_ids', 'decision_ids', 'active_decisions', 'waiver_ids',
    'active_waiver_ids', 'latest_event_sequence', 'latest_event_hash'
  ]);
  for (const field of ['candidate_ids', 'audit_ids', 'evidence_link_ids', 'resolution_ids', 'decision_ids', 'waiver_ids', 'active_waiver_ids']) {
    if (!Array.isArray(release[field])) fail('mobile release', `'${field}' must be an array`);
  }
  for (const field of ['active_candidates', 'active_audits', 'active_decisions']) {
    if (!object(release[field])) fail('mobile release', `'${field}' must be an object`);
  }
  return release;
}

export function validateReleaseList(value) {
  if (!Array.isArray(value)) fail('release list', 'expected an array');
  value.forEach(validateMobileRelease);
  return value;
}

export function validateReleasePortfolio(value) {
  const portfolio = requireVersion(value, 'release portfolio', '1.0');
  requireFields(portfolio, 'release portfolio', ['generated_at', 'counts', 'releases', 'review_queue']);
  if (!Array.isArray(portfolio.releases)) fail('release portfolio', "'releases' must be an array");
  if (!Array.isArray(portfolio.review_queue)) fail('release portfolio', "'review_queue' must be an array");
  requireObject(portfolio.counts, 'release portfolio counts');
  return portfolio;
}

export function validateReleaseView(value) {
  const view = requireFields(value, 'release view', [
    'release', 'candidates', 'audits', 'evidence_links', 'resolutions', 'decisions', 'waivers', 'events'
  ]);
  validateMobileRelease(view.release);
  for (const field of ['candidates', 'audits', 'evidence_links', 'resolutions', 'decisions', 'waivers', 'events']) {
    if (!Array.isArray(view[field])) fail('release view', `'${field}' must be an array`);
  }
  return view;
}

export function validateMobileAuditResult(value) {
  const result = requireFields(value, 'mobile audit result', ['report']);
  const report = requireVersion(result.report, 'mobile audit report', '2.0');
  requireFields(report, 'mobile audit report', [
    'generated_at', 'git_commit', 'tool_version', 'policy_digest', 'report_fingerprint',
    'policy', 'decision', 'decision_reasons', 'summary', 'project', 'findings'
  ]);
  if (!Array.isArray(report.findings)) fail('mobile audit report', "'findings' must be an array");
  return result;
}

export function validateReleaseAuditResult(value) {
  const result = requireVersion(value, 'release audit result', '1.0');
  requireFields(result, 'release audit result', ['audit', 'report']);
  requireVersion(result.audit, 'release audit', '1.0');
  validateMobileAuditResult({ report: result.report });
  return result;
}

export function validateSchemaRecord(value, contract, version = '1.0') {
  return requireVersion(value, contract, version);
}

