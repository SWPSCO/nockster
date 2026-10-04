import * as core from '../../packages/wallet/src/pkg/nockster_core.js';
import * as iris from 'iris-v1/wasm';
import {
  buildBridgeNoun,
  computeBridgeLockRoot,
  ZORP_BRIDGE_ADDRESSES,
  ZORP_BRIDGE_THRESHOLD,
  BYTHOS_TX_ENGINE_SETTINGS
} from 'iris-v1';
import {
  BRIDGE_LOCK_ROOT,
  BRIDGE_MINIMUM_NICKS,
  verifyBridgeTransaction
} from '../../packages/wallet/src/lib/utils/bridge';
import { ensureVaultReady } from '../../packages/wallet/src/vault/wasm';
import { initializeIrisV1, transactionProtobuf } from '../../packages/wallet/src/platform/irisV1';
import fixture from '../fixtures/nockster-signed.json';
const exportedTests: Array<[string, () => unknown]> = [];
const assert = {
  equal(a: unknown, b: unknown, reason?: string) {
    if (a !== b) throw new Error(reason ?? `Expected ${String(a)} to equal ${String(b)}`);
  },
  ok(a: unknown) {
    if (!a) throw new Error('Expected true');
  },
  deepEqual(a: Uint8Array, b: Uint8Array) {
    if (a.length !== b.length || a.some((v, i) => v !== b[i]))
      throw new Error('Encoded bytes differ');
  }
};
function addTest(name: string, test: () => unknown) {
  exportedTests.push([name, test]);
}
const config = {
  threshold: ZORP_BRIDGE_THRESHOLD,
  addresses: ZORP_BRIDGE_ADDRESSES,
  noteDataKey: 'bridge',
  chainTag: '65736162',
  versionTag: '0',
  minAmountNicks: String(BRIDGE_MINIMUM_NICKS),
  expectedLockRoot: BRIDGE_LOCK_ROOT
};

const destination = '0x1234567890abcdef1234567890abcdef12345678';
const sourceLock = () => iris.spendConditionNewPkh(iris.pkhSingle(fixture.source));
const bridgeLock = () =>
  iris.spendConditionNewPkh(iris.pkhNew(BigInt(ZORP_BRIDGE_THRESHOLD), ZORP_BRIDGE_ADDRESSES));
function expectedBridgeNoun(address: string): iris.Noun {
  const prime = 18446744069414584321n;
  const value = BigInt(address);
  return [
    '0',
    [
      '65736162',
      [
        (value % prime).toString(16),
        [((value / prime) % prime).toString(16), (value / (prime * prime)).toString(16)]
      ]
    ]
  ];
}
function build(
  address = destination,
  value = BRIDGE_MINIMUM_NICKS,
  assets = [50_001 * 65536, 50_001 * 65536, 50_001 * 65536]
) {
  const notes = [fixture.note.lastName, fixture.source, fixture.recipients[0].address]
    .slice(0, assets.length)
    .map((lastName, index) => ({ ...fixture.note, lastName, assets: assets[index] }));
  const result = core.composeUnsignedTx(
    fixture.source,
    notes,
    [{ address: BRIDGE_LOCK_ROOT, gift: Number(value), bridgeEvmAddress: address }],
    100000
  );
  const raw = iris.rawTxV1FromNoun(
    iris.cue(Uint8Array.from(atob(core.toRawTxJam(result.base64Tx)), c => c.charCodeAt(0)))
  );
  return { result, raw, notes };
}

function checkOutputs(raw: iris.RawTxV1, address: string, value: bigint) {
  const outputs = iris.rawTxV1Outputs(raw, 100000, BYTHOS_TX_ENGINE_SETTINGS);
  const deposits = outputs.filter(note => note.note_data.some(([key]) => key === 'bridge'));
  assert.equal(deposits.length, 1, 'The bridge must see exactly one merged deposit');
  assert.equal(deposits[0].name.first, iris.spendConditionFirstName(bridgeLock()));
  assert.equal(BigInt(deposits[0].assets), value);
  const payload = deposits[0].note_data.find(([key]) => key === 'bridge')![1];
  assert.deepEqual(iris.jam(payload), iris.jam(expectedBridgeNoun(address)));
  for (const output of outputs.filter(note => note !== deposits[0])) {
    assert.equal(output.name.first, iris.spendConditionFirstName(sourceLock()));
    assert.equal(output.note_data.length, 1);
    assert.equal(output.note_data[0][0], 'lock');
  }
  return outputs;
}

