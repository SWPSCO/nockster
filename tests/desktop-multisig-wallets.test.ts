import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  initSync,
  is_valid_pkh,
  compose_tx_v1_recipient_address
} from '../nockster-esp/crates/nockster-wasm/pkg/nockster_wasm.js';
import {
  addMultisigs,
  findMultisig,
  multisigFromSetup,
  removeMultisig,
  savedMultisig,
  setupFile,
  setupFileName,
  storedMultisigs,
  type MultisigLock
} from '../apps/desktop/src/transactions/multisig-wallets.ts';

// Stand-ins for the WASM engine: any "pkh-…" string is a valid signer, and a
// lock's address is a readable digest of its threshold and signers in order.
const valid = (value: string) => /^pkh-[a-z]+$/.test(value);
const lockAddress = (lock: MultisigLock) => `lock:${lock.m}:${lock.pkhs.join('+')}`;
const make = (name: string, m: string, signers: string) =>
  savedMultisig(name, m, signers, valid, lockAddress);

test('a saved multisig carries its lock and derived address', () => {
  assert.deepEqual(make('  Treasury   fund ', '2', 'pkh-a\npkh-b, pkh-c'), {
    name: 'Treasury fund',
    m: 2,
    pkhs: ['pkh-a', 'pkh-b', 'pkh-c'],
    address: 'lock:2:pkh-a+pkh-b+pkh-c'
  });
  assert.throws(() => make('', '1', 'pkh-a'), /name/);
  assert.throws(() => make('x'.repeat(65), '1', 'pkh-a'), /64/);
  assert.throws(() => make('T', '3', 'pkh-a\npkh-b'), /exceed/);
  assert.throws(() => make('T', '1', 'pkh-a\npkh-a'), /unique/);
  assert.throws(() => make('T', '1', 'nope'), /valid/);
});

test('one entry per lock, unique names, sorted by name', () => {
  const treasury = make('Treasury', '2', 'pkh-a\npkh-b');
  const ops = make('Ops', '1', 'pkh-a\npkh-c');
  let list = addMultisigs([], [treasury, ops]);
  assert.deepEqual(
    list.map(item => item.name),
    ['Ops', 'Treasury']
  );
  // Saving the same lock again renames it instead of duplicating it.
  list = addMultisigs(list, [make('Reserve', '2', 'pkh-a\npkh-b')]);
  assert.deepEqual(
    list.map(item => item.name),
    ['Ops', 'Reserve']
  );
  assert.throws(() => addMultisigs(list, [make('Ops', '2', 'pkh-b\npkh-c')]), /already named/);
  assert.deepEqual(removeMultisig(list, ops.address), [list[1]]);
  assert.equal(findMultisig(list, '1', 'pkh-a pkh-c', valid, lockAddress)?.name, 'Ops');
  assert.equal(findMultisig(list, '2', 'pkh-a pkh-c', valid, lockAddress), undefined);
  assert.equal(findMultisig(list, 'x', '', valid, lockAddress), undefined);
});

test('setup files round-trip and refuse anything that would change the lock', () => {
  const wallet = make('Treasury', '2', 'pkh-a\npkh-b\npkh-c');
  const text = setupFile(wallet);
  assert.equal(setupFileName(wallet), 'treasury.multisig.json');
  assert.equal(setupFileName(make('***', '1', 'pkh-a')), 'multisig.multisig.json');
  assert.deepEqual(multisigFromSetup(text, valid, lockAddress), wallet);

  const edit = (change: (setup: Record<string, unknown>) => void) => {
    const setup = JSON.parse(text);
    change(setup);
    return JSON.stringify(setup);
  };
  const refuses = (file: string, reason: RegExp) =>
    assert.throws(() => multisigFromSetup(file, valid, lockAddress), reason);

  refuses('not json', /not a multisig setup/);
  refuses('null', /not a multisig setup/);
  refuses(
    edit(setup => (setup.format = 'other')),
    /not a multisig setup/
  );
  refuses(
    edit(setup => (setup.version = 2)),
    /newer version/
  );
  // A swapped signer or a lowered threshold no longer matches the address.
  refuses(
    edit(setup => (setup.signers = ['pkh-a', 'pkh-b', 'pkh-z'])),
    /does not match/
  );
  refuses(
    edit(setup => (setup.required_signatures = 1)),
    /does not match/
  );
  refuses(
    edit(setup => (setup.address = 'lock:1:pkh-z')),
    /does not match/
  );
  refuses(
    edit(setup => (setup.signers = 'pkh-a')),
    /signers/
  );
  refuses(
    edit(setup => (setup.required_signatures = '2')),
    /required signatures/
  );
  refuses(
    edit(setup => (setup.signers = ['pkh-a', 'bad'])),
    /valid/
  );
  refuses(' '.repeat(64 * 1024 + 1), /too large/);
});

test('stored entries are revalidated and their addresses recomputed', () => {
  const treasury = make('Treasury', '2', 'pkh-a\npkh-b');
  const stored = [
    { ...treasury, address: 'lock:tampered' },
    { name: 'Broken', m: 5, pkhs: ['pkh-a'], address: 'x' },
    { name: 'No signers', m: 1 },
    null,
    'text'
  ];
  assert.deepEqual(storedMultisigs(stored, valid, lockAddress), [treasury]);
  assert.deepEqual(storedMultisigs(null, valid, lockAddress), []);
  assert.deepEqual(storedMultisigs({ 0: treasury }, valid, lockAddress), []);
});

test('the signing engine derives the saved address, and setup files keep it', () => {
  initSync({
    module: readFileSync(
      new URL('../nockster-esp/crates/nockster-wasm/pkg/nockster_wasm_bg.wasm', import.meta.url)
    )
  });
  const fixture = JSON.parse(
    readFileSync(new URL('./fixtures/nockster-signed.json', import.meta.url), 'utf8')
  );
  const signers = [fixture.source, fixture.recipients[0].address];
  const real = (name: string, m: string, list: string[]) =>
    savedMultisig(name, m, list.join('\n'), is_valid_pkh, compose_tx_v1_recipient_address);

  const wallet = real('Treasury', '2', signers);
  assert.equal(is_valid_pkh(wallet.address), true);
  assert.equal(wallet.address, compose_tx_v1_recipient_address({ m: 2, pkhs: signers }));
  // A different threshold over the same signers is a different lock.
  assert.notEqual(real('Any one', '1', signers).address, wallet.address);
  // Signer order does not change the lock, so co-signers may list them in any order.
  assert.equal(real('Reordered', '2', [...signers].reverse()).address, wallet.address);

  const imported = multisigFromSetup(
    setupFile(wallet),
    is_valid_pkh,
    compose_tx_v1_recipient_address
  );
  assert.deepEqual(imported, wallet);
  assert.deepEqual(
    storedMultisigs(
      JSON.parse(JSON.stringify([wallet])),
      is_valid_pkh,
      compose_tx_v1_recipient_address
    ),
    [wallet]
  );
});
