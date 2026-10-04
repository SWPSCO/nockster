import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import * as core from '../packages/wallet/src/pkg/nockster_core.js';
import * as iris from '@nockbox/iris-wasm/iris_wasm.js';

core.initSync({
  module: readFileSync(new URL('../packages/wallet/src/pkg/nockster_core_bg.wasm', import.meta.url))
});
iris.initSync({
  module: readFileSync(
    new URL('../node_modules/@nockbox/iris-wasm/iris_wasm_bg.wasm', import.meta.url)
  )
});
const fixture = JSON.parse(
  readFileSync(new URL('./fixtures/nockster-signed.json', import.meta.url), 'utf8')
);

test('Iris hashes the on-chain input independently of reconstructed API metadata', () => {
  for (const disclosed of [false, true]) {
    for (const bridge of [false, true]) {
      const note = structuredClone(fixture.note);
      note.assets = 200_000 * 65536;
      if (!disclosed) note.noteData.lock.source = 'reconstructed';
      const recipients = bridge
        ? [
            {
              address: 'AcsPkuhXQoGeEsF91yynpm1kcW17PQ2Z1MEozgx7YnDPkZwrtzLuuqd',
              gift: 100_000 * 65536,
              bridgeEvmAddress: '0x1234567890abcdef1234567890abcdef12345678'
            }
          ]
        : fixture.recipients;
      const built = core.composeUnsignedTx(fixture.source, [note], recipients, 153880);
      const raw = iris.rawTxV1FromNoun(
        iris.cue(Buffer.from(core.toRawTxJam(built.base64Tx), 'base64'))
      );
      assert.equal(raw.spends.length, 1);
      const [name, spend] = raw.spends[0];
      assert.equal(name.first, note.firstName);
      assert.equal(name.last, note.lastName);
      const lock = iris.spendConditionNewPkh(iris.pkhSingle(fixture.source));
      const parent = {
        version: 1,
        origin_page: note.originPage,
        name,
        assets: String(note.assets),
        note_data: disclosed ? [['lock', ['0', iris.spendConditionToNoun(lock)]]] : []
      };
      const expectedHash = iris.noteV1Hash(parent);
      assert.ok(spend.seeds.length > 0);
      for (const seed of spend.seeds) assert.equal(seed.parent_hash, expectedHash);
    }
  }
});
