import { test, expect, type Page } from '@playwright/test';

import { mockAccounts } from './rpcAuth.fixture';

const password = 'test-only-device-password';
async function request(page: Page, action: string, fields: Record<string, unknown> = {}) {
  return page.evaluate(async data => JSON.parse(await window.nocksterNative.dispatch(data)), {
    action,
    ...fields
  });
}
async function create(page: Page) {
  const { mnemonic } = await request(page, 'generate');
  expect(mnemonic).toHaveLength(24);
  const reply = await request(page, 'import', {
    name: 'Test Wallet',
    password,
    key: mnemonic.join(' ')
  });
  expect(reply.error).toBeUndefined();
  expect(reply.state.unlocked).toBe(true);
  expect(reply.state.wallets).toHaveLength(1);
  return { mnemonic, address: reply.state.wallets[0].address };
}

test.beforeEach(async ({ page }) => {
  // No test contacts a gateway or broadcasts a payment.
  await page.route('https://**', route => route.abort());
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.nocksterNative));
});

test('device unlock uses a verified vault key and persists only ciphertext', async ({ page }) => {
  const { mnemonic, address } = await create(page);
  expect((await request(page, 'deviceUnlockKey', { password: 'wrong' })).error).toBeTruthy();
  const { unlockKey } = await request(page, 'deviceUnlockKey', { password });
  expect(Buffer.from(unlockKey, 'base64')).toHaveLength(32);
  await request(page, 'lock');
  expect((await request(page, 'status')).state.unlocked).toBe(false);
  expect((await request(page, 'deviceUnlockKey', { password })).error).toBeTruthy();
  expect(
    (await request(page, 'unlockWithDeviceKey', { key: Buffer.alloc(32).toString('base64') })).error
  ).toBeTruthy();
  const unlocked = await request(page, 'unlockWithDeviceKey', { key: unlockKey });
  expect(unlocked.error).toBeUndefined();
  expect(unlocked.state.wallets[0].address).toBe(address);
  const storage = await page.evaluate(() => JSON.stringify(localStorage));
  expect(storage).not.toContain(unlockKey);
  expect(storage).not.toContain(password);
  expect(storage).not.toContain(mnemonic.join(' '));
  await page.reload();
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  expect((await request(page, 'status')).state.unlocked).toBe(false);
  const reopened = await request(page, 'unlock', { password });
  expect(reopened.state.wallets[0].address).toBe(address);
});

test('locking clears private state and invalidates queued unlock work', async ({ page }) => {
  await create(page);
  await page.evaluate(async password => {
    const opening = window.nocksterNative.dispatch({ action: 'unlock', password });
    const locking = window.nocksterNative.dispatch({ action: 'lock' });
    await Promise.all([opening, locking]);
  }, password);
  const status = await request(page, 'status');
  expect(status.state.unlocked).toBe(false);
  expect(status.state.wallets).toEqual([]);
  expect(status.state.history).toEqual([]);
});

test('additional wallet imports suggest unused names and distinguish name collisions from duplicate keys', async ({ page }) => {
  const firstKey = (await request(page, 'generate')).mnemonic.join(' ');
  const firstName = (await request(page, 'status')).state.suggestedWalletName;
  expect(firstName).toBe('My Wallet');
  const first = await request(page, 'import', { name: firstName, password, key: firstKey });
  expect(first.error).toBeUndefined();
  expect(first.state.suggestedWalletName).toBe('My Wallet 2');

  const secondKey = (await request(page, 'generate')).mnemonic.join(' ');
  const collision = await request(page, 'import', { name: firstName, key: secondKey });
  expect(collision.error).toContain('Choose a different wallet name');
  expect(collision.state.wallets).toEqual(first.state.wallets);
  const second = await request(page, 'import', {
    name: first.state.suggestedWalletName,
    key: secondKey
  });
  expect(second.error).toBeUndefined();
  expect(second.state.wallets).toHaveLength(2);
  expect(second.state.activeId).toBe('vault-My Wallet 2');
  expect(second.state.suggestedWalletName).toBe('My Wallet 3');
  const secondAddress = second.state.wallets.find((wallet: { name: string }) => wallet.name === 'My Wallet 2').address;
  expect(secondAddress).not.toBe(first.state.wallets[0].address);

  const extendedKey = await page.evaluate(async () => {
    const modulePath = '/src/vaultController.ts';
    const vault = await import(/* @vite-ignore */ modulePath);
    const exported = await vault.exportWallet('My Wallet 2');
    await vault.deleteWallet('My Wallet 2');
    return exported.extendedPrivateKey;
  });
  await request(page, 'lock');
  const unlocked = await request(page, 'unlock', { password });
  expect(unlocked.state.suggestedWalletName).toBe('My Wallet 2');
  const keyCollision = await request(page, 'import', { name: firstName, key: extendedKey });
  expect(keyCollision.error).toContain('Choose a different wallet name');
  const imported = await request(page, 'import', { key: extendedKey });
  expect(imported.error).toBeUndefined();
  expect(imported.state.activeId).toBe('vault-My Wallet 2');
  expect(imported.state.wallets.find((wallet: { name: string }) => wallet.name === 'My Wallet 2').address).toBe(secondAddress);
  for (const key of [secondKey, extendedKey]) {
    const duplicate = await request(page, 'import', { name: 'Different name', key });
    expect(duplicate.error).toContain('already uses that private key');
    expect(duplicate.state.wallets).toHaveLength(2);
  }
  await page.reload();
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  const reopened = await request(page, 'unlock', { password });
  expect(reopened.state.wallets.map((wallet: { address: string }) => wallet.address).sort()).toEqual(
    [first.state.wallets[0].address, secondAddress].sort()
  );
});

