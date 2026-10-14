import { expect, test } from '@playwright/test';

function collectPageFailures(page) {
  const failures = [];
  page.on('pageerror', error => failures.push(`pageerror: ${error.message}`));
  page.on('console', message => {
    if (message.type() === 'error') failures.push(`console: ${message.text()}`);
  });
  return failures;
}

test('engineer can run a rendered source-readiness audit', async ({ page }) => {
  const failures = collectPageFailures(page);
  await page.goto('/', { waitUntil: 'domcontentloaded' });

  await expect(page.getByRole('heading', { name: 'Release Portfolio', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Open Candidate Audit', exact: true }).click();
  await expect(page.locator('#mobile-decision')).toHaveText('NOT RUN');
  await expect(page.locator('#btn-mobile-audit-cancel')).toBeHidden();

  await page.getByRole('button', { name: 'Run source readiness check', exact: true }).click();
  await expect(page.locator('#mobile-decision')).toHaveText('AT RISK');
  await expect(page.locator('#mobile-feedback')).toContainText('Check complete');
  await expect(page.getByRole('heading', { name: 'Why this decision', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Release work queue', exact: true })).toBeVisible();
  await expect(page.locator('#mobile-summary-grid')).toContainText('Human evidence');
  await expect(page.locator('#mobile-findings article')).toHaveCount(4);
  expect(failures).toEqual([]);
});

test('engineer can review and manage the real policy workspace', async ({ page }) => {
  const failures = collectPageFailures(page);
  await page.goto('/', { waitUntil: 'domcontentloaded' });

  await page.getByRole('button', { name: 'Open Release Policies', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Release policies', exact: true })).toBeVisible();
  await expect(page.locator('#mobile-policy-workspace')).toContainText('Automated failures');
  await expect(page.locator('#mobile-policy-workspace')).toContainText('Candidate integrity');
  await page.getByRole('button', { name: 'Create or verify policy', exact: true }).click();
  await expect(page.locator('#mobile-policy-feedback')).toContainText('.gantry/mobile-policy.json');
  expect(failures).toEqual([]);
});

test('CI guidance matches the candidate-bound Action contract', async ({ page }) => {
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Open CI and Integrations', exact: true }).click();
  const snippet = page.locator('#snippet-github');
  await expect(snippet).toContainText('uses: trygantry/gantry@v1');
  await expect(snippet).toContainText('mobile-artifact:');
  await expect(snippet).toContainText('release-version:');
  await expect(snippet).toContainText('release-owner:');
  await expect(snippet).toContainText('fail-on-no-go: true');
  await expect(page.locator('#view-ci-setup')).toContainText('Require the check');
});

test('Settings makes the open and paid product boundary explicit', async ({ page }) => {
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Open Settings', exact: true }).click();
  const community = page.locator('[data-product-edition="community"]');
  await expect(community).toContainText('Gantry Community');
  await expect(community).toContainText('Open source · Apache-2.0');
  await expect(community).toContainText('No Gantry account is required');
  await expect(community).toContainText('No hosted Gantry transmission');
  await expect(community).toHaveAttribute('data-availability', 'available');

  const cloud = page.locator('[data-product-edition="cloud"]');
  await expect(cloud).toContainText('Coming soon');
  await expect(cloud).toContainText('separately distributed commercial product');
  await expect(cloud).toContainText('does not transmit data to a hosted Gantry service');
  await expect(cloud).toHaveAttribute('data-availability', 'coming_soon');
  await expect(page.getByRole('button', { name: /Connect Gantry Cloud/i })).toHaveCount(0);
});

test('release workspace remains readable at a narrow effective viewport', async ({ page }) => {
  await page.setViewportSize({ width: 480, height: 640 });
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  const geometry = await page.evaluate(() => ({
    viewport: document.documentElement.clientWidth,
    app: document.querySelector('.app-container').getBoundingClientRect().right,
    mainLeft: document.querySelector('.main-content').getBoundingClientRect().left,
    muted: getComputedStyle(document.documentElement).getPropertyValue('--text-muted').trim(),
    card: getComputedStyle(document.documentElement).getPropertyValue('--bg-card').trim(),
  }));
  expect(geometry.app).toBeLessThanOrEqual(geometry.viewport + 1);
  expect(geometry.mainLeft).toBe(72);

  const luminance = hex => {
    const channels = hex.match(/[a-f\d]{2}/gi).map(value => parseInt(value, 16) / 255)
      .map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
    return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
  };
  const contrast = (first, second) => {
    const [lighter, darker] = [luminance(first), luminance(second)].sort((a, b) => b - a);
    return (lighter + 0.05) / (darker + 0.05);
  };
  expect(contrast(geometry.muted, geometry.card)).toBeGreaterThanOrEqual(4.5);
  await expect(page.getByRole('heading', { name: 'Release portfolio', exact: true })).toBeVisible();
});

test('release manager can search, filter, and switch the rendered portfolio', async ({ page }) => {
  const failures = collectPageFailures(page);
  await page.goto('/', { waitUntil: 'domcontentloaded' });

  await expect(page.locator('#mobile-release-list .mobile-release-list-item')).toHaveCount(2);
  await page.locator('#mobile-release-search').fill('3.5.0-beta');
  await expect(page.locator('#mobile-release-list .mobile-release-list-item')).toHaveCount(1);
  await expect(page.locator('#mobile-release-list')).toContainText('3.5.0-beta');
  await expect(page.locator('#mobile-release-detail')).toContainText('iOS beta candidate');

  await page.locator('#mobile-release-search').fill('');
  await page.getByRole('button', { name: 'Approvals', exact: true }).click();
  await expect(page.locator('#mobile-release-review-queue .mobile-release-review-item')).toHaveCount(1);
  await expect(page.locator('#mobile-release-review-queue')).toContainText('Human approval required');

  await page.getByRole('button', { name: 'Calendar', exact: true }).click();
  await expect(page.locator('#mobile-release-list')).toHaveClass(/calendar/);
  await expect(page.locator('#mobile-release-list .mobile-release-calendar-item')).toHaveCount(2);
  expect(failures).toEqual([]);
});

test('hidden dialogs stay out of the accessibility tree', async ({ page }) => {
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  const dialogs = page.locator('[role="dialog"]');
  const count = await dialogs.count();
  expect(count).toBeGreaterThan(0);
  for (let index = 0; index < count; index += 1) {
    const dialog = dialogs.nth(index);
    await expect(dialog).toBeHidden();
    await expect(dialog).toHaveAttribute('aria-hidden', 'true');
    await expect(dialog).toHaveAttribute('inert', '');
  }
});

test('administrator can run vault health from the rendered settings view', async ({ page }) => {
  const failures = collectPageFailures(page);
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Open Settings', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Vault Health & Recovery', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Check vault', exact: true }).click();
  await expect(page.locator('#vault-operations-status')).toContainText('Vault healthy');
  await expect(page.locator('#vault-operations-status')).toHaveAttribute('data-kind', 'success');
  expect(failures).toEqual([]);
});
