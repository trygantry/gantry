import fs from 'node:fs';
import { execFileSync } from 'node:child_process';

const tag = process.argv[2] || process.env.RELEASE_TAG || process.env.GITHUB_REF_NAME;
if (!tag || !/^v\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(tag)) {
  throw new Error(`Expected a semantic release tag such as v0.5.0; received ${tag || '<missing>'}`);
}

const expected = tag.slice(1);
const packageVersion = JSON.parse(fs.readFileSync('package.json', 'utf8')).version;
const packageLock = JSON.parse(fs.readFileSync('package-lock.json', 'utf8'));
const tauriVersion = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
const cargoToml = fs.readFileSync('Cargo.toml', 'utf8');
const workspaceVersion = cargoToml.match(/\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/)?.[1];
const cargoMetadata = JSON.parse(execFileSync(
  'cargo',
  ['metadata', '--locked', '--no-deps', '--format-version', '1'],
  { encoding: 'utf8' },
));
const workspacePackages = new Set(['gantry', 'gantry-cli', 'gantry-core']);
const cargoPackageVersions = cargoMetadata.packages
  .filter(pkg => workspacePackages.has(pkg.name))
  .map(pkg => `${pkg.name}=${pkg.version}`)
  .join(', ');

const versions = {
  'Git tag': expected,
  'package.json': packageVersion,
  'package-lock.json': packageLock.version,
  'package-lock.json root package': packageLock.packages?.['']?.version,
  'src-tauri/tauri.conf.json': tauriVersion,
  'Cargo.toml workspace': workspaceVersion,
  'Cargo workspace packages': cargoMetadata.packages
    .filter(pkg => workspacePackages.has(pkg.name))
    .every(pkg => pkg.version === expected) ? expected : cargoPackageVersions,
};

const mismatches = Object.entries(versions).filter(([, version]) => version !== expected);
if (mismatches.length > 0) {
  const detail = Object.entries(versions)
    .map(([source, version]) => `${source}=${version || '<missing>'}`)
    .join(', ');
  throw new Error(`Release version mismatch: ${detail}`);
}

const changelog = fs.readFileSync('CHANGELOG.md', 'utf8');
if (!changelog.includes(`## [${expected}] - `)) {
  throw new Error(`CHANGELOG.md does not contain a dated ${expected} release section.`);
}

process.stdout.write(`Verified release version ${expected} across tag, npm, lockfiles, Tauri, Cargo, and changelog.\n`);