test('wallet management targets the chosen wallet and requires explicit deletion confirmation', async ({ page }) => {
  const { address } = await create(page);
  const firstId = (await request(page, 'status')).state.activeId;
  const secondKey = (await request(page, 'generate')).mnemonic.join(' ');
  const second = await request(page, 'import', { name: 'Savings', key: secondKey });
  expect(second.state.suggestedWalletName).toBe('My Wallet 3');
  const secondId = second.state.activeId;
  const secondAddress = second.state.wallets.find((wallet: { id: string }) => wallet.id === secondId).address;
  const renamed = await request(page, 'rename', { walletId: firstId, name: 'Spending' });
  expect(renamed.error).toBeUndefined();
  expect(renamed.state.activeId).toBe(secondId);
  expect(renamed.state.wallets.find((wallet: { name: string }) => wallet.name === 'Spending').address).toBe(address);
  expect((await request(page, 'rename', { name: 'Spending' })).error).toContain('already exists');
  expect((await request(page, 'rename', { name: '   ' })).error).toContain('Enter a wallet name');
  const renamedActive = await request(page, 'rename', { walletId: secondId, name: 'Savings renamed' });
  expect(renamedActive.state.activeId).toBe('vault-Savings renamed');
  expect(renamedActive.state.wallets.find((wallet: { id: string }) => wallet.id === renamedActive.state.activeId).address).toBe(secondAddress);
  const fields = { walletId: renamedActive.state.activeId, confirmation: 'Savings renamed', backupConfirmed: true };
  for (const invalid of [{ ...fields, backupConfirmed: false }, { ...fields, confirmation: 'Savings' }, { confirmation: fields.confirmation, backupConfirmed: true }]) {
    const refused = await request(page, 'deleteWallet', invalid);
    expect(refused.error).toBeTruthy();
    expect(refused.state.wallets).toHaveLength(2);
  }
  await request(page, 'lock');
  expect((await request(page, 'deleteWallet', fields)).error).toContain('Unlock');
  await request(page, 'unlock', { password });
  const deleted = await request(page, 'deleteWallet', fields);
  expect(deleted.error).toBeUndefined();
  expect(deleted.state.wallets).toHaveLength(1);
  expect(deleted.state.wallets[0]).toMatchObject({ name: 'Spending', address });
  expect(deleted.state.activeId).toBe('vault-Spending');
  expect(deleted.state.suggestedWalletName).toBe('My Wallet 2');
  expect((await request(page, 'deleteWallet', { walletId: 'vault-Spending', confirmation: 'Spending', backupConfirmed: true })).error).toContain('only wallet');
  await page.reload();
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  const reopened = await request(page, 'unlock', { password });
  expect(reopened.state.wallets).toHaveLength(1);
  expect(reopened.state.wallets[0]).toMatchObject({ name: 'Spending', address });
  const restored = await request(page, 'import', { key: secondKey });
  expect(restored.error).toBeUndefined();
  expect(restored.state.wallets).toHaveLength(2);
  expect(restored.state.wallets.find((wallet: { name: string }) => wallet.name === 'My Wallet 2').address).toBe(secondAddress);
});

