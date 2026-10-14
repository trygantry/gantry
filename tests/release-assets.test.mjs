import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const version = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).version;
const tag = `v${version}`;
const cliAssets = [
  'gantry-linux-x86_64',
  'gantry-macos-aarch64',
  'gantry-macos-x86_64',
  'gantry-windows-x86_64.exe',
];
const archiveAssets = [
  `gantry-v${version}-x86_64-unknown-linux-gnu.tar.gz`,
  `gantry-v${version}-aarch64-apple-darwin.tar.gz`,
  `gantry-v${version}-x86_64-apple-darwin.tar.gz`,
  `gantry-v${version}-x86_64-pc-windows-msvc.zip`,
];
const checksumAssets = [...cliAssets, ...archiveAssets];
const otherAssets = [
  'gantry-cli.cdx.json',
  'gantry-core.cdx.json',
  'gantry.cdx.json',
  `Gantry_${version}_aarch64.dmg`,
  `Gantry_${version}_x64.dmg`,
  'Gantry_aarch64.app.tar.gz',
  'Gantry_aarch64.app.tar.gz.sig',
  'Gantry_x64.app.tar.gz',
  'Gantry_x64.app.tar.gz.sig',
  'latest.json',
];

function createFixture() {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'gantry-release-assets-'));
  const assets = path.join(directory, 'assets');
  fs.mkdirSync(assets);

  for (const name of checksumAssets) {
    const contents = Buffer.from(`verified-${name}`);
    fs.writeFileSync(path.join(assets, name), contents);
    const digest = crypto.createHash('sha256').update(contents).digest('hex');
    fs.writeFileSync(path.join(assets, `${name}.sha256`), `${digest}  ${name}\n`);
  }
  for (const name of otherAssets.filter(name => name !== 'latest.json')) {
    fs.writeFileSync(path.join(assets, name), `verified-${name}`);
  }
  const manifestResult = spawnSync(
    process.execPath,
    ['scripts/ci/generate-release-checksums.mjs', assets, path.join(assets, 'SHA256SUMS')],
    { cwd: root, encoding: 'utf8' },
  );
  assert.equal(manifestResult.status, 0, manifestResult.stderr);
  fs.writeFileSync(path.join(assets, 'latest.json'), JSON.stringify({
    version,
    platforms: {
      'darwin-aarch64': {
        signature: 'verified-signature',
        url: `https://github.com/trygantry/gantry/releases/download/${tag}/Gantry_aarch64.app.tar.gz`,
      },
      'darwin-x86_64': {
        signature: 'verified-signature',
        url: `https://github.com/trygantry/gantry/releases/download/${tag}/Gantry_x64.app.tar.gz`,
      },
    },
  }));

  const names = fs.readdirSync(assets);
  const metadata = {
    tag_name: tag,
    draft: true,
    prerelease: false,
    assets: names.map(name => ({
      name,
      state: 'uploaded',
      size: fs.statSync(path.join(assets, name)).size,
    })),
  };
  const metadataPath = path.join(directory, 'release.json');
  fs.writeFileSync(metadataPath, JSON.stringify(metadata));
  return { assets, directory, metadataPath };
}

function runVerifier(metadataPath, assets, expectedDesktopMode) {
  return spawnSync(
    process.execPath,
    ['scripts/ci/verify-release-assets.mjs', metadataPath, assets],
    {
      cwd: root,
      encoding: 'utf8',
      env: expectedDesktopMode
        ? { ...process.env, EXPECTED_DESKTOP_MODE: expectedDesktopMode }
        : process.env,
    },
  );
}

test('accepts a complete draft with verified checksums and updater entries', () => {
  const fixture = createFixture();
  try {
    const result = runVerifier(fixture.metadataPath, fixture.assets);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Verified complete draft/);
  } finally {
    fs.rmSync(fixture.directory, { recursive: true, force: true });
  }
});

test('unsigned release mode rejects updater artifacts even when otherwise valid', () => {
  const fixture = createFixture();
  try {
    const result = runVerifier(fixture.metadataPath, fixture.assets, 'unsigned');
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Unsigned release unexpectedly contains updater assets/);
  } finally {
    fs.rmSync(fixture.directory, { recursive: true, force: true });
  }
});

test('accepts unsigned desktop bundles without updater artifacts', () => {
  const fixture = createFixture();
  try {
    for (const name of [
      'Gantry_aarch64.app.tar.gz',
      'Gantry_aarch64.app.tar.gz.sig',
      'Gantry_x64.app.tar.gz',
      'Gantry_x64.app.tar.gz.sig',
      'latest.json',
    ]) {
      fs.rmSync(path.join(fixture.assets, name));
    }
    const metadata = JSON.parse(fs.readFileSync(fixture.metadataPath, 'utf8'));
    metadata.assets = metadata.assets.filter(asset => fs.existsSync(path.join(fixture.assets, asset.name)));
    fs.writeFileSync(fixture.metadataPath, JSON.stringify(metadata));

    const result = runVerifier(fixture.metadataPath, fixture.assets, 'unsigned');
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /unsigned desktop build/);
    assert.match(result.stdout, /Verified complete draft/);
  } finally {
    fs.rmSync(fixture.directory, { recursive: true, force: true });
  }
});

test('fails closed when a required artifact is missing', () => {
  const fixture = createFixture();
  try {
    fs.rmSync(path.join(fixture.assets, 'gantry-cli.cdx.json'));
    const result = runVerifier(fixture.metadataPath, fixture.assets);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /download missing: gantry-cli\.cdx\.json/);
  } finally {
    fs.rmSync(fixture.directory, { recursive: true, force: true });
  }
});

test('fails closed when the aggregate checksum manifest is wrong', () => {
  const fixture = createFixture();
  try {
    const manifestPath = path.join(fixture.assets, 'SHA256SUMS');
    const manifest = fs.readFileSync(manifestPath, 'utf8');
    fs.writeFileSync(manifestPath, manifest.replace(/^[0-9a-f]{64}/, '0'.repeat(64)));
    const result = runVerifier(fixture.metadataPath, fixture.assets);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /SHA256SUMS mismatch/);
  } finally {
    fs.rmSync(fixture.directory, { recursive: true, force: true });
  }
});
