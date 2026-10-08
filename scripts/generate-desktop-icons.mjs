import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const output = mkdtempSync(join(tmpdir(), 'nockster-desktop-icons-'));
try {
  execFileSync(
    process.execPath,
    [
      join(root, 'node_modules/@tauri-apps/cli/tauri.js'),
      'icon',
      join(root, 'packages/wallet/public/nockster-logo.svg'),
      '--output',
      output
    ],
    { stdio: 'pipe' }
  );
  for (const file of ['32x32.png', '128x128.png', '128x128@2x.png', 'icon.ico', 'icon.icns'])
    copyFileSync(join(output, file), join(root, 'apps/desktop/src-tauri/icons', file));
  console.log('Generated desktop icons from the circle-N logo.');
} finally {
  rmSync(output, { recursive: true, force: true });
}