test('native payment submission requires a current reviewed draft', async ({ page }) => {
  await create(page);
  expect((await request(page, 'send', { previewId: 'not-reviewed' })).error).toContain('Review');
  expect(
    (await request(page, 'prepare', { recipients: [{ address: 'invalid', amount: '-1' }] })).error
  ).toContain('valid');
  await request(page, 'lock');
  expect((await request(page, 'prepare', { recipients: [] })).error).toContain('Unlock');
});

test('renaming keeps the wallet address and pending transaction reservations', async ({ page }) => {
  const { address } = await create(page);
  // Populate a synthetic pending record through the app's persistence boundary.
  await page.evaluate(() => {
    const state = JSON.parse(localStorage.getItem('walletState')!);
    const pending = {
      txId: 'synthetic',
      recipients: [],
      totalAmount: 1,
      fee: 1,
      timestamp: 1,
      fromAddress: state.wallets[0].addresses[0],
      signedTx: 'test',
      inputNotes: ['a/b']
    };
    state.wallets[0].pendingTransactions = [pending];
    state.activeWallet = state.wallets[0];
    localStorage.setItem('walletState', JSON.stringify(state));
  });
  await page.reload();
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  await request(page, 'unlock', { password });
  const renamed = await request(page, 'rename', { name: 'Daily Wallet' });
  expect(renamed.error).toBeUndefined();
  expect(renamed.state.wallets[0].address).toBe(address);
  expect(renamed.state.wallets[0].name).toBe('Daily Wallet');
  expect(renamed.state.history[0].id).toBe('synthetic');
});

