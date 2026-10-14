import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const [assetDirectory, outputPath] = process.argv.slice(2);
if (!assetDirectory || !outputPath) {
  throw new Error('Usage: node generate-release-checksums.mjs <asset-directory> <output-path>');
}

const version = JSON.parse(fs.readFileSync('package.json', 'utf8')).version;
const distributableAssets = [
  'gantry-linux-x86_64',
  'gantry-macos-aarch64',
  'gantry-macos-x86_64',
  'gantry-windows-x86_64.exe',
  `gantry-v${version}-x86_64-unknown-linux-gnu.tar.gz`,
  `gantry-v${version}-aarch64-apple-darwin.tar.gz`,
  `gantry-v${version}-x86_64-apple-darwin.tar.gz`,
  `gantry-v${version}-x86_64-pc-windows-msvc.zip`,
  `Gantry_${version}_aarch64.dmg`,
  `Gantry_${version}_x64.dmg`,
];

const missing = distributableAssets.filter(name => !fs.existsSync(path.join(assetDirectory, name)));
if (missing.length) {
  throw new Error(`Cannot generate SHA256SUMS; missing distributable assets: ${missing.join(', ')}`);
}

const manifest = distributableAssets
  .sort()
  .map(name => {
    const contents = fs.readFileSync(path.join(assetDirectory, name));
    const digest = crypto.createHash('sha256').update(contents).digest('hex');
    return `${digest}  ${name}`;
  })
  .join('\n');
fs.writeFileSync(outputPath, `${manifest}\n`);
process.stdout.write(`Wrote ${outputPath} with ${distributableAssets.length} verified downloads.\n`);
