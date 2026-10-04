import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const collect = fileURLToPath(new URL('../scripts/ci/collect-desktop.mjs', import.meta.url));

for (const [target, platform, extensions] of [
  ['x86_64-unknown-linux-gnu', 'linux-x64', ['deb', 'AppImage']],
  ['aarch64-apple-darwin', 'macos-arm64', ['dmg']],
  ['x86_64-pc-windows-msvc', 'windows-x64', ['exe']]
] as const) {
  test(`${platform} publishes stable names without changing installer bytes`, () => {
    const root = mkdtempSync(join(tmpdir(), 'nockster-release-'));
    try {
      const source =
        platform === 'macos-arm64'
          ? join(root, 'release/desktop')
          : join(root, 'apps/desktop/src-tauri/target', target, 'release/bundle/installers');
      mkdirSync(source, { recursive: true });
      for (const extension of extensions)
        writeFileSync(join(source, `Nockster_1.2.3.${extension}`), `signed installer ${extension}`);
      execFileSync(process.execPath, [collect, target, platform], { cwd: root });
      // Repeated collection produces the same aliases, not duplicate candidates.
      execFileSync(process.execPath, [collect, target, platform], { cwd: root });
      for (const extension of extensions) {
        const filename = `nockster-${platform}-latest.${extension}`;
        const bytes = readFileSync(join(root, 'release/desktop', filename));
        assert.equal(bytes.toString(), `signed installer ${extension}`);
        const digest = createHash('sha256').update(bytes).digest('hex');
        assert.equal(
          readFileSync(join(root, 'release/desktop', `${filename}.sha256`), 'utf8'),
          `${digest}  ${filename}\n`
        );
      }
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}

test('missing installers and mismatched architectures cannot become latest downloads', () => {
  const root = mkdtempSync(join(tmpdir(), 'nockster-release-'));
  try {
    mkdirSync(join(root, 'apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle'), {
      recursive: true
    });
    for (const platform of ['linux-x64', 'macos-arm64'])
      assert.throws(() =>
        execFileSync(process.execPath, [collect, 'x86_64-unknown-linux-gnu', platform], {
          cwd: root,
          stdio: 'pipe'
        })
      );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