for (const authentication of ['password', 'device'] as const) test(`payment review with ${authentication} submits its signed draft once and reserves inputs`, async ({ page }) => {
  const fixture = JSON.parse(
    await (
      await import('node:fs/promises')
    ).readFile(new URL('../fixtures/nockster-signed.json', import.meta.url), 'utf8')
  );
  const submitted: unknown[] = [];
  let confirmedId: string | undefined;
  await mockAccounts(page);
  await page.route('https://nockblocks.com/rpc**', async route => {
    const body = route.request().postDataJSON();
    let result: unknown;
    switch (body.method) {
      case 'getNotesByAddress':
        result = [fixture.note];
        break;
      case 'getTip':
        result = { height: fixture.height };
        break;
      case 'submitTransaction':
        submitted.push(body.params);
        result = 'synthetic-submission';
        break;
      case 'getBalance':
        result = { balance: fixture.note.assets };
        break;
      case 'getTransactionsByAddress':
        result = { transactions: confirmedId ? [{ txId: confirmedId, type: 'sent', amount: fixture.recipients[0].gift, fee: 0, timestamp: 1, blockHeight: fixture.height, counterparty: fixture.recipients[0].address, inputs: [], outputs: [] }] : [] };
        break;
      case 'getTransactionSubmission':
        result = { txId: body.params[0].txId, status: 'acknowledged', tracked: true };
        break;
      case 'getAddressBook':
        result = { entries: [{ id: 'recipient', alias: 'Test Contact', address: fixture.recipients[0].address }] };
        break;
      case 'getBlockchainMetrics':
        result = { blockHeight: fixture.height };
        break;
      default:
        result = [];
    }
    await route.fulfill({ json: { jsonrpc: '2.0', id: body.id, result, price: 0.116 } });
  });
  await page.route(
    'https://nockblocks.com/nockchain.public.v2.NockchainService/WalletSendTransaction',
    async route => {
      expect(route.request().headers().authorization).toContain('Bearer ak_');
      const body = route.request().postDataBuffer()!;
      expect(body[0]).toBe(0);
      expect(body.readUInt32BE(1)).toBe(body.length - 5);
      submitted.push(body);
      const trailer = Buffer.from('grpc-status: 0\r\n');
      const trailerFrame = Buffer.alloc(5);
      trailerFrame[0] = 128;
      trailerFrame.writeUInt32BE(trailer.length, 1);
      await route.fulfill({
        contentType: 'application/grpc-web+proto',
        body: Buffer.concat([Buffer.from([0, 0, 0, 0, 2, 10, 0]), trailerFrame, trailer])
      });
    }
  );
  const mnemonic =
    'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float';
  expect(
    (await request(page, 'import', { name: 'Test Wallet', password, key: mnemonic })).error
  ).toBeUndefined();
  const recipient = fixture.recipients[0];
  const prepared = await request(page, 'prepare', {
    amountUnit: 'nicks',
    recipients: [{ address: recipient.address, amount: String(recipient.gift) }]
  });
  expect(prepared.error).toBeUndefined();
  expect(prepared.preview.netSent).toBe('10');
  expect(prepared.preview.netSentUsd).toBe('~1.16 USD');
  expect(prepared.preview.recipients[0].amountUsd).toBe('~1.16 USD');
  expect(Number(prepared.preview.netSentNicks.replaceAll(',', '')) + Number(prepared.preview.feeNicks.replaceAll(',', ''))).toBe(Number(prepared.preview.totalNicks.replaceAll(',', '')));
  expect(submitted).toHaveLength(0);
  expect((await request(page, 'send', { previewId: prepared.preview.id })).error).toContain('Confirm this payment');
  expect((await request(page, 'send', { previewId: prepared.preview.id, password: 'wrong' })).error).toContain('Incorrect wallet password');
  expect((await request(page, 'send', { previewId: prepared.preview.id, key: Buffer.alloc(32).toString('base64') })).error).toContain('Device authentication failed');
  expect(submitted).toHaveLength(0);
  const credential = authentication === 'password' ? { password } : { key: (await request(page, 'deviceUnlockKey', { password })).unlockKey };
  const sent = await request(page, 'send', {
    previewId: prepared.preview.id,
    ...credential,
    recipients: [{ address: 'unreviewed', amount: '1000' }]
  });
  expect(sent.error).toBeUndefined();
  expect(submitted).toHaveLength(1);
  const stored = await page.evaluate(() => JSON.parse(localStorage.getItem('walletState')!));
  expect(sent.txId).toMatch(/^[1-9A-HJ-NP-Za-km-z]{40,60}$/);
  expect(stored.wallets[0].pendingTransactions[0].txId).toBe(sent.txId);
  expect(stored.wallets[0].pendingTransactions[0].submissionStatus).toBe('acknowledged');
  expect(stored.wallets[0].pendingTransactions[0].lastSubmitError).toBeNull();
  expect((await request(page, 'retry', { txId: sent.txId })).error).toContain('Confirm this payment');
  expect((await request(page, 'retry', { txId: sent.txId, password: 'wrong' })).error).toContain('Incorrect wallet password');
  expect(stored.wallets[0].pendingTransactions[0].recipients).toEqual([
    { address: recipient.address, amount: recipient.gift }
  ]);
  expect(stored.wallets[0].pendingTransactions[0].inputNotes.length).toBeGreaterThan(0);
  expect((await request(page, 'send', { previewId: prepared.preview.id })).error).toContain(
    'Review'
  );
  expect(submitted).toHaveLength(1);
  // A status refresh resolves local uncertainty without submitting the signed draft again.
  await request(page, 'lock');
  await page.evaluate(() => {
    const state = JSON.parse(localStorage.getItem('walletState')!);
    const pending = state.wallets[0].pendingTransactions[0];
    pending.submissionStatus = 'unknown';
    pending.lastSubmitError = 'Submission outcome unknown';
    localStorage.setItem('walletState', JSON.stringify(state));
  });
  await page.reload();
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  expect((await request(page, 'unlock', { password })).error).toBeUndefined();
  const refreshed = await request(page, 'refresh');
  expect(refreshed.state.history.find((tx: { id: string }) => tx.id === sent.txId)).toMatchObject({
    status: 'acknowledged',
    amountUsd: '~1.16 USD',
    error: null
  });
  expect(submitted).toHaveLength(1);
  expect(refreshed.state.history.find((tx: { id: string }) => tx.id === sent.txId).parties[0].label).toBe('Test Contact');
  confirmedId = sent.txId;
  const confirmed = await request(page, 'refresh');
  expect(confirmed.state.history.filter((tx: { id: string }) => tx.id === sent.txId)).toHaveLength(1);
  expect(confirmed.state.history.find((tx: { id: string }) => tx.id === sent.txId).status).toBe('confirmed');
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('walletState')!));
  expect(saved.wallets[0].pendingTransactions).toHaveLength(0);
  expect(submitted).toHaveLength(1);
});