function checkInputs(raw: iris.RawTxV1, notes: ReturnType<typeof build>['notes']) {
  for (const [name, spend] of raw.spends) {
    const note = notes.find(note => note.firstName === name.first && note.lastName === name.last);
    assert.ok(note);
    assert.equal(
      spend.seeds.reduce((sum, seed) => sum + BigInt(seed.gift), BigInt(spend.fee)),
      BigInt(note!.assets)
    );
    const parent: iris.NoteV1 = {
      version: 1,
      origin_page: note!.originPage,
      name,
      assets: String(note!.assets),
      note_data: [['lock', ['0', iris.spendConditionToNoun(sourceLock())]]]
    };
    for (const seed of spend.seeds) assert.equal(seed.parent_hash, iris.noteV1Hash(parent));
  }
}

function encode(raw: iris.RawTxV1): string {
  return btoa(String.fromCharCode(...iris.jam(iris.rawTxV1ToNoun(raw))));
}

export async function verifySignedDeposit(
  transaction: string,
  address: string,
  value: string,
  fee: number
) {
  await ensureVaultReady();
  await initializeIrisV1();
  await verifyBridgeTransaction(
    transaction,
    fixture.source,
    [{ address: BRIDGE_LOCK_ROOT, amount: Number(value), bridgeEvmAddress: address }],
    fee
  );
  const raw = iris.rawTxV1FromNoun(
    iris.cue(Uint8Array.from(atob(core.toRawTxJam(transaction)), c => c.charCodeAt(0)))
  );
  checkOutputs(raw, address, BigInt(value));
  checkInputs(raw, build(address, BigInt(value)).notes);
  for (const [, spend] of raw.spends) {
    assert.equal(spend.tag, 1);
    if (spend.tag !== 1) throw new Error('Expected V1 spend');
    assert.equal(spend.witness.pkh_signature.length, 1);
    for (const [pkh, [key, signature]] of spend.witness.pkh_signature) {
      assert.equal(pkh, fixture.source);
      assert.equal(iris.publicKeyHash(key), pkh);
      assert.ok(iris.publicKeyVerify(key, iris.spendV1SigHash(spend), signature));
    }
  }
  const transported = iris.rawTxFromProtobuf(transactionProtobuf(raw));
  assert.equal(transported.version, 1);
  assert.deepEqual(
    iris.jam(iris.rawTxV1ToNoun(raw)),
    iris.jam(iris.rawTxV1ToNoun(transported as iris.RawTxV1))
  );
  return { inputs: raw.spends.length, destination: address, amount: value };
}
addTest('Iris signer set derives the exact Nockster and Nockblocks bridge commitment', () => {
  assert.equal(computeBridgeLockRoot(config), BRIDGE_LOCK_ROOT);
  assert.deepEqual(
    iris.jam(buildBridgeNoun(destination, config)),
    iris.jam(expectedBridgeNoun(destination))
  );
});
addTest(
  'Nockster split-input bridge outputs match the protocol encoding and conserve the deposit',
  async () => {
    const prime = 18446744069414584321n;
    const values = [
      1n,
      prime - 1n,
      prime,
      prime + 1n,
      prime * prime - 1n,
      prime * prime,
      prime * prime + 1n,
      (1n << 160n) - 1n
    ];
    for (let i = 1n; i <= 24n; i++)
      values.push((i * 0xb74f4d97d40f49eaff06c3a9e97907362c9063fan) % (1n << 160n));
    for (const address of [
      destination,
      ...values.map(value => '0x' + value.toString(16).padStart(40, '0'))
    ]) {
      const { raw, notes, result } = build(address);
      assert.ok(raw.spends.length >= 2);
      checkOutputs(raw, address, BRIDGE_MINIMUM_NICKS);
      checkInputs(raw, notes);
      await verifyBridgeTransaction(
        result.base64Tx,
        fixture.source,
        [
          {
            address: BRIDGE_LOCK_ROOT,
            amount: Number(BRIDGE_MINIMUM_NICKS),
            bridgeEvmAddress: address
          }
        ],
        result.feePaid
      );
      for (const [, spend] of raw.spends)
        for (const seed of spend.seeds) {
          const entry = seed.note_data.find(([key]) => key === 'bridge');
          if (entry) assert.deepEqual(iris.jam(entry[1]), iris.jam(expectedBridgeNoun(address)));
        }
    }
  }
);

