import { writeFileSync } from 'node:fs';
import { releaseMetadata } from './plan.mjs';

const { version } = releaseMetadata(process.env.NOCKSTER_VERSION, 1, 1);
writeFileSync('apps/desktop/src-tauri/tauri.ci.conf.json', JSON.stringify({ version }));
