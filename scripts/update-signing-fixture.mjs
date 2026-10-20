import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { initSync, composeUnsignedTx, verifySignedDraft, inspectTxJam } from '../packages/wallet/src/pkg/nockster_core.js';

const root = fileURLToPath(new URL('../', import.meta.url));
const path = join(root, 'tests/fixtures/nockster-signed.json');
const fixture = JSON.parse(readFileSync(path, 'utf8'));
initSync({ module: readFileSync(join(root, 'packages/wallet/src/pkg/nockster_core_bg.wasm')) });
const draft = composeUnsignedTx(fixture.source, [fixture.note], fixture.recipients, fixture.height);
const temporary = mkdtempSync(join(tmpdir(), 'nockster-signing-fixture-'));
try {
  const input = join(temporary, 'draft.jam');
  const output = join(temporary, 'signed.jam');
  writeFileSync(input, Buffer.from(draft.base64Tx, 'base64'));
  execFileSync('cargo', ['run', '-p', 'nockster-core', '--example', 'sign_test_fixture', '--', input, output, String(fixture.height)], {
    cwd: join(root, 'nockster-esp'), stdio: 'inherit'
  });
  const signed = readFileSync(output).toString('base64');
  verifySignedDraft(draft.base64Tx, signed);
  const inspected = inspectTxJam(signed);
  if (!inspected.spends.every(spend => spend.isFullySigned && spend.signedBy.includes(fixture.source)))
    throw new Error('fixture must be fully signed by its public test key');
  fixture.draft = draft.base64Tx;
  fixture.fee = draft.feePaid;
  fixture.signedTx = signed;
  fixture.signer = {
    implementation: 'nockster-core',
    source: 'nockster-esp/crates/nockster-core/src/draft_sign/tx_v1.rs',
    generator: 'scripts/update-signing-fixture.mjs'
  };
  writeFileSync(path, JSON.stringify(fixture, null, 2) + '\n');
  console.log(`Updated verified signing fixture; fee ${fixture.fee} nicks at height ${fixture.height}.`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
