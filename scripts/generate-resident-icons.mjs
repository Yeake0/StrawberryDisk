/**
 * Derive application and tray icons from the approved transparent artwork.
 * Tauri supplies the rasterizer and native icon encoders.
 */
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync, copyFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = fileURLToPath(new URL('../', import.meta.url));
const sourcePath = join(root, 'public/strawberrydisk.png');
const source = readFileSync(sourcePath);
const template = `<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 1024 1024"><defs><filter id="monochrome" color-interpolation-filters="sRGB"><feColorMatrix type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 1 0"/></filter></defs><image width="1024" height="1024" href="data:image/png;base64,${source.toString('base64')}" filter="url(#monochrome)"/></svg>`;
const temporary = mkdtempSync(join(tmpdir(), 'strawberrydisk-icons-'));
const output = join(root, 'src-tauri/icons');
const cli = join(root, 'node_modules/@tauri-apps/cli/tauri.js');
function generate(input, directory, sizes = []) {
  execFileSync(
    process.execPath,
    [cli, 'icon', input, '--output', directory, ...sizes.flatMap(size => ['--png', String(size)])],
    {
      cwd: root,
      stdio: 'inherit',
    }
  );
}
try {
  const templateSource = join(temporary, 'template.svg');
  writeFileSync(templateSource, template);
  generate(templateSource, join(temporary, 'template'), [36]);
  generate(sourcePath, join(temporary, 'color'), [64]);
  generate(sourcePath, join(temporary, 'application'));
  copyFileSync(join(temporary, 'template/36x36.png'), join(output, 'tray-template.png'));
  copyFileSync(join(temporary, 'color/64x64.png'), join(output, 'tray-color.png'));
  for (const name of ['32x32.png', '128x128.png', '128x128@2x.png', 'icon.png', 'icon.ico', 'icon.icns']) {
    copyFileSync(join(temporary, 'application', name), join(output, name));
  }
  mkdirSync(join(output, 'windows'), { recursive: true });
  // Tauri embeds the first ICO entry as the live window icon. Put the largest
  // raster first to avoid enlarging 32 px pixels on high-DPI taskbars. Windows
  // still selects the appropriate size from the full executable icon directory.
  const ico = readFileSync(join(temporary, 'application/icon.ico'));
  const count = ico.readUInt16LE(4);
  const entries = Array.from({ length: count }, (_, index) => ico.subarray(6 + index * 16, 22 + index * 16));
  entries.sort((left, right) => (right[0] || 256) - (left[0] || 256));
  writeFileSync(
    join(output, 'windows/icon.ico'),
    Buffer.concat([ico.subarray(0, 6), ...entries, ico.subarray(6 + count * 16)])
  );
  console.log('Generated StrawberryDisk application and tray icons.');
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
