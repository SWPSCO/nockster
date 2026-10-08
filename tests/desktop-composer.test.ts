import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  initSync,
  is_valid_pkh,
  compose_tx_v1_unsigned,
  compose_tx_v1_min_inputs,
  review_draft,
  compose_tx_v1_recipient_address,
  merge_signed_tx
} from '../nockster-esp/crates/nockster-wasm/pkg/nockster_wasm.js';
import {
  initSync as initCore,
  inspectTxJam,
  toRawTxJam,
  buildVault,
  importWallet,
  exportWallet,
  signTxJam,
  verifyPartialSignedDraft,
  verifySignedDraft,
  lockVault
} from '../packages/wallet/src/pkg/nockster_core.js';
import * as iris from '@nockbox/iris-wasm/iris_wasm.js';
import {
  newOutput,
  outputFromForm,
  amountInNicks,
  multisig,
  validateNotes,
  htlcBranches,
  type Note
} from '../apps/desktop/src/transactions/composer.ts';

initSync({
  module: readFileSync(
    new URL('../nockster-esp/crates/nockster-wasm/pkg/nockster_wasm_bg.wasm', import.meta.url)
  )
});
initCore({
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
const recipient = fixture.recipients[0].address;
const note: Note = {
  name_first: fixture.note.firstName,
  name_last: fixture.note.lastName,
  origin_page: fixture.note.originPage,
  assets: fixture.note.assets,
  version: 1
};

test('composer validates nick precision, multisig thresholds, duplicate notes, and lock parameters', () => {
  assert.equal(amountInNicks('0.0000152587890625', 'NOCK'), 1);
  assert.equal(amountInNicks('65,536', 'NOCK'), 4294967296);
  assert.equal(amountInNicks('65536', 'nicks'), 65536);
  for (const value of ['', '-1', '0', 'NaN', '1e6', '9007199254740992'])
    assert.throws(() => amountInNicks(value, 'nicks'));
  assert.throws(() => multisig('2', fixture.source, is_valid_pkh), /exceed/);
  assert.throws(
    () => multisig('1', `${fixture.source}\n${fixture.source}`, is_valid_pkh),
    /unique/
  );
  assert.throws(() => validateNotes([note, note], is_valid_pkh), /once/);
  assert.throws(
    () => validateNotes([{ ...note, assets: Number.MAX_SAFE_INTEGER + 1 }], is_valid_pkh),
    /whole/
  );
  assert.throws(() => validateNotes([{ ...note, version: 0 }], is_valid_pkh), /V1/);
  assert.throws(
    () =>
      outputFromForm({ ...newOutput(1), address: 'invalid', amount: '10' }, 'NOCK', is_valid_pkh),
    /recipient/
  );
  assert.throws(
    () =>
      outputFromForm(
        { ...newOutput(1), address: recipient, amount: '10', condition: 'timelock', height: '1.5' },
        'NOCK',
        is_valid_pkh
      ),
    /whole/
  );
});

for (const condition of ['plain', 'timelock', 'hashlock', 'htlc', 'burn'] as const) {
  test(`composer ${condition} outputs round-trip through the transaction engine`, () => {
    const output = outputFromForm(
      {
        ...newOutput(1),
        address: recipient,
        amount: '10',
        condition,
        height: '60000',
        commitments: fixture.source,
        refundAddress: fixture.source,
        refundHeight: '70000'
      },
      'NOCK',
      is_valid_pkh
    );
    const tx = compose_tx_v1_unsigned({
      source_pkh: fixture.source,
      notes: [{ ...note, assets: 655360000 }],
      outputs: [output],
      current_height: fixture.height
    });
    try {
      const inspected = inspectTxJam(Buffer.from(tx.wallet_jam).toString('base64'));
      assert.equal(inspected.spends.length, 1);
      const review = review_draft(tx.wallet_jam, fixture.source);
      const paid = review.outputs.find((item: { gift: number }) => item.gift === 655360);
      assert.ok(paid);
      if (condition === 'timelock')
        assert.ok(
          paid.lock.some(
            (lock: { kind: string; abs_min: number }) =>
              lock.kind === 'timelock' && lock.abs_min === 60000
          )
        );
      if (condition === 'hashlock')
        assert.ok(paid.lock.some((lock: { kind: string }) => lock.kind === 'hax'));
      if (condition === 'burn')
        assert.ok(paid.lock.some((lock: { kind: string }) => lock.kind === 'burn'));
      if (condition === 'htlc') assert.equal(paid.or_lock, 2);
      assert.equal(review.external_total + review.refund_total + review.fee_total, 655360000);
    } finally {
      tx.free();
    }
  });
}

test('multisig output composition preserves the required signer count', () => {
  const lock = multisig('2', `${fixture.source}\n${recipient}`, is_valid_pkh);
  assert.equal(is_valid_pkh(compose_tx_v1_recipient_address(lock)), true);
  const output = outputFromForm(
    {
      ...newOutput(1),
      amount: '10',
      recipientKind: 'multisig',
      signers: lock.pkhs.join('\n'),
      threshold: '2'
    },
    'NOCK',
    is_valid_pkh
  );
  const tx = compose_tx_v1_unsigned({
    source_pkh: fixture.source,
    notes: [{ ...note, assets: 655360000 }],
    outputs: [output],
    current_height: fixture.height
  });
  try {
    const inspected = inspectTxJam(Buffer.from(tx.wallet_jam).toString('base64'));
    assert.equal(inspected.spends.length, 1);
    const review = review_draft(tx.wallet_jam, fixture.source);
    assert.ok(
      review.outputs.some((item: { lock: Array<{ kind: string; m: number; n: number }> }) =>
        item.lock?.some(lock => lock.kind === 'pkh' && lock.m === 2 && lock.n === 2)
      )
    );
  } finally {
    tx.free();
  }
  assert.deepEqual(htlcBranches(recipient, fixture.source, fixture.source, '70000', is_valid_pkh), [
    { recipient, hashlock: [fixture.source] },
    { recipient: fixture.source, timelock: { abs_min: 70000 } }
  ]);
});

test('multisig copies combine verified signatures without changing the approved transaction', () => {
  const created = buildVault('multisig test password');
  const first = importWallet(
    created.vault,
    created.vaultKey,
    'first',
    'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float'
  );
  const vault = importWallet(first.vault, first.vaultKey, 'second', 'abandon '.repeat(23) + 'art');
  try {
    const addresses = ['first', 'second'].map(
      name => exportWallet(vault.vault, vault.vaultKey, name).wallet.publicKey
    );
    const source = { m: 2, pkhs: addresses };
    const firstName = iris.spendConditionFirstName(
      iris.spendConditionNewPkh(iris.pkhNew(2n, addresses))
    );
    const tx = compose_tx_v1_unsigned({
      source_pkh: compose_tx_v1_recipient_address(source),
      source_multisig: source,
      notes: [{ ...note, name_first: firstName, assets: 655360000 }],
      outputs: [{ recipient, amount: 655360 }],
      current_height: fixture.height
    });
    try {
      const draft = Buffer.from(tx.wallet_jam).toString('base64');
      const a = signTxJam(vault.vault, vault.vaultKey, 'first', draft).base64Tx;
      const b = signTxJam(vault.vault, vault.vaultKey, 'second', draft).base64Tx;
      verifyPartialSignedDraft(draft, a);
      assert.equal(inspectTxJam(a).spends[0].isFullySigned, false);
      assert.throws(() => verifySignedDraft(draft, a));
      const merged = Buffer.from(
        merge_signed_tx(Buffer.from(a, 'base64'), Buffer.from(b, 'base64'))
      ).toString('base64');
      verifySignedDraft(draft, merged);
      assert.deepEqual(inspectTxJam(merged).spends[0].signedBy.sort(), addresses.sort());
      assert.throws(() => verifyPartialSignedDraft(draft, fixture.signedTx));
    } finally {
      tx.free();
    }
  } finally {
    lockVault(vault.vault, vault.vaultKey);
  }
});

test('branching output locks validate their roots for every supported tree size', () => {
  for (const size of [2, 4, 8, 16]) {
    const tx = compose_tx_v1_unsigned({
      source_pkh: fixture.source,
      notes: [{ ...note, assets: 655360000 }],
      outputs: [
        {
          recipient,
          amount: 655360,
          or_branches: Array.from({ length: size }, (_, index) => ({
            recipient,
            timelock: { abs_min: 60000 + index }
          }))
        }
      ],
      current_height: fixture.height
    });
    try {
      const encoded = Buffer.from(tx.wallet_jam).toString('base64');
      assert.equal(inspectTxJam(encoded).outputs.length, 2);
      const raw = iris.rawTxV1FromNoun(iris.cue(Buffer.from(toRawTxJam(encoded), 'base64')));
      raw.spends[0][1].seeds[0].lock_root = fixture.source;
      const forged = iris.jam(iris.rawTxV1ToNoun(iris.rawTxV1New(raw.spends)));
      assert.throws(
        () => inspectTxJam(Buffer.from(forged).toString('base64')),
        /lock data does not match/
      );
    } finally {
      tx.free();
    }
  }
});

test('largest-first selection covers the fee with the fewest input notes and conserves amounts', () => {
  const notes = [80, 100, 90].map((amount, index) => ({
    ...note,
    name_last: iris.hashU64(BigInt(index + 100)),
    assets: amount * 65536
  }));
  const compose = (amount: number) =>
    compose_tx_v1_min_inputs({
      source_pkh: fixture.source,
      notes,
      outputs: [{ recipient, amount: amount * 65536 }],
      current_height: fixture.height
    });
  for (const [amount, count] of [
    [10, 1],
    [70, 2]
  ]) {
    const tx = compose(amount);
    try {
      const summary = JSON.parse(tx.summary_json);
      assert.equal(summary.inputs_used.length, count);
      assert.equal(summary.inputs_used[0].assets, 100 * 65536);
      if (count === 2) assert.equal(summary.inputs_used[1].assets, 90 * 65536);
      const review = review_draft(tx.wallet_jam, fixture.source);
      assert.equal(
        review.external_total + review.refund_total + review.fee_total,
        summary.inputs_used.reduce((sum: number, input: Note) => sum + input.assets, 0)
      );
      assert.equal(
        inspectTxJam(Buffer.from(tx.wallet_jam).toString('base64')).spends.length,
        count
      );
    } finally {
      tx.free();
    }
  }
});

test('largest-first selection splits outputs across notes without adding avoidable inputs', () => {
  const notes = [1000, 700, 500].map((amount, index) => ({
    ...note,
    name_last: iris.hashU64(BigInt(index + 200)),
    assets: amount * 65536
  }));
  const tx = compose_tx_v1_min_inputs({
    source_pkh: fixture.source,
    notes,
    outputs: [
      { recipient, amount: 900 * 65536 },
      { recipient: fixture.source, amount: 650 * 65536 }
    ],
    current_height: fixture.height
  });
  try {
    const summary = JSON.parse(tx.summary_json);
    assert.equal(summary.inputs_used.length, 2);
    assert.equal(inspectTxJam(Buffer.from(tx.wallet_jam).toString('base64')).spends.length, 2);
  } finally {
    tx.free();
  }
});
