import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import {
  initSync,
  buildVault,
  importWallet,
  exportWallet,
  lockVault,
  unlockVault,
  composeUnsignedTx,
  verifySignedDraft,
  inspectTxJam,
  cheetahPkhB58
} from '../packages/wallet/src/pkg/nockster_core.js';

initSync({ module: readFileSync(new URL('../packages/wallet/src/pkg/nockster_core_bg.wasm', import.meta.url)) });
const mnemonic =
  'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float';

test('vault import, lock, unlock and export preserve signing keys', () => {
  const built = buildVault('nockster test password');
  const imported = importWallet(built.vault, built.vaultKey, 'test', mnemonic);
  const before = exportWallet(imported.vault, imported.vaultKey, 'test').wallet;
  assert.equal(before.publicKey, '4Lu3cSW34WPwvDkTwKh7xB6yMZrvh3bFW7w26UDJVdboxADGkr2bTnL');
  lockVault(imported.vault, imported.vaultKey);
  assert.throws(() => exportWallet(imported.vault, imported.vaultKey, 'test'));
  assert.throws(() => unlockVault('incorrect', imported.vault));
  const unlocked = unlockVault('nockster test password', imported.vault);
  const after = exportWallet(unlocked.vault, unlocked.vaultKey, 'test').wallet;
  assert.deepEqual(after, before);
  lockVault(unlocked.vault, unlocked.vaultKey);
});

test('Nockster drafts round-trip through the Nockster firmware signing core', () => {
  const fixture = JSON.parse(
    readFileSync(new URL('./fixtures/nockster-signed.json', import.meta.url), 'utf8')
  );
  assert.equal(cheetahPkhB58(fixture.publicKey.x, fixture.publicKey.y), fixture.source);
  const draft = composeUnsignedTx(
    fixture.source,
    [fixture.note],
    fixture.recipients,
    fixture.height
  );
  assert.equal(draft.base64Tx, fixture.draft);
  assert.equal(draft.feePaid, fixture.fee);
  verifySignedDraft(draft.base64Tx, fixture.signedTx);
  const signed = inspectTxJam(fixture.signedTx);
  assert.equal(signed.spends.length, 1);
  assert.equal(signed.spends[0].isFullySigned, true);
  assert.deepEqual(signed.spends[0].signedBy, [fixture.source]);
  assert.equal(signed.feePaid, fixture.fee);
  assert.throws(() => verifySignedDraft(draft.base64Tx, draft.base64Tx));
  const altered = composeUnsignedTx(
    fixture.source,
    [fixture.note],
    [{ ...fixture.recipients[0], gift: fixture.recipients[0].gift + 1 }],
    fixture.height
  );
  assert.throws(() => verifySignedDraft(altered.base64Tx, fixture.signedTx));
});