test('import selects and refreshes the new wallet and refresh keeps every balance current', async ({
  page
}) => {
  await mockAccounts(page, { anyPublicKey: true });
  let failHistory = false;
  const balances = new Map<string, number>();
  await page.route('https://nockblocks.com/rpc**', async route => {
    const body = route.request().postDataJSON();
    const address = body.params?.[0]?.address;
    if (body.method === 'getAddressBook') return route.fulfill({ json: { result: { entries: [] } } });
    if (body.method === 'getTip') return route.fulfill({ json: { result: { height: 1 }, price: 0.116 } });
    if (body.method === 'getNotes') {
      if (!balances.has(address)) balances.set(address, (balances.size + 1) * 65536);
      return route.fulfill({ json: { result: { nicks: balances.get(address) } } });
    }
    if (body.method === 'getTransactionsByAddress') {
      if (failHistory) return route.fulfill({ status: 500, body: 'History unavailable' });
      return route.fulfill({ json: { result: { transactions: [] } } });
    }
    if (body.method === 'getBlockchainMetrics')
      return route.fulfill({ json: { result: { blockHeight: 1 } } });
    throw new Error(`Unexpected dashboard request: ${body.method}`);
  });
  const first = await create(page);
  const generated = await request(page, 'generate');
  const imported = await request(page, 'import', {
    name: 'Second wallet',
    key: generated.mnemonic.join(' ')
  });
  expect(imported.error).toBeUndefined();
  expect(imported.state.activeId).toBe('vault-Second wallet');
  expect(
    imported.state.wallets.find((wallet: { id: string }) => wallet.id === 'vault-Test Wallet')
      .balance
  ).toBe('1.00');
  expect(
    imported.state.wallets.find((wallet: { id: string }) => wallet.id === 'vault-Second wallet')
      .balance
  ).toBe('2.00');
  expect(imported.state.networkError).toBeNull();
  balances.set(first.address, 3 * 65536 + 1);
  failHistory = true;
  const partial = await request(page, 'refresh');
  expect(
    partial.state.wallets.find((wallet: { address: string }) => wallet.address === first.address)
      .balance
  ).toBe('3.00');
  expect(
    partial.state.wallets.find((wallet: { address: string }) => wallet.address === first.address)
      .balanceNicks
  ).toBe('196,609');
  expect(partial.state.networkError).toContain('Transaction history could not update: HTTP 500');
  expect(partial.state.networkError).not.toContain('balance');
  failHistory = false;
  expect((await request(page, 'refresh')).state.networkError).toBeNull();
});

test('foreground native entry stays unlocked beyond the engine idle interval and explicit lock still wins', async ({
  page
}) => {
  await create(page);
  await page.clock.setFixedTime(new Date(Date.now() + 6 * 60_000));
  const generated = await request(page, 'generate');
  const imported = await request(page, 'import', {
    name: 'After typing',
    key: generated.mnemonic.join(' ')
  });
  expect(imported.error).toBeUndefined();
  expect(imported.state.unlocked).toBe(true);
  await request(page, 'lock');
  expect(
    (await request(page, 'import', { name: 'Locked', key: generated.mnemonic.join(' ') })).error
  ).toContain('Unlock');
});

test('switching send units preserves exact nick amounts and empty recipient fields', async ({
  page
}) => {
  await create(page);
  const values = ['1', '65536', '9007199254740991', ''];
  const converted = await request(page, 'convertAmounts', {
    amountUnit: 'nicks',
    recipients: values.map(amount => ({ amount }))
  });
  expect(converted.error).toBeUndefined();
  expect(converted.amounts).toEqual([
    '0.0000152587890625',
    '1',
    '137438953471.9999847412109375',
    ''
  ]);
  const back = await request(page, 'convertAmounts', {
    amountUnit: 'nock',
    recipients: converted.amounts.map((amount: string) => ({ amount }))
  });
  expect(back.amounts).toEqual(values);
  expect(
    (
      await request(page, 'prepare', {
        amountUnit: 'nicks',
        recipients: [{ address: 'invalid', amount: '1.5' }]
      })
    ).error
  ).toContain('valid');
});


