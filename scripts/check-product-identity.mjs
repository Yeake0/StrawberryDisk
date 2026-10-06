/**
 * Verifies that public product names, bundle identifiers, binary names, and
 * required brand assets remain consistent across the frontend, Tauri, and CLI
 * entry points.
 *
 * Run with `pnpm check:identity`. The command throws with the mismatched field
 * or missing asset when a product identity regression is detected.
 */
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const projectRoot = process.cwd();

function readJson(relativePath) {
  return JSON.parse(readFileSync(join(projectRoot, relativePath), 'utf8'));
}

function readText(relativePath) {
  return readFileSync(join(projectRoot, relativePath), 'utf8');
}

function assertEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(`${label} must be ${JSON.stringify(expected)}, received ${JSON.stringify(actual)}.`);
  }
}

function assertContains(content, expected, label) {
  if (!content.includes(expected)) {
    throw new Error(`${label} must contain ${JSON.stringify(expected)}.`);
  }
}

function assertExists(relativePath) {
  if (!existsSync(join(projectRoot, relativePath))) {
    throw new Error(`Required product asset is missing: ${relativePath}.`);
  }
}

const packageJson = readJson('package.json');
const tauriConfig = readJson('src-tauri/tauri.conf.json');
const tauriManifest = readText('src-tauri/Cargo.toml');
const cliManifest = readText('src-tauri/crates/strawberrydisk-cli/Cargo.toml');
const readme = readText('README.md');
const indexHtml = readText('index.html');
const tauriLibrary = readText('src-tauri/src/lib.rs');
const coreLibrary = readText('src-tauri/crates/strawberrydisk-core/src/lib.rs');
const tauriMain = readText('src-tauri/src/main.rs');
const macosChangeTracking = readText('src-tauri/crates/strawberrydisk-platform/src/macos/change_tracking.rs');

assertEqual(packageJson.name, 'strawberrydisk', 'npm package name');
assertEqual(tauriConfig.productName, 'StrawberryDisk', 'Tauri product name');
assertEqual(tauriConfig.identifier, 'app.strawberrydisk.desktop', 'Tauri bundle identifier');
assertContains(
  coreLibrary,
  'pub const APPLICATION_IDENTIFIER: &str = "app.strawberrydisk.desktop";',
  'Core application identifier'
);
assertEqual(tauriConfig.mainBinaryName, 'StrawberryDisk', 'Tauri binary name');
assertEqual(tauriConfig.app?.windows?.[0]?.title, 'StrawberryDisk', 'main window title');

assertContains(tauriManifest, 'name = "strawberrydisk"', 'Tauri package manifest');
assertContains(tauriManifest, 'name = "strawberrydisk_lib"', 'Tauri library manifest');
assertContains(cliManifest, 'name = "strawberrydisk-cli"', 'CLI package manifest');
assertContains(cliManifest, 'name = "strawberrydisk"', 'CLI binary manifest');

assertExists('public/strawberrydisk.png');
assertExists('public/strawberrydisk-source.png');
assertExists('src/components/icons/md-icon-strawberrydisk.vue');

assertContains(readme, 'strawberrydisk clean', 'README CLI usage');
assertContains(indexHtml, 'StrawberryDisk', 'HTML application shell');
assertContains(tauriLibrary, 'StrawberryDisk', 'Tauri library');
assertContains(tauriMain, 'strawberrydisk_lib', 'Tauri entry point');
assertContains(macosChangeTracking, 'app.strawberrydisk.cache-dirty-monitor', 'macOS cache monitor identity');

console.log('Product identity is consistent.');
