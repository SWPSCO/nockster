import { writeFileSync } from 'node:fs';
import { releaseMetadata } from './plan.mjs';
import { publicKey } from './desktop-updates.mjs';

const { version } = releaseMetadata(process.env.NOCKSTER_VERSION, 1, 1);
const pubkey = process.env.NOCKSTER_DESKTOP_UPDATER_PUBLIC_KEY?.trim();
publicKey(pubkey);
writeFileSync(
  'apps/desktop/src-tauri/tauri.ci.conf.json',
  JSON.stringify({ version, plugins: { updater: { pubkey } } })
);