addTest(
  'one-input and fractional deposits preserve value, change, and the reviewed fee',
  async () => {
    for (const value of [
      BRIDGE_MINIMUM_NICKS,
      BRIDGE_MINIMUM_NICKS + 1n,
      BRIDGE_MINIMUM_NICKS + 65535n,
      BRIDGE_MINIMUM_NICKS + 65536n
    ]) {
      for (const assets of [[150003 * 65536], [50001 * 65536, 50001 * 65536, 50001 * 65536]]) {
        const { raw, notes, result } = build(destination, value, assets);
        checkOutputs(raw, destination, value);
        checkInputs(raw, notes);
        assert.equal(
          raw.spends.reduce((sum, [, spend]) => sum + BigInt(spend.fee), 0n),
          BigInt(result.feePaid)
        );
        await verifyBridgeTransaction(
          result.base64Tx,
          fixture.source,
          [{ address: BRIDGE_LOCK_ROOT, amount: Number(value), bridgeEvmAddress: destination }],
          result.feePaid
        );
      }
    }
  }
);

addTest('serialized transaction mutations fail the submission guard', async () => {
  const mutations: Array<(raw: iris.RawTxV1) => void> = [
    raw => {
      raw.spends[0][1].fee = String(BigInt(raw.spends[0][1].fee) + 1n);
    },
    raw => {
      raw.spends
        .flatMap(([, spend]) => spend.seeds)
        .find(seed => seed.note_data.some(([key]) => key === 'bridge'))!.gift = '1';
    },
    raw => {
      raw.spends
        .flatMap(([, spend]) => spend.seeds)
        .find(seed => seed.note_data.some(([key]) => key === 'bridge'))!.lock_root =
        iris.lockHash(sourceLock());
    },
    raw => {
      raw.spends
        .flatMap(([, spend]) => spend.seeds)
        .find(seed => seed.note_data.some(([key]) => key === 'bridge'))!.note_data = [];
    },
    raw => {
      raw.spends
        .flatMap(([, spend]) => spend.seeds)
        .find(seed => seed.note_data.some(([key]) => key === 'bridge'))!.note_data = [
        ['bridge', expectedBridgeNoun('0x' + '1'.repeat(40))]
      ];
    },
    raw => {
      raw.spends
        .flatMap(([, spend]) => spend.seeds)
        .find(seed => seed.note_data.some(([key]) => key === 'bridge'))!.note_data = [
        ['bridge', ['1', ['65736162', ['1', ['0', '0']]]]]
      ];
    },
    raw => {
      raw.spends
        .flatMap(([, spend]) => spend.seeds)
        .find(seed => seed.note_data.some(([key]) => key === 'bridge'))!.note_data = [
        ['bridge', ['0', ['657468', ['1', ['0', '0']]]]]
      ];
    },
    raw => {
      raw.spends
        .flatMap(([, spend]) => spend.seeds)
        .find(seed => !seed.note_data.some(([key]) => key === 'bridge'))!.lock_root =
        BRIDGE_LOCK_ROOT;
    }
  ];
  for (const mutate of mutations) {
    const { raw, result } = build();
    mutate(raw);
    let rejected = false;
    try {
      await verifyBridgeTransaction(
        encode(raw),
        fixture.source,
        [
          {
            address: BRIDGE_LOCK_ROOT,
            amount: Number(BRIDGE_MINIMUM_NICKS),
            bridgeEvmAddress: destination
          }
        ],
        result.feePaid
      );
    } catch {
      rejected = true;
    }
    assert.ok(rejected);
  }
});

addTest('repeated multi-note drafts release temporary WASM stacks', () => {
  for (let index = 0; index < 32; index++) {
    const { result } = build();
    core.verifyBridgeIntent(
      result.base64Tx,
      fixture.source,
      [
        {
          address: BRIDGE_LOCK_ROOT,
          gift: Number(BRIDGE_MINIMUM_NICKS),
          bridgeEvmAddress: destination
        }
      ],
      BigInt(result.feePaid)
    );
  }
});

export async function verify() {
  await ensureVaultReady();
  await initializeIrisV1();
  const passed: string[] = [];
  for (const [name, run] of exportedTests) {
    await run();
    passed.push(name);
  }
  return passed;
}
