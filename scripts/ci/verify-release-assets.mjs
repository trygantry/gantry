import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const [metadataPath, assetDirectory] = process.argv.slice(2);
if (!metadataPath || !assetDirectory) {
  throw new Error('Usage: node verify-release-assets.mjs <release-metadata.json> <asset-directory>');
}
const expectedDesktopMode = process.env.EXPECTED_DESKTOP_MODE || 'either';
if (!['either', 'unsigned'].includes(expectedDesktopMode)) {
  throw new Error(`Unsupported EXPECTED_DESKTOP_MODE: ${expectedDesktopMode}`);
}

const packageVersion = JSON.parse(fs.readFileSync('package.json', 'utf8')).version;
const expectedTag = `v${packageVersion}`;
const release = JSON.parse(fs.readFileSync(metadataPath, 'utf8'));

if (release.tag_name !== expectedTag) {
  throw new Error(`Expected release tag ${expectedTag}; received ${release.tag_name || '<missing>'}.`);
}
if (release.draft !== true) {
  throw new Error('The automated release must remain a draft until manual packaged-build review.');
}
if (release.prerelease !== false) {
  throw new Error('The release metadata unexpectedly marks this stable candidate as a prerelease.');
}

const cliAssets = [
  'gantry-linux-x86_64',
  'gantry-macos-aarch64',
  'gantry-macos-x86_64',
  'gantry-windows-x86_64.exe',
];
const archiveAssets = [
  `gantry-v${packageVersion}-x86_64-unknown-linux-gnu.tar.gz`,
  `gantry-v${packageVersion}-aarch64-apple-darwin.tar.gz`,
  `gantry-v${packageVersion}-x86_64-apple-darwin.tar.gz`,
  `gantry-v${packageVersion}-x86_64-pc-windows-msvc.zip`,
];
const desktopAssets = [
  `Gantry_${packageVersion}_aarch64.dmg`,
  `Gantry_${packageVersion}_x64.dmg`,
];
const checksumAssets = [...cliAssets, ...archiveAssets];
const distributableAssets = [...checksumAssets, ...desktopAssets];
const requiredAssets = [
  ...checksumAssets.flatMap(name => [name, `${name}.sha256`]),
  'gantry-cli.cdx.json',
  'gantry-core.cdx.json',
  'gantry.cdx.json',
  ...desktopAssets,
  'SHA256SUMS',
];

const metadataAssets = new Map((release.assets || []).map(asset => [asset.name, asset]));
const missingMetadata = requiredAssets.filter(name => !metadataAssets.has(name));
const missingDownloads = requiredAssets.filter(name => !fs.existsSync(path.join(assetDirectory, name)));
if (missingMetadata.length || missingDownloads.length) {
  throw new Error([
    missingMetadata.length ? `release metadata missing: ${missingMetadata.join(', ')}` : '',
    missingDownloads.length ? `download missing: ${missingDownloads.join(', ')}` : '',
  ].filter(Boolean).join('; '));
}

for (const name of requiredAssets) {
  const metadata = metadataAssets.get(name);
  const downloaded = fs.statSync(path.join(assetDirectory, name));
  if (!metadata || metadata.state !== 'uploaded' || metadata.size <= 0 || downloaded.size <= 0) {
    throw new Error(`Release asset ${name} is empty or not fully uploaded.`);
  }
}

for (const name of checksumAssets) {
  const binary = fs.readFileSync(path.join(assetDirectory, name));
  const expectedLine = fs.readFileSync(path.join(assetDirectory, `${name}.sha256`), 'utf8').trim();
  const match = expectedLine.match(/^([0-9a-f]{64})\s+\*?(.+)$/i);
  if (!match || match[2] !== name) {
    throw new Error(`Malformed checksum file for ${name}.`);
  }
  const actual = crypto.createHash('sha256').update(binary).digest('hex');
  if (actual !== match[1].toLowerCase()) {
    throw new Error(`Checksum mismatch for ${name}.`);
  }
}

const manifestLines = fs.readFileSync(path.join(assetDirectory, 'SHA256SUMS'), 'utf8')
  .trim()
  .split(/\r?\n/)
  .filter(Boolean);
const manifest = new Map();
for (const line of manifestLines) {
  const match = line.match(/^([0-9a-f]{64})\s+\*?([^/\\]+)$/i);
  if (!match || manifest.has(match?.[2])) {
    throw new Error('SHA256SUMS is malformed or contains duplicate/path-qualified entries.');
  }
  manifest.set(match[2], match[1].toLowerCase());
}
for (const name of distributableAssets) {
  const expected = manifest.get(name);
  if (!expected) {
    throw new Error(`SHA256SUMS does not cover ${name}.`);
  }
  const actual = crypto.createHash('sha256')
    .update(fs.readFileSync(path.join(assetDirectory, name)))
    .digest('hex');
  if (actual !== expected) {
    throw new Error(`SHA256SUMS mismatch for ${name}.`);
  }
}
if (manifest.size !== distributableAssets.length) {
  throw new Error('SHA256SUMS contains unexpected download entries.');
}

const updaterAssets = [...metadataAssets.keys()].filter(name => (
  name === 'latest.json'
  || name.endsWith('.app.tar.gz')
  || name.endsWith('.app.tar.gz.sig')
));
if (expectedDesktopMode === 'unsigned' && updaterAssets.length) {
  throw new Error(`Unsigned release unexpectedly contains updater assets: ${updaterAssets.join(', ')}.`);
}

const updaterPath = path.join(assetDirectory, 'latest.json');
if (fs.existsSync(updaterPath)) {
  const updater = JSON.parse(fs.readFileSync(updaterPath, 'utf8'));
  if (updater.version !== packageVersion) {
    throw new Error(`Updater manifest version must be ${packageVersion}; received ${updater.version || '<missing>'}.`);
  }
  for (const platform of ['darwin-aarch64', 'darwin-x86_64']) {
    const entry = updater.platforms?.[platform];
    if (!entry?.signature || !entry?.url?.includes(`/download/${expectedTag}/`)) {
      throw new Error(`Updater manifest is missing a signed ${platform} entry for ${expectedTag}.`);
    }
  }
} else {
  process.stdout.write('Updater manifest latest.json absent (unsigned desktop build); skipping updater manifest checks.\n');
}

const unsupportedDesktopAssets = [...metadataAssets.keys()].filter(name => (
  name.endsWith('.msi')
  || name.endsWith('.AppImage')
  || name.endsWith('.deb')
  || name.endsWith('.rpm')
  || name.endsWith('-setup.exe')
));
if (unsupportedDesktopAssets.length) {
  throw new Error(`Unsupported Windows/Linux desktop assets are present: ${unsupportedDesktopAssets.join(', ')}.`);
}

process.stdout.write(`Verified complete draft ${expectedTag}: raw binaries, normalized archives, aggregate checksums, DMGs, and SBOMs validated.\n`);
