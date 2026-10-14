import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const root = path.resolve(import.meta.dirname, '..');
const version = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).version;

for (const schema of [
  'mobile-policy.schema.json',
  'mobile-audit-report.schema.json',
]) {
  test(`gantry-core packaged ${schema} matches the public schema`, () => {
    const publicSchema = fs.readFileSync(path.join(root, 'schemas', schema));
    const packagedSchema = fs.readFileSync(path.join(root, 'crates', 'gantry-core', 'schemas', schema));
    assert.deepEqual(packagedSchema, publicSchema);
  });
}

test('explicit release tag takes precedence over GitHub reserved ref variables', () => {
  const result = spawnSync(
    process.execPath,
    ['scripts/ci/verify-release-version.mjs', `v${version}`],
    {
      cwd: root,
      encoding: 'utf8',
      env: { ...process.env, GITHUB_REF_NAME: 'main' },
    },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, new RegExp(`Verified release version ${version.replaceAll('.', '\\.')}`));
});