test('Base bridge review binds the destination, payout, fee and signed deposit through submission', async ({ page }) => {
  const fixture = JSON.parse(await (await import('node:fs/promises')).readFile(new URL('../fixtures/nockster-signed.json', import.meta.url), 'utf8'));
  const destination = '0x1234567890abcdef1234567890abcdef12345678';
  const notes = [fixture.note.lastName, fixture.source, fixture.recipients[0].address].map(lastName => ({ ...fixture.note, lastName, assets: 50_001 * 65536 }));
  const submitted: Buffer[] = [];
  await mockAccounts(page);
  await page.route('https://nockblocks.com/rpc**', async route => {
    const body = route.request().postDataJSON();
    const result = body.method === 'getNotesByAddress' ? notes
      : body.method === 'getTip' ? { height: 100000 }
      : body.method === 'getBalance' ? { balance: 150003 * 65536 }
      : body.method === 'getTransactionsByAddress' ? { transactions: [] }
      : body.method === 'getTransactionSubmission' ? { txId: body.params[0].txId, status: 'acknowledged', tracked: true }
      : body.method === 'getAddressBook' ? { entries: [] } : [];
    await route.fulfill({ json: { jsonrpc: '2.0', id: body.id, result } });
  });
  await page.route('https://nockblocks.com/nockchain.public.v2.NockchainService/WalletSendTransaction', async route => {
    submitted.push(route.request().postDataBuffer()!);
    const trailer = Buffer.from('grpc-status: 0\r\n');
    const frame = Buffer.alloc(5); frame[0] = 128; frame.writeUInt32BE(trailer.length, 1);
    await route.fulfill({ contentType: 'application/grpc-web+proto', body: Buffer.concat([Buffer.from([0,0,0,0,2,10,0]), frame, trailer]) });
  });
  const key = 'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float';
  expect((await request(page, 'import', { name: 'Bridge test', password, key })).error).toBeUndefined();
  for (const bridge of [{destination,amount:'99999'}, {destination:'0x'+'0'.repeat(40),amount:'100000'}])
    expect((await request(page, 'prepare', {bridge})).error).toBeTruthy();
  expect((await request(page, 'prepare', {bridge:{destination,amount:'100000'},privateOutputs:true})).error).toContain('public');
  const reviewed = await request(page, 'prepare', {bridge:{destination,amount:'100000'}});
  expect(reviewed.error).toBeUndefined();
  expect(reviewed.preview.bridge).toEqual({destination,amount:'100000',protocolFee:'297.54638671875',expectedReceived:'99702.45361328125'});
  expect(reviewed.preview.recipients).toHaveLength(1);
  expect(reviewed.preview.privateOutputs).toBe(false);
  expect(Number(reviewed.preview.total)).toBe(100000 + Number(reviewed.preview.fee));
  expect(submitted).toHaveLength(0);
  expect((await request(page, 'send', {previewId:reviewed.preview.id,password:'wrong'})).error).toContain('Incorrect');
  const sent = await request(page, 'send', {previewId:reviewed.preview.id,password,bridge:{destination:'0x'+'1'.repeat(40),amount:'200000'}});
  expect(sent.error).toBeUndefined();
  expect(submitted).toHaveLength(1);
  const stored = await page.evaluate(() => JSON.parse(localStorage.getItem('walletState')!));
  const pending = stored.wallets[0].pendingTransactions[0];
  const independentlyChecked = await page.evaluate(async ({path, pending, destination}) => {
    const audit = await import('/@fs' + path);
    return audit.verifySignedDeposit(pending.signedTx, destination, String(pending.totalAmount), pending.fee);
  }, {path: new URL('./bridge.fixture.ts', import.meta.url).pathname, pending, destination});
  expect(independentlyChecked.amount).toBe(String(100000 * 65536));
  expect(pending.bridge.destination).toBe(destination);
  expect(pending.totalAmount).toBe(100000 * 65536);
  expect(pending.inputNotes.length).toBeGreaterThanOrEqual(2);
  expect(sent.state.history[0].bridge).toEqual(reviewed.preview.bridge);
  expect((await request(page, 'send', {previewId:reviewed.preview.id,password})).error).toContain('Review');
  expect(submitted).toHaveLength(1);
});

