import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const root = path.resolve(import.meta.dirname, '..');
const installer = path.join(root, 'install.sh');

function fixtureFor(asset) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'gantry-installer-'));
  const releases = path.join(directory, 'release');
  const home = path.join(directory, 'home');
  fs.mkdirSync(releases);
  fs.mkdirSync(home);
  const payload = Buffer.from('#!/bin/sh\necho "gantry 0.5.0"\n');
  fs.writeFileSync(path.join(releases, asset), payload, { mode: 0o755 });
  const digest = crypto.createHash('sha256').update(payload).digest('hex');
  fs.writeFileSync(path.join(releases, `${asset}.sha256`), `${digest}  ${asset}\n`);
  return { directory, downloadBase: pathToFileURL(releases).href, home };
}

test('Unix installer verifies and normalizes a macOS CLI download', () => {
  const fixture = fixtureFor('gantry-macos-aarch64');
  try {
    const result = spawnSync('sh', [installer], {
      encoding: 'utf8',
      env: {
        ...process.env,
        GANTRY_ARCH: 'arm64',
        GANTRY_DOWNLOAD_BASE: fixture.downloadBase,
        GANTRY_OS: 'Darwin',
        HOME: fixture.home,
        PATH: process.env.PATH,
      },
    });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Installed Gantry/);
    assert.match(result.stdout, /gantry 0\.5\.0/);
    const installed = path.join(fixture.home, '.local', 'bin', 'gantry');
    assert.equal(fs.statSync(installed).mode & 0o111, 0o111);
  } finally {
    fs.rmSync(fixture.directory, { recursive: true, force: true });
  }
});

test('Unix installer fails clearly on an unsupported Linux architecture', () => {
  const result = spawnSync('sh', [installer], {
    encoding: 'utf8',
    env: {
      ...process.env,
      GANTRY_ARCH: 'aarch64',
      GANTRY_OS: 'Linux',
    },
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Linux CLI for x86_64 only/);
});

test('PowerShell installer verifies SHA-256 and installs without administrator access', () => {
  const source = fs.readFileSync(path.join(root, 'install.ps1'), 'utf8');
  assert.match(source, /Get-FileHash -Algorithm SHA256/);
  assert.match(source, /LOCALAPPDATA.*Programs\\Gantry\\bin/);
  assert.match(source, /gantry-windows-x86_64\.exe/);
  assert.match(source, /SetEnvironmentVariable\("Path"/);
});
