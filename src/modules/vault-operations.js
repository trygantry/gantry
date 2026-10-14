import {
  checkVaultHealth,
  previewVaultMigration,
  writeSupportBundle
} from './api.js';

let initialized = false;

function setStatus(message, kind = 'neutral') {
  const status = document.getElementById('vault-operations-status');
  if (!status) return;
  status.textContent = message;
  status.dataset.kind = kind;
}

async function run(button, operation) {
  if (!button) return;
  button.disabled = true;
  setStatus('Working…');
  try {
    await operation();
  } catch (error) {
    setStatus(String(error), 'error');
  } finally {
    button.disabled = false;
  }
}

export function initializeVaultOperations() {
  if (initialized) return;
  initialized = true;

  const checkButton = document.getElementById('btn-vault-check');
  checkButton?.addEventListener('click', () => run(checkButton, async () => {
    const report = await checkVaultHealth(false);
    const detail = `${report.verified_releases} release(s) verified · ${report.pending_transactions} pending transaction(s)`;
    setStatus(
      report.healthy
        ? `Vault healthy. ${detail}.`
        : `Vault needs attention. ${detail}. ${report.errors.join(' ')}`,
      report.healthy ? 'success' : 'error'
    );
  }));

  const repairButton = document.getElementById('btn-vault-repair');
  repairButton?.addEventListener('click', () => run(repairButton, async () => {
    const report = await checkVaultHealth(true);
    setStatus(
      report.healthy
        ? `Vault healthy. Recovered ${report.repaired_transactions} transaction(s).`
        : `Repair could not restore vault health. ${report.errors.join(' ')}`,
      report.healthy ? 'success' : 'error'
    );
  }));

  const migrationButton = document.getElementById('btn-vault-migration');
  migrationButton?.addEventListener('click', () => run(migrationButton, async () => {
    const report = await previewVaultMigration();
    setStatus(
      report.migrations_required === 0
        ? `Schema ${report.current_release_schema} is current; no migration is required.`
        : `${report.migrations_required} migration(s) would be required. No files were changed.`,
      'success'
    );
  }));

  const supportButton = document.getElementById('btn-vault-support');
  supportButton?.addEventListener('click', () => run(supportButton, async () => {
    const result = await writeSupportBundle();
    setStatus(
      `Redacted support bundle written to ${result.path}. Review it before sharing.`,
      'success'
    );
  }));
}