test('pending activity shows mempool sightings, clears locally, and yields to confirmed history', async ({ page }) => {
  await mockAccounts(page);
  let mempool: 'missing' | 'present' | 'offline' = 'missing';
  let queried = 0;
  let mined = false;
  const txId = '7jaf1xLSmoiMU9k8UULQtaNZGNUJWZBJ9DA2ZK2JUTRkAc8H7pDmueP';
  await page.route('https://nockblocks.com/rpc**', async route => {
    const body = route.request().postDataJSON();
    if (body.method === 'getMempoolTransactionByTxid') {
      queried++;
      expect(body.params).toEqual([{ transactionId: txId }]);
      if (mempool === 'offline') return route.abort();
      return route.fulfill({ json: mempool === 'present'
        ? { jsonrpc: '2.0', id: body.id, result: { transaction: { id: txId } } }
        : { jsonrpc: '2.0', id: body.id, error: { code: -32000, message: 'Transaction not found in mempool' } } });
    }
    const result = body.method === 'getTransactionSubmission'
      ? { txId, status: 'acknowledged', tracked: true }
      : body.method === 'getTransactionsByAddress' ? { transactions: mined ? [{ txId, type: 'sent', amount: 65536, fee: 256, timestamp: 1_790_000_000, blockHeight: 153881, counterparties: ['test'], inputs: [], outputs: [] }] : [] }
      : body.method === 'getBalance' ? { balance: 65536 }
      : body.method === 'getAddressBook' ? { entries: [] }
      : body.method === 'getTip' ? { height: 153880, price: 0.01 } : [];
    await route.fulfill({ json: { jsonrpc: '2.0', id: body.id, result } });
  });
  expect((await request(page, 'import', { name: 'Pending test', password,
    key: 'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float'
  })).error).toBeUndefined();
  const state = (await request(page, 'status')).state;
  const walletId = state.activeId;
  async function seed(confirmed = false) {
    await page.evaluate(async ({ txId, confirmed }) => {
      const state = JSON.parse(localStorage.getItem('walletState')!);
      const wallet = { ...state.activeWallet, pendingTransactions: [{
        txId, timestamp: Date.now() - 31 * 60_000, submissionStatus: 'acknowledged',
        recipients: [{ address: 'test', amount: 65536 }], totalAmount: 65536,
        fee: 256, fromAddress: 'test', inputNotes: ['test/input']
      }], transactions: confirmed ? [{ txId, amount: 65536, fee: 256, timestamp: Date.now(),
        status: 'confirmed', blockHeight: 153881, type: 'sent', from: 'test', to: 'test' }] : [] };
      state.wallets = [wallet];
      state.activeWallet = wallet;
      localStorage.setItem('walletState', JSON.stringify(state));
    }, { txId, confirmed });
    await page.reload();
    await page.waitForFunction(() => Boolean(window.nocksterNative));
    expect((await request(page, 'unlock', { password })).error).toBeUndefined();
  }
  await seed();
  expect((await request(page, 'status')).state.history[0].statusLabel).toBe('Failed?');
  mempool = 'present';
  const seen = await request(page, 'refresh');
  expect(queried).toBeGreaterThan(0);
  expect(seen.state.history[0].statusLabel).toBe('In mempool');
  expect(seen.state.history[0].canClear).toBe(true);
  mempool = 'offline';
  expect((await request(page, 'refresh')).state.history[0].statusLabel).toBe('Failed?');
  expect((await request(page, 'clearPending', { walletId, txId })).error).toBeTruthy();
  expect((await request(page, 'clearPending', { walletId: 'other', txId, confirmation: txId })).error).toBeTruthy();
  const cleared = await request(page, 'clearPending', { walletId, txId, confirmation: txId });
  expect(cleared.error).toBeUndefined();
  expect(cleared.state.history).toHaveLength(0);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('walletState')!));
  expect(saved.wallets[0].pendingTransactions).toEqual([]);
  await page.reload();
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  expect((await request(page, 'unlock', { password })).state.history).toHaveLength(0);
  await seed(true);
  const confirmed = (await request(page, 'status')).state.history;
  expect(confirmed).toHaveLength(1);
  expect(confirmed[0].statusLabel).toBe('Confirmed');
  expect(confirmed[0].canClear).toBe(false);
  expect((await request(page, 'clearPending', { walletId, txId, confirmation: txId })).error).toContain('no longer pending');
  mined = true;
  const refreshed = await request(page, 'refresh');
  expect(refreshed.state.history).toHaveLength(1);
  expect(refreshed.state.history[0].status).toBe('confirmed');
  expect((await page.evaluate(() => JSON.parse(localStorage.getItem('walletState')!))).wallets[0].pendingTransactions).toEqual([]);
});

