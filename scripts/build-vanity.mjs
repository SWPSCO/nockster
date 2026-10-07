import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const cargo = process.env.CARGO || 'cargo';
const metadata = JSON.parse(
  execFileSync(
    cargo,
    [
      '+nightly-2025-02-14',
      'metadata',
      '--locked',
      '--format-version',
      '1',
      '--manifest-path',
      resolve(root, 'packages/wallet-engine/Cargo.toml')
    ],
    { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 }
  )
);
const library = metadata.packages.find(pkg => pkg.name === 'tx-types');
if (!library) throw new Error('The wallet engine must depend on tx-types.');
const source = resolve(dirname(library.manifest_path), '..');
const revision = library.source?.split('#')[1];
if (!/^[a-f0-9]{40}$/.test(revision ?? ''))
  throw new Error('The wallet engine must pin tx-types to a Git revision.');
execFileSync(
  process.env.PYTHON || 'python3',
  [
    resolve(source, 'vanity/build-browser.py'),
    '--out-dir',
    resolve(root, 'packages/wallet/public/vanity')
  ],
  {
    cwd: source,
    stdio: 'inherit',
    env: {
      ...process.env,
      CARGO: cargo,
      RUSTUP_TOOLCHAIN: 'nightly-2025-02-14',
      CARGO_TARGET_DIR: resolve(root, 'packages/wallet-engine/target/vanity', revision)
    }
  }
);
