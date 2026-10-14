import { addReleaseCandidate, addReleaseEvidence, auditReleaseCandidate, cancelMobileAudit, decideRelease, diffReleaseCandidates, ensureReleaseWorkflow, getCIStatus, getGitUser, getReleasePortfolio, initMobilePolicy, listReleaseReports, openProjectFile, pickMobileArtifact, resolveReleaseFinding, runMobileAudit, scheduleRelease, showReleaseWorkflow, waiveReleaseFinding, writeReleasePacket } from '../api.js';

let lastReport = null;
let lastReportPath = null;
let findingFilter = 'attention';
let wired = false;
let releaseWorkspace = 'releases';
let mobileNavContext = 'releases';
let savedReleases = [];
let selectedReleaseId = null;
let releasePortfolio = null;
let releaseQueueFilter = 'all';
let releaseSearch = '';
let operatorIdentity = '';
let operatorIdentityLoaded = false;
let auditResultStale = false;
let auditedArtifactPath = null;
let detailRequestSerial = 0;
let releasePresentation = 'list';
let ciConfigured = false;
const setupStorageKey = 'gantry.releaseSetup.v1';

export async function renderMobileRelease() {
  if (!operatorIdentityLoaded) {
    operatorIdentityLoaded = true;
    const user = await getGitUser().catch(() => ({ name: '', email: '' }));
    operatorIdentity = user.email || user.name || '';
    ciConfigured = Boolean((await getCIStatus().catch(() => ({ configured: false }))).configured);
  }
  wireControls();
  syncMobileWorkspace();
  updateAuditMode();
  if (lastReport) renderReport(lastReport, lastReportPath);
  if (releaseWorkspace === 'releases') await loadSavedReleases();
  renderOnboarding();
}

function wireControls() {
  if (wired) return;
  wired = true;
  const artifactInput = document.getElementById('mobile-artifact-path');
  document.getElementById('btn-mobile-audit')?.addEventListener('click', runAudit);
  document.getElementById('btn-mobile-audit-cancel')?.addEventListener('click', cancelAudit);
  document.getElementById('btn-mobile-policy')?.addEventListener('click', preparePolicy);
  document.getElementById('btn-mobile-policy-create')?.addEventListener('click', preparePolicy);
  document.getElementById('btn-mobile-policy-open')?.addEventListener('click', openPolicyFile);
  document.getElementById('btn-mobile-onboarding-toggle')?.addEventListener('click', toggleOnboarding);
  document.getElementById('btn-mobile-artifact')?.addEventListener('click', chooseArtifact);
  artifactInput?.addEventListener('input', handleAuditInputChange);
  document.querySelectorAll('[data-mobile-filter]').forEach((button) => {
    button.addEventListener('click', () => setFindingFilter(button.dataset.mobileFilter));
  });
  document.getElementById('btn-mobile-track-release')?.addEventListener('click', trackAuditedCandidate);
  document.getElementById('btn-mobile-release-refresh')?.addEventListener('click', () => loadSavedReleases(true));
  document.getElementById('mobile-release-search')?.addEventListener('input', async (event) => {
    releaseSearch = event.target.value.trim().toLowerCase();
    renderReleasePortfolio();
    await synchronizeReleaseSelection();
  });
  document.querySelectorAll('[data-release-presentation]').forEach((button) => {
    button.addEventListener('click', () => {
      releasePresentation = button.dataset.releasePresentation;
      renderReleaseList();
    });
  });
  document.querySelectorAll('[data-release-queue-filter]').forEach((button) => {
    button.addEventListener('click', () => {
      releaseQueueFilter = button.dataset.releaseQueueFilter;
      renderReleasePortfolio();
    });
  });
}

export async function openMobileWorkspace(workspace, context = workspace) {
  await setReleaseWorkspace(workspace, context);
  const title = document.getElementById('page-title-display');
  if (title) title.textContent = context === 'releases' ? 'Release Portfolio' : context === 'policy' ? 'Release Policies' : 'Audit Candidate';
}

async function setReleaseWorkspace(workspace, context = workspace) {
  releaseWorkspace = workspace;
  mobileNavContext = context;
  const audit = document.getElementById('mobile-audit-workspace');
  const policy = document.getElementById('mobile-policy-workspace');
  const releases = document.getElementById('mobile-release-workspace');
  if (audit) audit.hidden = workspace !== 'audit';
  if (policy) policy.hidden = workspace !== 'policy';
  if (releases) releases.hidden = workspace !== 'releases';
  syncMobileWorkspace();
  if (workspace === 'releases') await loadSavedReleases();
}

function syncMobileWorkspace() {
  const audit = document.getElementById('mobile-audit-workspace');
  const policy = document.getElementById('mobile-policy-workspace');
  const releases = document.getElementById('mobile-release-workspace');
  if (audit) audit.hidden = releaseWorkspace !== 'audit';
  if (policy) policy.hidden = releaseWorkspace !== 'policy';
  if (releases) releases.hidden = releaseWorkspace !== 'releases';
  document.querySelectorAll('.nav-item[data-view="mobile"]').forEach((item) => {
    item.classList.toggle('active', item.dataset.mobileContext === mobileNavContext);
  });
}

function releaseSetupState() {
  try { return JSON.parse(localStorage.getItem(setupStorageKey) || '{}'); }
  catch { return {}; }
}

function rememberReleaseSetup(update) {
  try { localStorage.setItem(setupStorageKey, JSON.stringify({ ...releaseSetupState(), ...update })); }
  catch { /* Derived repository progress still works when storage is unavailable. */ }
}

function openPrimaryDestination(view, workspace = null, context = null, focusId = null) {
  const nav = document.querySelector(`.nav-item[data-view="${view}"]${context ? `[data-mobile-context="${context}"]` : ''}`);
  nav?.click();
  if (workspace) openMobileWorkspace(workspace, context || workspace);
  if (focusId) queueMicrotask(() => document.getElementById(focusId)?.focus());
}

function renderOnboarding() {
  const list = document.getElementById('mobile-release-onboarding-steps');
  const progress = document.getElementById('mobile-release-onboarding-progress');
  if (!list || !progress) return;
  const setup = releaseSetupState();
  const hasRelease = savedReleases.length > 0;
  const hasAudit = savedReleases.some(release => Boolean(release.automated_decision)) || Boolean(lastReport?.artifact);
  const hasDecision = savedReleases.some(release => Number(release.active_decisions || 0) > 0);
  const steps = [
    ['Detect project', Boolean(lastReport?.project) || hasRelease, 'Confirm Gantry sees the first-party mobile targets.', () => openPrimaryDestination('mobile', 'audit', 'audit', 'btn-mobile-audit')],
    ['Prepare policy', Boolean(setup.policyPrepared) || hasAudit, 'Create or preserve the repository-owned mobile policy.', () => openPrimaryDestination('mobile', 'policy', 'policy', 'btn-mobile-policy-create')],
    ['Connect CI', ciConfigured, 'Run the same deterministic gate for every candidate build.', () => openPrimaryDestination('ci-setup')],
    ['Audit exact candidate', hasAudit, 'Fingerprint the IPA, AAB, or APK intended for upload.', () => openPrimaryDestination('mobile', 'audit', 'audit', 'mobile-artifact-path')],
    ['Track release', hasRelease, 'Promote the audited artifact into durable release history.', () => openPrimaryDestination('mobile', hasAudit ? 'audit' : 'releases', hasAudit ? 'audit' : 'releases', hasAudit ? 'btn-mobile-track-release' : null)],
    ['Request accountable decision', hasDecision, 'Review evidence and record approval or rejection.', () => openPrimaryDestination('mobile', 'releases', 'releases')]
  ];
  const completed = steps.filter(([, done]) => done).length;
  progress.textContent = `${completed} of ${steps.length} complete`;
  const onboarding = document.getElementById('mobile-release-onboarding');
  const toggle = document.getElementById('btn-mobile-onboarding-toggle');
  const compact = setup.onboardingExpanded === false || (setup.onboardingExpanded === undefined && completed >= 4);
  onboarding?.classList.toggle('compact', compact);
  if (toggle) {
    toggle.textContent = compact ? 'Show steps' : 'Hide steps';
    toggle.setAttribute('aria-expanded', String(!compact));
  }
  list.textContent = '';
  const firstIncomplete = steps.findIndex(([, done]) => !done);
  steps.forEach(([label, done, copy, action], index) => {
    const item = document.createElement('li');
    item.className = done ? 'complete' : index === firstIncomplete ? 'next' : '';
    const state = textElement('span', done ? '✓' : String(index + 1));
    const content = document.createElement('div');
    content.append(textElement('strong', label), textElement('small', copy));
    const button = textElement('button', done ? 'Review' : index === firstIncomplete ? 'Continue' : 'Open');
    button.className = index === firstIncomplete ? 'btn btn-primary' : 'btn';
    button.addEventListener('click', action);
    item.append(state, content, button);
    list.appendChild(item);
  });
}

function toggleOnboarding() {
  const onboarding = document.getElementById('mobile-release-onboarding');
  const expanded = onboarding?.classList.contains('compact');
  rememberReleaseSetup({ onboardingExpanded: Boolean(expanded) });
  renderOnboarding();
}