for (const outcome of ['rejected', 'unknown'] as const) {
  test(`submission ${outcome} returns an error and preserves the signed pending record`, async ({ page }) => {
    const fixture = JSON.parse(await (await import('node:fs/promises')).readFile(new URL('../fixtures/nockster-signed.json', import.meta.url), 'utf8'));
    await mockAccounts(page);
    let sends = 0;
    await page.route('https://nockblocks.com/rpc**', async route => {
      const body = route.request().postDataJSON();
      const result = body.method === 'getNotesByAddress' ? [fixture.note]
        : body.method === 'getTip' ? { height: fixture.height }
        : body.method === 'getBalance' ? { balance: fixture.note.assets }
        : body.method === 'getTransactionsByAddress' ? { transactions: [] }
        : body.method === 'getTransactionSubmission' ? { txId: body.params[0].txId, status: outcome, tracked: true, checkFailed: outcome === 'unknown' }
        : body.method === 'getAddressBook' ? { entries: [] } : [];
      await route.fulfill({ json: { jsonrpc: '2.0', id: body.id, result } });
    });
    await page.route('https://nockblocks.com/nockchain.public.v2.NockchainService/WalletSendTransaction', async route => {
      sends++;
      const frame = (data: Buffer, trailer = false) => {
        const header = Buffer.alloc(5); header[0] = trailer ? 128 : 0;
        header.writeUInt32BE(data.length, 1); return Buffer.concat([header, data]);
      };
      const message = Buffer.from('Node did not accept the transaction.');
      const error = Buffer.concat([Buffer.from([8, 4, 18, message.length]), message]);
      const response = outcome === 'rejected'
        ? Buffer.concat([frame(Buffer.concat([Buffer.from([18, error.length]), error])), frame(Buffer.from('grpc-status: 0\r\n'), true)])
        : frame(Buffer.from('grpc-status: 14\r\ngrpc-message: Acceptance%20check%20unavailable\r\n'), true);
      await route.fulfill({ contentType: 'application/grpc-web+proto', body: response });
    });
    const imported = await request(page, 'import', { name: 'Submission test', password,
      key: 'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float' });
    expect(imported.error).toBeUndefined();
    const reviewed = await request(page, 'prepare', { recipients: [{ address: fixture.recipients[0].address, amount: '10' }] });
    expect(reviewed.error).toBeUndefined();
    const sent = await request(page, 'send', { previewId: reviewed.preview.id, password });
    expect(sent.error).toBeTruthy();
    expect(sent.txId).toBeUndefined();
    expect(sends).toBe(1);
    expect(sent.state.history).toHaveLength(1);
    expect(sent.state.history[0].statusLabel).toBe(outcome === 'rejected' ? 'Not accepted' : 'Pending');
    expect(sent.state.history[0].canClear).toBe(true);
    const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('walletState')!));
    const pending = saved.wallets[0].pendingTransactions[0];
    expect(pending.submissionStatus).toBe(outcome);
    expect(pending.signedTx.length).toBeGreaterThan(0);
    expect(pending.inputNotes.length).toBeGreaterThan(0);
  });
}

test('native bridge history shares inclusion-based progress with unknown-tip handling', async ({ page }) => {
  await mockAccounts(page, {anyPublicKey:true});
  let tip: number | null = 1123;
  await page.route('https://nockblocks.com/rpc**', async route => {
    const body = route.request().postDataJSON();
    const bridgeOutput = { note: {lockScriptHash: 'AcsPkuhXQoGeEsF91yynpm1kcW17PQ2Z1MEozgx7YnDPkZwrtzLuuqd', assets: 6553600000, noteData: {bridge: '0x'+'1'.repeat(40)}} };
    const result = body.method === 'getTransactionsByAddress' ? {transactions:[{
      txId:'deposit',type:'sent',amount:6553600000,fee:100,timestamp:1790000000,blockHeight:1000,
      counterparties:['test'],inputs:[],outputs:[bridgeOutput]
    }]} : body.method === 'getTip' ? {height:tip}
      : body.method === 'getNotes' ? {nicks:0,notes:[]}
      : body.method === 'getAddressBook' ? {entries:[]} : [];
    await route.fulfill({json:{jsonrpc:'2.0',id:body.id,result}});
  });
  await create(page);
  expect((await request(page,'status')).state.history[0].bridgeProgress).toMatchObject({blocks:123,target:400,phase:'confirming'});
  for (const [height,phase,blocks] of [[1400,'ready',400],[2000,'ready',400],[null,'unavailable',null]] as const) {
    tip=height;
    const updated=await request(page,'refresh');
    expect(updated.state.history[0].bridgeProgress).toMatchObject({blocks,phase});
  }
});