async function loadSavedReleases(force = false) {
  if (savedReleases.length && !force) return;
  setReleaseFeedback('Loading verified release records…');
  try {
    releasePortfolio = await getReleasePortfolio();
    savedReleases = releasePortfolio.releases || [];
    if (!selectedReleaseId || !savedReleases.some(release => release.id === selectedReleaseId)) {
      selectedReleaseId = savedReleases[0]?.id || null;
    }
    renderReleasePortfolio();
    renderReleaseList();
    if (selectedReleaseId) await loadReleaseDetail(selectedReleaseId);
    else renderReleaseEmpty('No tracked releases', 'Audit an exact candidate, then track it here without leaving Gantry.');
    setReleaseFeedback(savedReleases.length ? `${savedReleases.length} verified release record${savedReleases.length === 1 ? '' : 's'} loaded.` : 'No persistent releases found.', 'success');
    renderOnboarding();
  } catch (error) {
    renderReleaseEmpty('Release records unavailable', String(error));
    setReleaseFeedback(`Could not load releases: ${error}`, 'error');
  }
}

function renderReleasePortfolio() {
  if (!releasePortfolio) return;
  const metrics = document.getElementById('mobile-release-portfolio-metrics');
  if (metrics) {
    metrics.textContent = '';
    [
      ['Total releases', releasePortfolio.counts.total, ''],
      ['Need attention', releasePortfolio.counts.needs_attention, ''],
      ['Schedule pressure', Number(releasePortfolio.counts.overdue || 0) + Number(releasePortfolio.counts.due_soon || 0), releasePortfolio.counts.overdue ? 'blocked' : ''],
      ['Blocked / rejected', releasePortfolio.counts.blocked_or_rejected, 'blocked'],
      ['Awaiting approval', releasePortfolio.counts.awaiting_approval, ''],
      ['Risk / expiry', releasePortfolio.counts.approved_with_risk + releasePortfolio.counts.accepted_risk + releasePortfolio.counts.expiring_evidence + releasePortfolio.counts.expiring_waivers, 'risk']
    ].forEach(([label, value, kind]) => {
      const metric = document.createElement('div');
      const activeKind = kind && Number(value) > 0 ? kind : '';
      metric.className = `mobile-release-metric ${activeKind}`.trim();
      metric.append(textElement('strong', value), textElement('span', label));
      metrics.appendChild(metric);
    });
  }
  document.querySelectorAll('[data-release-queue-filter]').forEach((button) => {
    const active = button.dataset.releaseQueueFilter === releaseQueueFilter;
    button.classList.toggle('active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  const queue = document.getElementById('mobile-release-review-queue');
  if (!queue) return;
  queue.textContent = '';
  const visible = (releasePortfolio.review_queue || []).filter((item) => {
    const matchesFilter = releaseQueueFilter === 'all'
      || (releaseQueueFilter === 'blocked' && ['blocked', 'rejected'].includes(item.kind))
      || (releaseQueueFilter === 'approval' && item.kind === 'approval_required')
      || (releaseQueueFilter === 'risk' && ['approved_with_risk', 'accepted_risk', 'expiring_evidence', 'expiring_waiver'].includes(item.kind))
      || (releaseQueueFilter === 'schedule' && ['overdue', 'due_soon'].includes(item.kind));
    return matchesFilter && matchesReleaseSearch(item);
  });
  if (!visible.length) {
    const empty = document.createElement('div');
    empty.className = 'mobile-empty';
    empty.append(textElement('strong', 'Queue is clear'), textElement('span', releaseSearch ? 'No review items match this search and filter.' : 'No verified releases need attention in this view.'));
    queue.appendChild(empty);
    return;
  }
  visible.forEach((item) => {
    const button = document.createElement('button');
    button.className = 'mobile-release-review-item';
    button.dataset.severity = item.severity;
    const dueLabel = item.due_at
      ? `due ${['overdue', 'due_soon'].includes(item.kind) ? formatDateOnly(item.due_at.slice(0, 10)) : formatDate(item.due_at)}`
      : null;
    const meta = [item.release_version, item.platform?.toUpperCase(), item.owner || 'Unassigned', dueLabel].filter(Boolean).join(' · ');
    const next = textElement('p', item.next_action);
    next.className = 'mobile-release-review-next';
    button.append(textElement('span', `${humanizeReason(item.kind)} · ${meta}`), textElement('strong', item.title), textElement('p', item.reason), next);
    button.addEventListener('click', async () => {
      selectedReleaseId = item.release_id;
      renderReleaseList();
      await loadReleaseDetail(item.release_id);
      document.getElementById('mobile-release-detail')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
    queue.appendChild(button);
  });
}

function matchesReleaseSearch(item) {
  if (!releaseSearch) return true;
  return [item.release_version, item.release_name, item.owner, item.platform, item.title, item.reason]
    .filter(Boolean)
    .some(value => String(value).toLowerCase().includes(releaseSearch));
}

function renderReleaseList() {
  const container = document.getElementById('mobile-release-list');
  if (!container) return;
  container.textContent = '';
  const visible = visibleSavedReleases();
  container.classList.toggle('calendar', releasePresentation === 'calendar');
  setText('mobile-release-list-count', visible.length);
  document.querySelectorAll('[data-release-presentation]').forEach((button) => {
    const active = button.dataset.releasePresentation === releasePresentation;
    button.classList.toggle('active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  if (!visible.length) {
    container.appendChild(textElement('div', 'No verified releases match this search.'));
    return;
  }
  if (releasePresentation === 'calendar') {
    renderReleaseCalendar(container, visible);
    return;
  }
  visible.forEach((release) => {
    const button = document.createElement('button');
    button.className = 'mobile-release-list-item';
    button.classList.toggle('active', release.id === selectedReleaseId);
    button.setAttribute('aria-pressed', String(release.id === selectedReleaseId));
    const top = document.createElement('span');
    top.className = 'mobile-release-list-top';
    const displayedStage = release.review_kinds?.includes('approved_with_risk') ? 'approved_with_risk' : release.stage;
    top.append(textElement('strong', release.version), stageBadge(displayedStage));
    const status = textElement('small', release.needs_attention ? `${release.counts?.blockers || 0} blockers · ${release.age_days}d old · ${release.next_action}` : `${release.age_days}d old · no queued review`);
    if (release.needs_attention) status.className = 'mobile-release-list-item-needs';
    const schedule = release.target_date ? ` · target ${formatDateOnly(release.target_date)}` : '';
    button.append(top, textElement('span', release.name || release.id), textElement('small', `${release.owner || 'Unassigned'} · ${(release.platforms || []).map(value => value.toUpperCase()).join(' + ') || 'No candidate'}${schedule}`), status);
    button.addEventListener('click', async () => {
      selectedReleaseId = release.id;
      renderReleaseList();
      await loadReleaseDetail(release.id);
    });
    container.appendChild(button);
  });
}

function renderReleaseCalendar(container, releases) {
  container.classList.add('calendar');
  const ordered = [...releases].sort((left, right) => (left.target_date || '9999').localeCompare(right.target_date || '9999'));
  let currentGroup = null;
  ordered.forEach((release) => {
    const group = release.target_date
      ? new Date(`${release.target_date}T00:00:00Z`).toLocaleDateString([], { month: 'long', year: 'numeric', timeZone: 'UTC' })
      : 'Unscheduled';
    if (group !== currentGroup) {
      const heading = textElement('strong', group);
      heading.className = 'mobile-release-calendar-month';
      container.appendChild(heading);
      currentGroup = group;
    }
    const button = document.createElement('button');
    button.className = 'mobile-release-calendar-item';
    button.classList.toggle('active', release.id === selectedReleaseId);
    button.append(
      textElement('span', release.target_date ? formatCalendarDay(release.target_date) : '—'),
      textElement('strong', `${release.version} · ${release.name || release.id}`),
      textElement('small', release.days_to_target == null ? 'Set a target date' : release.days_to_target < 0 ? `${Math.abs(release.days_to_target)}d overdue` : `${release.days_to_target}d remaining`)
    );
    button.addEventListener('click', async () => {
      selectedReleaseId = release.id;
      renderReleaseList();
      await loadReleaseDetail(release.id);
    });
    container.appendChild(button);
  });
}

function visibleSavedReleases() {
  return savedReleases.filter((release) => !releaseSearch || [release.version, release.name, release.owner, ...(release.platforms || [])]
    .filter(Boolean)
    .some(value => String(value).toLowerCase().includes(releaseSearch)));
}

async function synchronizeReleaseSelection() {
  const visible = visibleSavedReleases();
  if (!visible.some(release => release.id === selectedReleaseId)) {
    selectedReleaseId = visible[0]?.id || null;
  }
  renderReleaseList();
  setReleaseFeedback('', 'neutral');
  if (selectedReleaseId) await loadReleaseDetail(selectedReleaseId);
  else {
    detailRequestSerial += 1;
    renderReleaseEmpty('No matching release', 'Clear or change the search to reopen a verified release.');
  }
}

async function loadReleaseDetail(releaseId) {
  const container = document.getElementById('mobile-release-detail');
  if (!container) return;
  const requestSerial = ++detailRequestSerial;
  container.textContent = '';
  container.appendChild(textElement('div', 'Verifying release history…'));
  try {
    const [view, reports] = await Promise.all([
      showReleaseWorkflow(releaseId),
      listReleaseReports(releaseId)
    ]);
    if (requestSerial !== detailRequestSerial || selectedReleaseId !== releaseId) return;
    renderReleaseDetail(view, reports);
  } catch (error) {
    if (requestSerial !== detailRequestSerial || selectedReleaseId !== releaseId) return;
    renderReleaseEmpty('Release verification failed', String(error));
    setReleaseFeedback(`Release verification failed: ${error}`, 'error');
  }
}

function renderReleaseDetail(view, reports = []) {
  const container = document.getElementById('mobile-release-detail');
  if (!container) return;
  container.textContent = '';
  const release = view.release;
  const portfolioItem = releasePortfolio?.releases?.find(item => item.id === release.id);
  const header = document.createElement('header');
  header.className = 'mobile-release-detail-header';
  const title = document.createElement('div');
  title.append(textElement('div', 'VERIFIED RELEASE RECORD'), textElement('h2', release.name || release.version), textElement('p', `${release.version} · ${release.id} · updated ${formatDate(release.updated_at)}`));
  header.append(title, stageBadge(portfolioItem?.review_kinds?.includes('approved_with_risk') ? 'approved_with_risk' : release.stage));

  const actions = document.createElement('div');
  actions.className = 'mobile-release-detail-actions';
  const packetButton = textElement('button', 'Write handoff packet');
  packetButton.className = 'btn btn-primary';
  packetButton.addEventListener('click', () => writePacket(release.id, packetButton));
  const diffButton = textElement('button', 'Compare candidates');
  diffButton.className = 'btn';
  diffButton.disabled = view.candidates.length < 2;
  diffButton.title = diffButton.disabled ? 'A prior candidate is required' : '';
  diffButton.addEventListener('click', () => compareCandidates(release, diffButton));
  actions.append(packetButton, diffButton);

  const handoff = document.createElement('section');
  handoff.id = 'mobile-release-handoff';
  handoff.className = 'mobile-release-handoff';
  handoff.hidden = true;
  const handoffBody = document.createElement('div');
  handoffBody.className = 'mobile-release-handoff-body';
  handoff.append(textElement('h3', 'Verified handoff summary'), handoffBody);

  const platforms = document.createElement('div');
  platforms.className = 'mobile-release-platforms';
  Object.entries(release.active_candidates || {}).forEach(([platform, candidateId]) => {
    const candidate = view.candidates.find(item => item.id === candidateId);
    const auditId = release.active_audits?.[platform];
    const audit = view.audits.find(item => item.id === auditId);
    const decisionId = release.active_decisions?.[platform];
    const decision = view.decisions.find(item => item.id === decisionId);
    const card = document.createElement('article');
    card.className = 'mobile-release-platform-card';
    card.append(textElement('strong', platform.toUpperCase()), detailRows([
      ['Candidate', candidate ? `${candidate.id} · ${shortHash(candidate.artifact_sha256)}` : candidateId],
      ['Artifact', candidate ? `${String(candidate.artifact_kind).toUpperCase()} · ${formatBytes(candidate.artifact_size_bytes)}` : 'Unavailable'],
      ['Audit', audit ? `${audit.decision.replaceAll('_', ' ').toUpperCase()} · ${audit.id}` : 'Not audited'],
      ['Human decision', decision ? `${decision.decision.toUpperCase()} by ${decision.decided_by}` : 'Pending'],
      ['Policy', audit ? shortHash(audit.policy_digest) : 'No active audit']
    ]));
    platforms.appendChild(card);
  });

  const nextAction = document.createElement('div');
  nextAction.className = 'mobile-release-next-action';
  nextAction.append(textElement('strong', 'Next best action'), document.createTextNode(portfolioItem?.next_action || 'Continue the verified release workflow.'));
  container.append(header, nextAction, actions, handoff, releaseSchedulePanel(view), candidateOperations(view));
  if (platforms.childElementCount) container.appendChild(platforms);
  container.appendChild(releaseFindingQueue(view, reports));
  if (Object.keys(release.active_audits || {}).length) container.appendChild(decisionPanel(view, reports));
  container.appendChild(releaseActivityPanel(view));
  container.append(sectionList('Decision history', view.decisions, decision => decisionRecordLabel(view, decision), 'No accountable decisions recorded.'));
  container.append(sectionList('Accepted risk', view.waivers, waiver => waiverRecordLabel(view, waiver), 'No waivers recorded.'));
  container.append(sectionList('Verified timeline', view.events, event => `${event.sequence}. ${humanizeReason(event.kind)} · ${formatDate(event.occurred_at)}${event.actor ? ` · ${event.actor}` : ''}`, 'No events recorded.'));
}

function releaseActivityPanel(view) {
  const section = document.createElement('section');
  section.className = 'mobile-release-activity';
  const header = document.createElement('div');
  header.append(textElement('h3', 'Since your last review'));
  const key = `gantry.releaseReview.${view.release.id}`;
  let reviewedSequence = 0;
  try { reviewedSequence = Number(localStorage.getItem(key) || 0); } catch { /* local review state is optional */ }
  const changes = view.events.filter(event => Number(event.sequence) > reviewedSequence);
  const list = document.createElement('ul');
  if (!changes.length) list.appendChild(textElement('li', 'No verified workflow changes since your last review.'));
  else changes.slice(-8).reverse().forEach((event) => {
    list.appendChild(textElement('li', `${activityLabel(event.kind)} · ${formatDate(event.occurred_at)}${event.actor ? ` · ${event.actor}` : ''}`));
  });
  const mark = textElement('button', changes.length ? `Mark ${changes.length} reviewed` : 'Reviewed');
  mark.className = 'btn';
  mark.disabled = !changes.length;
  mark.addEventListener('click', () => {
    const latest = Math.max(0, ...view.events.map(event => Number(event.sequence) || 0));
    try { localStorage.setItem(key, String(latest)); } catch { /* no persistent storage */ }
    section.replaceWith(releaseActivityPanel(view));
    setReleaseFeedback('This device will highlight later verified release changes.', 'success');
  });
  header.appendChild(mark);
  section.append(header, list);
  return section;
}

function activityLabel(kind) {
  return ({
    release_created: 'Release record created',
    release_scheduled: 'Schedule changed',
    candidate_bound: 'Candidate identity changed',
    candidate_audited: 'Automated audit refreshed',
    evidence_linked: 'Evidence attached',
    finding_resolved: 'Finding resolution recorded',
    release_approved: 'Candidate approved',
    release_rejected: 'Candidate rejected',
    finding_waived: 'Risk acceptance recorded'
  })[kind] || humanizeReason(kind);
}

function releaseSchedulePanel(view) {
  const panel = document.createElement('details');
  panel.className = 'mobile-release-schedule';
  panel.open = !view.release.target_date;
  panel.appendChild(textElement('summary', view.release.target_date
    ? `Release schedule · target ${formatDateOnly(view.release.target_date)}`
    : 'Set release target and operating dates'));
  const fields = document.createElement('div');
  fields.className = 'mobile-release-schedule-fields';
  const target = dateInputElement('Target date', view.release.target_date);
  const submission = dateInputElement('Submission date', view.release.submission_date);
  const rollout = dateInputElement('Rollout date', view.release.rollout_date);
  const actor = inputElement('Scheduled by', 'release-manager@company.com');
  actor.input.value = operatorIdentity || view.release.owner || '';
  const save = textElement('button', 'Save verified schedule');
  save.className = 'btn btn-primary';
  save.addEventListener('click', async () => {
    if (!actor.input.value.trim() || (!target.input.value && !submission.input.value && !rollout.input.value)) {
      setReleaseFeedback('A scheduler identity and at least one date are required.', 'error');
      return;
    }
    setBusy(save, true, 'Saving schedule…');
    try {
      await scheduleRelease(view.release.id, target.input.value || null, submission.input.value || null, rollout.input.value || null, actor.input.value.trim());
      savedReleases = [];
      await loadSavedReleases(true);
      setReleaseFeedback('Release dates were recorded in the verified event history.', 'success');
    } catch (error) { setReleaseFeedback(`Schedule failed: ${error}`, 'error'); }
    finally { setBusy(save, false); }
  });
  fields.append(target.label, submission.label, rollout.label, actor.label, save);
  panel.appendChild(fields);
  return panel;
}

function candidateOperations(view) {
  const panel = document.createElement('details');
  panel.className = 'mobile-release-operations';
  panel.append(
    textElement('summary', 'Engineer actions · bind or audit a candidate'),
    textElement('p', 'Bind the exact store-upload artifact, then audit the active candidate. A new candidate invalidates prior active audit and approval state for that platform.')
  );
  const path = inputElement('Artifact path', 'build/app/outputs/bundle/release/app-release.aab');
  const actor = inputElement('Engineer identity', 'engineer@company.com');
  actor.input.value = operatorIdentity;
  const platform = selectElement('Platform', ['android', 'ios']);
  platform.input.value = Object.keys(view.release.active_candidates || {})[0] || 'android';
  const choose = textElement('button', 'Choose candidate…');
  choose.className = 'btn';
  choose.addEventListener('click', async () => {
    setBusy(choose, true, 'Choosing…');
    try {
      const selected = await pickMobileArtifact();
      if (selected) {
        path.input.value = selected;
        if (selected.toLowerCase().endsWith('.ipa')) platform.input.value = 'ios';
        else platform.input.value = 'android';
        setReleaseFeedback('Candidate selected. Bind it to preserve its identity in the release record.', 'success');
      } else setReleaseFeedback('No file selected. You can enter a project-relative artifact path.', 'neutral');
    } catch (error) { setReleaseFeedback(`Candidate selection failed: ${error}`, 'error'); }
    finally { setBusy(choose, false); }
  });
  const bind = textElement('button', 'Bind candidate');
  bind.className = 'btn';
  const confirmation = document.createElement('div');
  confirmation.className = 'mobile-candidate-replacement';
  confirmation.hidden = true;
  const resetConfirmation = () => {
    confirmation.hidden = true;
    confirmation.textContent = '';
    bind.textContent = 'Bind candidate';
    delete bind.dataset.confirming;
  };
  path.input.addEventListener('input', resetConfirmation);
  platform.input.addEventListener('change', resetConfirmation);
  bind.addEventListener('click', () => {
    const activeCandidate = view.release.active_candidates?.[platform.input.value];
    if (activeCandidate && bind.dataset.confirming !== 'true') {
      confirmation.hidden = false;
      confirmation.append(
        textElement('strong', 'This replaces the active candidate.'),
        textElement('span', `${activeCandidate} and its active audit, decision, and candidate-scoped evidence will become stale for ${platform.input.value.toUpperCase()}.`)
      );
      bind.dataset.confirming = 'true';
      bind.textContent = 'Confirm replacement';
      setReleaseFeedback('Review the invalidation warning, then confirm the candidate replacement.', 'warning');
      return;
    }
    bindCandidate(view.release.id, platform.input.value, path.input, actor.input, bind);
  });
  const audit = textElement('button', 'Audit active candidate');
  audit.className = 'btn btn-primary';
  audit.addEventListener('click', () => auditCandidate(view.release.id, platform.input.value, actor.input, audit));
  const pathRow = document.createElement('div');
  pathRow.className = 'mobile-release-path-row';
  pathRow.append(path.label, choose);
  const buttons = document.createElement('div');
  buttons.className = 'mobile-release-detail-actions';
  buttons.append(bind, audit);
  panel.append(pathRow, platform.label, actor.label, confirmation, buttons);
  return panel;
}

async function bindCandidate(release, platform, path, actor, button) {
  if (!path.value.trim() || !actor.value.trim()) {
    setReleaseFeedback('Artifact path and engineer identity are required to bind a candidate.', 'error');
    return;
  }
  setBusy(button, true, 'Binding…');
  try {
    const candidate = await addReleaseCandidate(release, path.value.trim(), platform, actor.value.trim());
    await loadSavedReleases(true);
    setReleaseFeedback(`${platform.toUpperCase()} candidate ${candidate.id} bound. Run its audit before requesting approval.`, 'success');
  } catch (error) { setReleaseFeedback(`Candidate binding failed: ${error}`, 'error'); }
  finally { setBusy(button, false); }
}

async function auditCandidate(release, platform, actor, button) {
  if (!actor.value.trim()) {
    setReleaseFeedback('Engineer identity is required to audit a candidate.', 'error');
    return;
  }
  setBusy(button, true, 'Auditing…');
  try {
    const result = await auditReleaseCandidate(release, platform, actor.value.trim());
    await loadSavedReleases(true);
    setReleaseFeedback(`${platform.toUpperCase()} audit ${result.audit.id} recorded: ${humanizeReason(result.audit.decision)}.`, result.audit.decision === 'no_go' ? 'error' : 'success');
  } catch (error) { setReleaseFeedback(`Candidate audit failed: ${error}`, 'error'); }
  finally { setBusy(button, false); }
}

function releaseFindingQueue(view, reports) {
  const section = document.createElement('section');
  section.className = 'mobile-release-work-queue';
  section.append(
    textElement('h3', 'Active audit work queue'),
    textElement('p', 'Attach human proof, record completed remediation, or accept a time-bounded risk. These actions are recorded and bound to the active audit.')
  );
  let count = 0;
  Object.entries(view.release.active_audits || {}).forEach(([platform, auditId]) => {
    const audit = view.audits.find(item => item.id === auditId);
    const report = reports.find(item => item.report_fingerprint === audit?.report_fingerprint);
    (report?.findings || []).filter(finding => finding.status !== 'pass').forEach((finding) => {
      count += 1;
      section.appendChild(releaseFindingItem(view, platform, finding));
    });
  });
  if (!count) {
    section.appendChild(textElement('div', Object.keys(view.release.active_audits || {}).length
      ? 'No active findings need attention.'
      : 'Audit an active candidate to create an actionable work queue.'));
  }
  return section;
}

function releaseFindingItem(view, platform, finding) {
  const item = document.createElement('article');
  item.className = `mobile-release-work-item status-${finding.status}`;
  const header = document.createElement('div');
  const title = document.createElement('div');
  title.append(
    textElement('span', `${statusLabel(finding.status)} · ${platform.toUpperCase()} · ${finding.rule_id || finding.id}`),
    textElement('strong', finding.title),
    textElement('p', finding.remediation || 'Record how this release requirement is being handled.'),
    textElement('small', [finding.locations?.[0] ? `File: ${finding.locations[0]}` : null, finding.expected ? `Expected: ${finding.expected}` : null].filter(Boolean).join(' · '))
  );
  const resolution = view.resolutions.find(item => item.finding_instance_id === finding.finding_instance_id && item.platform === platform);
  const waiver = view.waivers.find(item => item.finding_instance_id === finding.finding_instance_id && item.platform === platform && item.status === 'active');
  const evidence = view.evidence_links.filter(item => item.finding_instance_id === finding.finding_instance_id && item.platform === platform);
  header.append(title, workStatus(resolution, waiver, evidence));
  item.appendChild(header);

  const actions = document.createElement('div');
  actions.className = 'mobile-release-finding-actions';
  actions.appendChild(findingQuickActions(view, platform, finding));
  if (finding.category === 'evidence') actions.appendChild(evidenceAction(view.release.id, platform, finding));
  if (!resolution) actions.appendChild(resolutionAction(view.release.id, platform, finding, evidence));
  if (!waiver) actions.appendChild(waiverAction(view.release.id, platform, finding));
  item.appendChild(actions);
  return item;
}

function findingQuickActions(view, platform, finding) {
  const actions = document.createElement('div');
  actions.className = 'mobile-release-finding-quick-actions';
  const location = finding.locations?.[0]?.replace(/:(\d+)(:\d+)?$/, '');
  if (location) {
    const open = textElement('button', 'Open affected file');
    open.className = 'btn';
    open.addEventListener('click', async () => {
      try {
        await openProjectFile(location);
        setReleaseFeedback(`Opened ${location}.`, 'success');
      } catch (error) { setReleaseFeedback(`Could not open ${location}: ${error}`, 'error'); }
    });
    actions.appendChild(open);
  }
  const copy = textElement('button', 'Copy next step');
  copy.className = 'btn';
  copy.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(finding.remediation || finding.title);
      setReleaseFeedback('Remediation copied to the clipboard.', 'success');
    } catch { setReleaseFeedback('Clipboard access is unavailable. The remediation remains visible above.', 'warning'); }
  });
  const rerun = textElement('button', 'Rerun candidate audit');
  rerun.className = 'btn';
  rerun.addEventListener('click', async () => {
    const actor = operatorIdentity || view.release.owner;
    if (!actor) {
      setReleaseFeedback('Set a Git identity or release owner before rerunning the audit.', 'error');
      return;
    }
    setBusy(rerun, true, 'Rerunning…');
    try {
      const result = await auditReleaseCandidate(view.release.id, platform, actor);
      savedReleases = [];
      await loadSavedReleases(true);
      setReleaseFeedback(`${platform.toUpperCase()} candidate re-audited: ${humanizeReason(result.audit.decision)}.`, result.audit.decision === 'no_go' ? 'error' : 'success');
    } catch (error) { setReleaseFeedback(`Audit failed: ${error}`, 'error'); }
    finally { setBusy(rerun, false); }
  });
  actions.append(copy, rerun);
  return actions;
}

function workStatus(resolution, waiver, evidence) {
  const status = document.createElement('div');
  status.className = 'mobile-release-work-status';
  if (resolution) status.appendChild(textElement('span', `RESOLVED · ${resolution.actor}`));
  if (waiver) status.appendChild(textElement('span', `WAIVED · until ${formatDate(waiver.expires_at)}`));
  if (evidence.length) status.appendChild(textElement('span', `${evidence.length} EVIDENCE LINK${evidence.length === 1 ? '' : 'S'}`));
  if (!status.childElementCount) status.appendChild(textElement('span', 'OPEN'));
  return status;
}

function evidenceAction(release, platform, finding) {
  const details = actionShell('Attach evidence');
  const title = inputElement('Evidence title', finding.title);
  const issuer = inputElement('Issuer', 'owner@company.com');
  const url = inputElement('Evidence URL (optional)', 'https://…');
  const actor = inputElement('Recorded by', 'engineer@company.com');
  issuer.input.value = operatorIdentity;
  actor.input.value = operatorIdentity;
  const submit = textElement('button', 'Attach verified evidence');
  submit.className = 'btn btn-primary';
  submit.addEventListener('click', async () => {
    if (!title.input.value.trim() || !issuer.input.value.trim() || !actor.input.value.trim()) {
      setReleaseFeedback('Evidence title, issuer, and recorder are required.', 'error');
      return;
    }
    setBusy(submit, true, 'Attaching…');
    try {
      const evidenceKind = finding.rule_id === 'MOB-EVD-002' ? 'rollback_plan' : 'release_owner_approval';
      await addReleaseEvidence(release, platform, finding.finding_instance_id, evidenceKind, title.input.value.trim(), issuer.input.value.trim(), url.input.value.trim() || null, null, actor.input.value.trim());
      await loadSavedReleases(true);
      setReleaseFeedback('Evidence attached to the active candidate and audit. Re-audit to recalculate the automated verdict.', 'success');
    } catch (error) { setReleaseFeedback(`Evidence failed: ${error}`, 'error'); }
    finally { setBusy(submit, false); }
  });
  details.lastElementChild.append(title.label, issuer.label, url.label, actor.label, submit);
  return details;
}

function resolutionAction(release, platform, finding, evidence) {
  const details = actionShell('Mark addressed');
  const reason = textAreaElement('Resolution summary', 'Describe the change and how it was verified.');
  const evidenceIds = inputElement('Evidence IDs (optional)', evidence.map(item => item.evidence_id).join(', '));
  evidenceIds.input.value = evidence.map(item => item.evidence_id).join(', ');
  const actor = inputElement('Resolved by', 'engineer@company.com');
  actor.input.value = operatorIdentity;
  const submit = textElement('button', 'Record resolution');
  submit.className = 'btn btn-primary';
  submit.addEventListener('click', async () => {
    if (!reason.input.value.trim() || !actor.input.value.trim()) {
      setReleaseFeedback('Resolution summary and engineer identity are required.', 'error');
      return;
    }
    setBusy(submit, true, 'Recording…');
    try {
      const ids = evidenceIds.input.value.split(',').map(value => value.trim()).filter(Boolean);
      await resolveReleaseFinding(release, platform, finding.finding_instance_id, reason.input.value.trim(), ids, actor.input.value.trim());
      await loadSavedReleases(true);
      setReleaseFeedback('Resolution recorded. Re-audit the candidate to prove the finding now passes.', 'success');
    } catch (error) { setReleaseFeedback(`Resolution failed: ${error}`, 'error'); }
    finally { setBusy(submit, false); }
  });
  details.lastElementChild.append(reason.label, evidenceIds.label, actor.label, submit);
  return details;
}

function waiverAction(release, platform, finding) {
  const details = actionShell('Accept risk');
  const reason = textAreaElement('Risk rationale', 'Why can this specific candidate proceed despite the finding?');
  const approver = inputElement('Approved by', 'release-manager@company.com');
  approver.input.value = operatorIdentity;
  const expiry = inputElement('Expires at (RFC 3339)', futureExpiry());
  expiry.input.value = futureExpiry();
  const submit = textElement('button', 'Record expiring waiver');
  submit.className = 'btn';
  submit.addEventListener('click', async () => {
    if (!reason.input.value.trim() || !approver.input.value.trim() || !expiry.input.value.trim()) {
      setReleaseFeedback('Risk rationale, approver, and expiry are required.', 'error');
      return;
    }
    setBusy(submit, true, 'Waiving…');
    try {
      await waiveReleaseFinding(release, platform, finding.finding_instance_id, reason.input.value.trim(), approver.input.value.trim(), expiry.input.value.trim());
      await loadSavedReleases(true);
      setReleaseFeedback('Expiring waiver recorded against this candidate, audit, policy, and finding.', 'success');
    } catch (error) { setReleaseFeedback(`Waiver failed: ${error}`, 'error'); }
    finally { setBusy(submit, false); }
  });
  details.lastElementChild.append(reason.label, approver.label, expiry.label, submit);
  return details;
}

function actionShell(label) {
  const details = document.createElement('details');
  details.className = 'mobile-release-action';
  details.append(textElement('summary', label));
  const fields = document.createElement('div');
  fields.className = 'mobile-release-action-form';
  details.appendChild(fields);
  return details;
}

function decisionPanel(view, reports = []) {
  const panel = document.createElement('section');
  panel.className = 'mobile-release-decision-form';
  panel.append(textElement('h3', 'Release-manager decision'), textElement('p', 'Review the exact candidate and remaining work before recording a candidate-bound decision. No-Go cannot be approved.'));
  const identity = inputElement('Decision-maker identity', 'email');
  identity.input.value = operatorIdentity;
  const role = inputElement('Role', 'release_manager');
  role.input.value = 'release_manager';
  const rationale = inputElement('Rationale', 'Explain why this candidate should or should not proceed.');
  const platformLabel = document.createElement('label');
  platformLabel.appendChild(textElement('span', 'Platform'));
  const platformSelect = document.createElement('select');
  platformSelect.className = 'form-input';
  Object.keys(view.release.active_audits || {}).forEach((platform) => {
    const option = document.createElement('option');
    option.value = platform;
    option.textContent = platform.toUpperCase();
    platformSelect.appendChild(option);
  });
  platformLabel.appendChild(platformSelect);
  const form = document.createElement('div');
  form.className = 'mobile-release-decision-fields';
  const activeDecision = Object.values(view.release.active_decisions || {})
    .map(id => view.decisions.find(decision => decision.id === id))
    .find(Boolean);
  if (activeDecision) {
    const active = document.createElement('div');
    active.className = 'mobile-active-decision';
    active.append(
      textElement('strong', `${activeDecision.decision.toUpperCase()} by ${activeDecision.decided_by}`),
      textElement('span', `${activeDecision.role} · ${formatDate(activeDecision.created_at)} · ${activeDecision.rationale}`)
    );
    const change = textElement('button', 'Change decision');
    change.className = 'btn';
    change.addEventListener('click', () => {
      form.hidden = false;
      change.hidden = true;
      identity.input.focus();
    });
    active.appendChild(change);
    panel.appendChild(active);
    form.hidden = true;
  }
  const review = document.createElement('div');
  review.className = 'mobile-decision-review';
  review.hidden = true;
  const reviewRows = document.createElement('dl');
  reviewRows.className = 'mobile-release-detail-rows';
  const readiness = document.createElement('ul');
  readiness.className = 'mobile-decision-readiness';
  const acknowledgement = document.createElement('label');
  acknowledgement.className = 'mobile-decision-ack';
  const acknowledgeInput = document.createElement('input');
  acknowledgeInput.type = 'checkbox';
  acknowledgement.append(acknowledgeInput, document.createTextNode(' I understand this approval may proceed while automated findings remain open.'));
  review.append(textElement('strong', 'Approval review'), reviewRows, textElement('strong', 'Decision readiness'), readiness, acknowledgement);
  const updateReview = () => {
    const platform = platformSelect.value;
    const candidate = view.candidates.find(item => item.id === view.release.active_candidates?.[platform]);
    const audit = view.audits.find(item => item.id === view.release.active_audits?.[platform]);
    const report = reports.find(item => item.report_fingerprint === audit?.report_fingerprint);
    const open = (report?.findings || []).filter(finding => finding.status !== 'pass').length;
    reviewRows.textContent = '';
    [
      ['Platform', platform.toUpperCase()],
      ['Candidate', candidate ? `${candidate.id} · ${shortHash(candidate.artifact_sha256)}` : 'Unavailable'],
      ['Automated decision', audit ? humanizeReason(audit.decision).toUpperCase() : 'NOT AUDITED'],
      ['Open findings', open],
      ['Policy', audit ? shortHash(audit.policy_digest) : 'Unavailable']
    ].forEach(([term, value]) => reviewRows.append(textElement('dt', term), textElement('dd', value)));
    const evidenceFindings = (report?.findings || []).filter(finding => finding.category === 'evidence' && finding.status !== 'pass');
    const linkedFindingIds = new Set(view.evidence_links.filter(link => link.platform === platform).map(link => link.finding_instance_id));
    const rollbackRequired = evidenceFindings.some(finding => finding.rule_id === 'MOB-EVD-002');
    const rollbackLinked = view.evidence_links.some(link => link.platform === platform && link.mobile_evidence_kind === 'rollback_plan');
    const targetTime = view.release.target_date ? Date.parse(`${view.release.target_date}T23:59:59Z`) : null;
    const waiversCoverTarget = !targetTime || view.waivers
      .filter(waiver => waiver.platform === platform && view.release.active_waiver_ids?.includes(waiver.id))
      .every(waiver => Date.parse(waiver.expires_at) > targetTime);
    readiness.textContent = '';
    [
      ['Candidate identity and audit verified', Boolean(candidate && audit)],
      ['Deterministic blockers cleared', Number(audit?.counts?.blockers || 0) === 0],
      ['Required human evidence attached', evidenceFindings.every(finding => linkedFindingIds.has(finding.finding_instance_id))],
      ['Rollback proof attached', !rollbackRequired || rollbackLinked],
      ['Waivers remain valid through target', waiversCoverTarget],
      ['Release target date recorded', Boolean(view.release.target_date)]
    ].forEach(([label, complete]) => {
      const item = textElement('li', `${complete ? '✓' : '○'} ${label}`);
      item.className = complete ? 'complete' : 'open';
      readiness.appendChild(item);
    });
    acknowledgement.hidden = audit?.decision !== 'at_risk';
    acknowledgeInput.checked = false;
  };
  const buttons = document.createElement('div');
  buttons.className = 'mobile-release-detail-actions';
  const approve = textElement('button', 'Review approval'); approve.className = 'btn btn-primary';
  const reject = textElement('button', 'Reject candidate'); reject.className = 'btn';
  approve.addEventListener('click', () => {
    if (review.hidden) {
      updateReview();
      review.hidden = false;
      approve.textContent = 'Confirm approval';
      review.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      return;
    }
    if (!acknowledgement.hidden && !acknowledgeInput.checked) {
      setReleaseFeedback('Acknowledge the remaining automated risk before approval.', 'error');
      acknowledgeInput.focus();
      return;
    }
    submitDecision(view.release.id, platformSelect.value, 'approved', identity.input, role.input, rationale.input, approve);
  });
  reject.addEventListener('click', () => submitDecision(view.release.id, platformSelect.value, 'rejected', identity.input, role.input, rationale.input, reject));
  platformSelect.addEventListener('change', () => {
    review.hidden = true;
    approve.textContent = 'Review approval';
  });
  buttons.append(approve, reject);
  form.append(platformLabel, identity.label, role.label, rationale.label, review, buttons);
  panel.appendChild(form);
  return panel;
}

async function submitDecision(release, platform, decision, identity, role, rationale, button) {
  if (!identity.value.trim() || !role.value.trim() || !rationale.value.trim()) {
    setReleaseFeedback('Identity, role, and rationale are required for an accountable decision.', 'error');
    return;
  }
  setBusy(button, true, decision === 'approved' ? 'Approving…' : 'Rejecting…');
  try {
    await decideRelease(release, platform, decision, identity.value.trim(), role.value.trim(), rationale.value.trim());
    savedReleases = [];
    await loadSavedReleases(true);
    setReleaseFeedback(`Candidate ${decision}. The decision is bound to the current audit.`, 'success');
  } catch (error) {
    setReleaseFeedback(`Decision failed: ${error}`, 'error');
  } finally { setBusy(button, false); }
}

async function writePacket(release, button) {
  setBusy(button, true, 'Writing packet…');
  try {
    const result = await writeReleasePacket(release);
    const handoff = document.getElementById('mobile-release-handoff');
    if (handoff) {
      handoff.hidden = false;
      const preview = handoff.querySelector('.mobile-release-handoff-body');
      if (preview && window.marked && window.DOMPurify) {
        preview.innerHTML = window.DOMPurify.sanitize(window.marked.parse(result.handoff_summary));
      } else if (preview) preview.textContent = result.handoff_summary;
    }
    setReleaseFeedback(`Handoff packet ${result.packet.packet_id} written to .gantry/${result.html_path}`, 'success');
  } catch (error) { setReleaseFeedback(`Packet failed: ${error}`, 'error'); }
  finally { setBusy(button, false); }
}

async function compareCandidates(release, button) {
  setBusy(button, true, 'Comparing…');
  try {
    const platform = Object.keys(release.active_candidates || {})[0];
    const diff = await diffReleaseCandidates(release.id, platform);
    setReleaseFeedback(`${diff.platform.toUpperCase()} diff: ${diff.introduced_findings.length} introduced, ${diff.resolved_findings.length} resolved, ${diff.changed_findings.length} changed.`, 'success');
  } catch (error) { setReleaseFeedback(`Comparison failed: ${error}`, 'error'); }
  finally { setBusy(button, false); }
}

function updateAuditMode() {
  const hasArtifact = Boolean(document.getElementById('mobile-artifact-path')?.value.trim());
  const mode = document.getElementById('mobile-audit-mode');
  const badge = document.getElementById('mobile-mode-badge');
  const title = document.getElementById('mobile-mode-title');
  const description = document.getElementById('mobile-mode-description');
  const button = document.getElementById('btn-mobile-audit');
  if (mode) mode.dataset.mode = hasArtifact ? 'candidate' : 'source';
  if (badge) badge.textContent = hasArtifact ? 'CANDIDATE-BOUND' : 'SOURCE READINESS';
  if (title) title.textContent = hasArtifact ? 'Audit the exact release candidate' : 'Start with a source readiness check';
  if (description) description.textContent = hasArtifact
    ? 'Gantry will fingerprint the file and bind every result to this build.'
    : 'Useful early in the release cycle. This is not a candidate-bound shipping decision.';
  if (button && !button.disabled) button.textContent = hasArtifact ? 'Audit exact candidate' : 'Run source readiness check';
  updateWorkflowSteps();
}

function handleAuditInputChange() {
  updateAuditMode();
  if (!lastReport) return;
  const current = document.getElementById('mobile-artifact-path')?.value.trim() || null;
  if (current !== auditedArtifactPath) markAuditResultStale('The candidate input changed after this check. Rerun before tracking, resolving, or approving it.');
}

function markAuditResultStale(message) {
  auditResultStale = true;
  const panel = document.getElementById('mobile-decision-panel');
  panel?.classList.add('stale');
  let banner = document.getElementById('mobile-stale-result');
  if (!banner && panel) {
    banner = document.createElement('div');
    banner.id = 'mobile-stale-result';
    banner.className = 'mobile-stale-result';
    panel.prepend(banner);
  }
  if (banner) banner.textContent = message;
  const track = document.getElementById('btn-mobile-track-release');
  if (track) track.disabled = true;
  setFeedback(message, 'warning');
}

function clearAuditResultStale() {
  auditResultStale = false;
  document.getElementById('mobile-decision-panel')?.classList.remove('stale');
  document.getElementById('mobile-stale-result')?.remove();
  const track = document.getElementById('btn-mobile-track-release');
  if (track) track.disabled = false;
}

function updateWorkflowSteps() {
  const source = document.querySelector('[data-mobile-step="source"]');
  const candidate = document.querySelector('[data-mobile-step="candidate"]');
  const resolve = document.querySelector('[data-mobile-step="resolve"]');
  [source, candidate, resolve].forEach((step) => step?.classList.remove('active', 'complete'));
  if (!lastReport) {
    source?.classList.add('active');
  } else if (!lastReport.artifact) {
    source?.classList.add('complete');
    candidate?.classList.add('active');
  } else {
    source?.classList.add('complete');
    candidate?.classList.add('complete');
    resolve?.classList.add('active');
  }
}

async function chooseArtifact() {
  const button = document.getElementById('btn-mobile-artifact');
  setBusy(button, true, 'Choosing…');
  setFeedback('Opening candidate picker…');
  try {
    const path = await pickMobileArtifact();
    if (path) {
      document.getElementById('mobile-artifact-path').value = path;
      setFeedback('Candidate selected. The audit will bind its verdict to this exact file.', 'success');
    } else {
      setFeedback('No candidate selected. You can still run a source readiness check.');
    }
    handleAuditInputChange();
  } catch (error) {
    setFeedback(`Candidate selection failed: ${error}`, 'error');
  } finally {
    setBusy(button, false);
  }
}

async function runAudit() {
  const button = document.getElementById('btn-mobile-audit');
  const cancelButton = document.getElementById('btn-mobile-audit-cancel');
  const artifactValue = document.getElementById('mobile-artifact-path')?.value.trim();
  const write = document.getElementById('mobile-write-report')?.checked ?? true;
  const isCandidate = Boolean(artifactValue);
  setBusy(button, true, isCandidate ? 'Auditing candidate…' : 'Checking source…');
  if (cancelButton) {
    cancelButton.hidden = false;
    cancelButton.disabled = false;
  }
  setFeedback(isCandidate
    ? 'Fingerprinting the candidate and inspecting signing, privacy, policy, and evidence…'
    : 'Inspecting first-party source, project configuration, policy, and evidence requirements…');
  try {
    const result = await runMobileAudit(artifactValue || null, write);
    lastReport = result.report || result;
    lastReportPath = result.report_path || null;
    auditedArtifactPath = artifactValue || null;
    clearAuditResultStale();
    findingFilter = 'attention';
    renderReport(lastReport, lastReportPath);
    renderOnboarding();
    setFeedback(write
      ? `Check complete. Content-addressed release packet saved at ${lastReportPath || '.gantry/mobile/audits/'}.`
      : 'Check complete. No files were written.', 'success');
  } catch (error) {
    setFeedback(`Release check failed: ${error}`, 'error');
  } finally {
    setBusy(button, false);
    if (cancelButton) cancelButton.hidden = true;
    updateAuditMode();
  }
}

async function cancelAudit() {
  const button = document.getElementById('btn-mobile-audit-cancel');
  if (button) button.disabled = true;
  try {
    const cancelled = await cancelMobileAudit();
    setFeedback(cancelled ? 'Stopping the release check safely…' : 'The release check has already finished.');
  } catch (error) {
    setFeedback(`Could not cancel the release check: ${error}`, 'error');
  }
}

async function preparePolicy() {
  const button = document.activeElement?.id === 'btn-mobile-policy-create'
    ? document.getElementById('btn-mobile-policy-create')
    : document.getElementById('btn-mobile-policy');
  const feedback = button?.id === 'btn-mobile-policy-create'
    ? document.getElementById('mobile-policy-feedback')
    : document.getElementById('mobile-feedback');
  setBusy(button, true, 'Preparing…');
  setFeedbackElement(feedback, 'Checking the repository mobile policy…');
  try {
    const path = await initMobilePolicy(false);
    rememberReleaseSetup({ policyPrepared: true });
    if (lastReport) markAuditResultStale('Policy preparation may have changed the evidence used by this result. Run the check again before tracking it.');
    setFeedbackElement(feedback, `Mobile policy is ready at ${path}. Existing policy was preserved.`, 'success');
    const status = document.getElementById('mobile-policy-status');
    if (status) {
      status.dataset.state = 'ready';
      const label = status.querySelector('strong');
      if (label) label.textContent = 'Policy ready';
    }
    renderOnboarding();
  } catch (error) {
    setFeedbackElement(feedback, `Policy setup failed: ${error}`, 'error');
  } finally {
    setBusy(button, false);
  }
}

async function openPolicyFile() {
  const button = document.getElementById('btn-mobile-policy-open');
  const feedback = document.getElementById('mobile-policy-feedback');
  setBusy(button, true, 'Opening…');
  try {
    const path = await openProjectFile('.gantry/mobile-policy.json');
    setFeedbackElement(feedback, `Opened ${path || '.gantry/mobile-policy.json'}.`, 'success');
  } catch (error) {
    setFeedbackElement(feedback, `Could not open the policy: ${error}`, 'error');
  } finally {
    setBusy(button, false);
  }
}

function setBusy(button, busy, label = '') {
  if (!button) return;
  if (busy) button.dataset.idleLabel = button.textContent;
  button.disabled = busy;
  button.setAttribute('aria-busy', String(busy));
  button.textContent = busy ? label : (button.dataset.idleLabel || button.textContent);
}

function setFeedback(message, kind = 'neutral') {
  setFeedbackElement(document.getElementById('mobile-feedback'), message, kind);
}

function setFeedbackElement(feedback, message, kind = 'neutral') {
  if (!feedback) return;
  feedback.textContent = message;
  feedback.dataset.kind = kind;
}

function renderReport(report, reportPath = null) {
  const decision = document.getElementById('mobile-decision');
  const normalized = String(report.decision || 'unknown').replaceAll('_', ' ').toUpperCase();
  if (decision) {
    decision.textContent = normalized;
    decision.dataset.decision = report.decision;
  }

  const attentionCount = countAttention(report.summary || {});
  const badge = document.getElementById('sidebar-mobile-badge');
  if (badge) {
    badge.style.display = attentionCount > 0 ? 'inline-flex' : 'none';
    badge.textContent = String(attentionCount);
    badge.title = `${attentionCount} mobile release item${attentionCount === 1 ? '' : 's'} need attention`;
  }

  renderSummary(report.summary || {});
  renderDecision(report, reportPath);
  renderProjectMeta(report);
  renderFindingFilters(report.findings || []);
  renderFindings(report.findings || []);
  renderTrackingPanel(report);
  updateWorkflowSteps();
}

function renderTrackingPanel(report) {
  const panel = document.getElementById('mobile-track-release');
  if (!panel) return;
  panel.hidden = !report.artifact;
  if (!report.artifact) return;
  const version = document.getElementById('mobile-track-version');
  const name = document.getElementById('mobile-track-name');
  const owner = document.getElementById('mobile-track-owner');
  if (version && !version.value) version.value = report.artifact.version || '';
  if (name && !name.value) name.value = `${report.artifact.version || 'Mobile'} release`;
  if (owner && !owner.value) owner.value = operatorIdentity;
  const button = document.getElementById('btn-mobile-track-release');
  if (button) button.disabled = auditResultStale;
}

async function trackAuditedCandidate() {
  const button = document.getElementById('btn-mobile-track-release');
  if (!lastReport?.artifact || auditResultStale) {
    setFeedback('Rerun the exact candidate audit before creating a tracked release.', 'error');
    return;
  }
  const version = document.getElementById('mobile-track-version')?.value.trim();
  const name = document.getElementById('mobile-track-name')?.value.trim();
  const owner = document.getElementById('mobile-track-owner')?.value.trim();
  const targetDate = document.getElementById('mobile-track-target-date')?.value || null;
  const artifactPath = document.getElementById('mobile-artifact-path')?.value.trim();
  if (!version || !owner || !artifactPath) {
    setFeedback('Release version, owner, and exact candidate path are required.', 'error');
    return;
  }
  setBusy(button, true, 'Tracking candidate…');
  try {
    const release = await ensureReleaseWorkflow(version, name || null, owner, operatorIdentity || owner);
    if (targetDate) await scheduleRelease(release.id, targetDate, null, null, operatorIdentity || owner);
    await addReleaseCandidate(release.id, artifactPath, lastReport.artifact.platform, operatorIdentity || owner);
    await auditReleaseCandidate(release.id, lastReport.artifact.platform, operatorIdentity || owner);
    selectedReleaseId = release.id;
    savedReleases = [];
    await openMobileWorkspace('releases', 'releases');
    await loadSavedReleases(true);
    setReleaseFeedback(`Candidate tracked as ${version}. Resolve evidence or request the accountable decision from this verified record.`, 'success');
  } catch (error) {
    setFeedback(`Could not track candidate: ${error}`, 'error');
  } finally { setBusy(button, false); }
}

function renderSummary(summary) {
  const container = document.getElementById('mobile-summary-grid');
  if (!container) return;
  container.textContent = '';
  [
    ['Passed', summary.passed || 0, 'pass'],
    ['Blockers', summary.blockers || 0, 'blocker'],
    ['Warnings', summary.warnings || 0, 'warning'],
    ['Human evidence', summary.manual || 0, 'manual'],
    ['Tool unavailable', summary.unavailable || 0, 'unavailable']
  ].forEach(([label, value, kind]) => {
    const card = document.createElement('div');
    card.className = `mobile-summary-card ${kind}`;
    const number = document.createElement('strong');
    number.textContent = String(value);
    const text = document.createElement('span');
    text.textContent = label;
    card.append(number, text);
    container.appendChild(card);
  });
}

function renderDecision(report, reportPath) {
  const panel = document.getElementById('mobile-decision-panel');
  const caption = document.getElementById('mobile-decision-caption');
  const reasons = document.getElementById('mobile-decision-reasons');
  const nextAction = document.getElementById('mobile-next-action');
  const context = document.getElementById('mobile-release-context');
  if (panel) panel.hidden = false;

  const isCandidate = Boolean(report.artifact);
  const decisionCopy = {
    go: isCandidate ? 'Candidate meets the current release policy.' : 'Source is ready; bind a candidate before shipping.',
    at_risk: 'Review the remaining evidence before approval.',
    no_go: 'Do not ship until blocking requirements are resolved.'
  };
  if (caption) caption.textContent = decisionCopy[report.decision] || 'Review the release report.';

  const decisionReasons = report.decision_reasons?.length
    ? report.decision_reasons
    : fallbackDecisionReasons(report.summary || {}, isCandidate);
  if (reasons) {
    reasons.textContent = '';
    decisionReasons.forEach((reason) => reasons.appendChild(textElement('li', humanizeReason(reason))));
  }

  const priorityFinding = [...(report.findings || [])]
    .sort((a, b) => findingRank(a) - findingRank(b) || a.id.localeCompare(b.id))
    .find((finding) => finding.status !== 'pass');
  if (nextAction) {
    nextAction.textContent = '';
    nextAction.appendChild(textElement('strong', 'Next best action'));
    nextAction.appendChild(document.createTextNode(priorityFinding?.remediation
      || (isCandidate ? 'Archive the packet with the release approval.' : 'Select the exact IPA, AAB, or APK intended for upload.')));
  }

  if (context) {
    context.textContent = '';
    const artifact = report.artifact;
    const candidate = artifact
      ? `${String(artifact.kind).toUpperCase()} · ${artifact.version || 'version ?'} (${artifact.build || 'build ?'})`
      : 'Source-only check';
    const gitState = `${shortHash(report.git_commit)}${report.git_branch ? ` · ${report.git_branch}` : ''}${report.git_dirty ? ' · dirty' : ''}`;
    const values = [
      ['Mode', artifact ? 'Candidate-bound' : 'Source readiness'],
      ['Candidate', candidate],
      ...(artifact?.sha256 ? [['Artifact SHA', `${artifact.sha256.slice(0, 16)}…`]] : []),
      ['Git', gitState],
      ['Policy', policyLabel(report)],
      ...(report.report_fingerprint ? [['Report ID', `${report.report_fingerprint.slice(0, 16)}…`]] : []),
      ...(reportPath ? [['Saved packet', reportPath]] : [])
    ];
    values.forEach(([term, value]) => {
      context.append(textElement('dt', term), textElement('dd', value));
    });
  }
}

function renderProjectMeta(report) {
  const container = document.getElementById('mobile-project-meta');
  if (!container) return;
  const stacks = (report.project?.stacks || []).join(', ') || 'unknown stack';
  const platforms = (report.project?.platforms || []).join(' + ') || 'no platform';
  container.textContent = `${stacks} · ${platforms} · ${(report.findings || []).length} checks`;
}

function renderFindingFilters(findings) {
  const toolbar = document.getElementById('mobile-findings-toolbar');
  if (toolbar) toolbar.hidden = false;
  const attention = findings.filter((finding) => finding.status !== 'pass').length;
  const passed = findings.filter((finding) => finding.status === 'pass').length;
  setText('mobile-filter-attention-count', attention);
  setText('mobile-filter-pass-count', passed);
  setText('mobile-filter-all-count', findings.length);
  document.querySelectorAll('[data-mobile-filter]').forEach((button) => {
    const active = button.dataset.mobileFilter === findingFilter;
    button.classList.toggle('active', active);
    button.setAttribute('aria-pressed', String(active));
  });
}

function setFindingFilter(filter) {
  findingFilter = filter;
  renderFindingFilters(lastReport?.findings || []);
  renderFindings(lastReport?.findings || []);
}

function renderFindings(findings) {
  const container = document.getElementById('mobile-findings');
  if (!container) return;
  container.textContent = '';
  const visible = findings
    .filter((finding) => findingFilter === 'all'
      || (findingFilter === 'pass' ? finding.status === 'pass' : finding.status !== 'pass'))
    .sort((a, b) => findingRank(a) - findingRank(b) || a.id.localeCompare(b.id));

  if (!visible.length) {
    const empty = document.createElement('div');
    empty.className = 'mobile-empty';
    empty.append(
      textElement('strong', findingFilter === 'attention' ? 'Nothing needs attention' : 'No findings in this view'),
      textElement('span', findingFilter === 'attention' ? 'Review passed checks or archive the release packet.' : 'Choose another filter to continue.')
    );
    container.appendChild(empty);
    return;
  }

  visible.forEach((finding) => container.appendChild(createFinding(finding)));
}

function createFinding(finding) {
  const row = document.createElement('article');
  row.className = `mobile-finding status-${finding.status}`;

  const status = document.createElement('span');
  status.className = 'mobile-finding-status';
  status.textContent = statusLabel(finding.status);

  const content = document.createElement('div');
  const rule = document.createElement('div');
  rule.className = 'mobile-finding-rule';
  rule.textContent = `${finding.rule_id || finding.id} · ${finding.category}${finding.subject_id ? ` · ${humanizeReason(finding.subject_id)}` : ''}`;
  const heading = textElement('h4', finding.title);
  const remediation = document.createElement('p');
  remediation.className = 'mobile-remediation';
  remediation.append(textElement('strong', finding.status === 'pass' ? 'Verified' : 'Next step'));
  remediation.appendChild(document.createTextNode(finding.status === 'pass'
    ? (finding.observed || finding.rationale || 'Requirement verified.')
    : (finding.remediation || 'Review this requirement.')));
  content.append(rule, heading, remediation);

  const evidence = evidenceRows(finding);
  if (evidence.length) {
    const details = document.createElement('details');
    details.className = 'mobile-finding-details';
    details.appendChild(textElement('summary', 'View evidence and rationale'));
    const list = document.createElement('dl');
    list.className = 'mobile-finding-evidence';
    evidence.forEach(([term, value]) => list.append(textElement('dt', term), textElement('dd', value)));
    details.appendChild(list);
    content.appendChild(details);
  }

  row.append(status, content);
  return row;
}

function evidenceRows(finding) {
  return [
    ['Observed', finding.observed],
    ['Expected', finding.expected],
    ['Why it matters', finding.rationale],
    ['Location', finding.locations?.join(', ')],
    ['Authority', finding.authoritative_source],
    ['Reviewed', finding.source_reviewed_at]
  ].filter(([, value]) => value !== null && value !== undefined && String(value).trim());
}

function statusLabel(status) {
  return ({ fail: 'BLOCKED', warning: 'REVIEW', manual: 'HUMAN PROOF', unavailable: 'NO TOOL', pass: 'PASSED' })[status]
    || String(status).toUpperCase();
}

function policyLabel(report) {
  const version = report.policy?.policy_pack_version || report.policy?.version || 'custom';
  const digest = report.policy_digest ? ` · ${report.policy_digest.slice(0, 10)}…` : '';
  return `${version}${digest}`;
}

function fallbackDecisionReasons(summary, isCandidate) {
  const reasons = [];
  if (summary.blockers) reasons.push(`${summary.blockers} blocking requirement${summary.blockers === 1 ? '' : 's'} failed`);
  if (summary.warnings) reasons.push(`${summary.warnings} warning${summary.warnings === 1 ? '' : 's'} requires review`);
  if (summary.manual) reasons.push(`${summary.manual} human evidence item${summary.manual === 1 ? '' : 's'} remain`);
  if (summary.unavailable) reasons.push(`${summary.unavailable} automated check${summary.unavailable === 1 ? ' is' : 's are'} unavailable`);
  if (!isCandidate) reasons.push('No exact release candidate was supplied');
  return reasons.length ? reasons : ['All evaluated release requirements passed'];
}

function countAttention(summary) {
  return (summary.blockers || 0) + (summary.warnings || 0) + (summary.manual || 0) + (summary.unavailable || 0);
}

function shortHash(value) {
  if (!value) return 'unknown';
  return value.length > 12 ? `${value.slice(0, 12)}…` : value;
}

function humanizeReason(value) {
  const text = String(value || '').replaceAll('_', ' ').trim();
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : text;
}

function textElement(tag, text) {
  const element = document.createElement(tag);
  element.textContent = text;
  return element;
}

function setText(id, value) {
  const element = document.getElementById(id);
  if (element) element.textContent = String(value);
}

function findingRank(finding) {
  if (finding.status === 'fail') return 0;
  if (finding.status === 'warning') return 1;
  if (finding.status === 'manual') return 2;
  if (finding.status === 'unavailable') return 3;
  return 4;
}

function stageBadge(stage) {
  const badge = textElement('span', humanizeReason(stage).toUpperCase());
  badge.className = 'mobile-release-stage';
  badge.dataset.stage = stage;
  return badge;
}

function detailRows(rows) {
  const list = document.createElement('dl');
  list.className = 'mobile-release-detail-rows';
  rows.forEach(([term, value]) => list.append(textElement('dt', term), textElement('dd', value)));
  return list;
}

function sectionList(title, items, formatter, emptyCopy) {
  const section = document.createElement('section');
  section.className = 'mobile-release-record-section';
  section.appendChild(textElement('h3', title));
  if (!items?.length) {
    section.appendChild(textElement('p', emptyCopy));
    return section;
  }
  const list = document.createElement('ul');
  items.forEach(item => list.appendChild(textElement('li', formatter(item))));
  section.appendChild(list);
  return section;
}

function decisionRecordLabel(view, decision) {
  const active = view.release.active_decisions?.[decision.platform] === decision.id;
  return `${active ? 'ACTIVE' : 'SUPERSEDED'} · ${decision.decision.toUpperCase()} · ${String(decision.platform).toUpperCase()} · ${decision.decided_by} (${decision.role})`;
}

function waiverRecordLabel(view, waiver) {
  const active = view.release.active_waiver_ids?.includes(waiver.id);
  const expired = new Date(waiver.expires_at).getTime() <= Date.now();
  const status = active ? 'ACTIVE' : expired ? 'EXPIRED' : 'SUPERSEDED';
  return `${status} · ${waiver.finding_instance_id} · ${waiver.approved_by} · expires ${formatDate(waiver.expires_at)}`;
}

function inputElement(labelText, placeholder) {
  const label = document.createElement('label');
  label.appendChild(textElement('span', labelText));
  const input = labelText === 'Rationale' ? document.createElement('textarea') : document.createElement('input');
  input.className = 'form-input';
  input.placeholder = placeholder;
  if (input.tagName === 'TEXTAREA') input.rows = 3;
  label.appendChild(input);
  return { label, input };
}

function dateInputElement(labelText, value = null) {
  const label = document.createElement('label');
  label.appendChild(textElement('span', labelText));
  const input = document.createElement('input');
  input.className = 'form-input';
  input.type = 'date';
  input.value = value || '';
  label.appendChild(input);
  return { label, input };
}

function textAreaElement(labelText, placeholder) {
  const label = document.createElement('label');
  label.appendChild(textElement('span', labelText));
  const input = document.createElement('textarea');
  input.className = 'form-input';
  input.placeholder = placeholder;
  input.rows = 3;
  label.appendChild(input);
  return { label, input };
}

function selectElement(labelText, options) {
  const label = document.createElement('label');
  label.appendChild(textElement('span', labelText));
  const input = document.createElement('select');
  input.className = 'form-input';
  options.forEach((value) => {
    const option = document.createElement('option');
    option.value = value;
    option.textContent = value.toUpperCase();
    input.appendChild(option);
  });
  label.appendChild(input);
  return { label, input };
}

function futureExpiry() {
  const date = new Date();
  date.setDate(date.getDate() + 14);
  return date.toISOString().replace(/\.\d{3}Z$/, 'Z');
}

function renderReleaseEmpty(title, copy) {
  const container = document.getElementById('mobile-release-detail');
  if (!container) return;
  container.textContent = '';
  const empty = document.createElement('div');
  empty.className = 'mobile-empty';
  empty.append(textElement('strong', title), textElement('span', copy));
  container.appendChild(empty);
}

function setReleaseFeedback(message, kind = 'neutral') {
  const feedback = document.getElementById('mobile-release-feedback');
  if (!feedback) return;
  feedback.textContent = message;
  feedback.dataset.kind = kind;
}

function formatDate(value) {
  if (!value) return 'unknown time';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
}

function formatDateOnly(value) {
  if (!value) return 'Unscheduled';
  const date = new Date(`${value}T00:00:00Z`);
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleDateString([], { dateStyle: 'medium', timeZone: 'UTC' });
}

function formatCalendarDay(value) {
  const date = new Date(`${value}T00:00:00Z`);
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleDateString([], { month: 'short', day: 'numeric', timeZone: 'UTC' });
}

function formatBytes(value) {
  const bytes = Number(value || 0);
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
