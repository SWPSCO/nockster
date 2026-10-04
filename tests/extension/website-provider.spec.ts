import { test, expect, chromium, type Page, type BrowserContext } from '@playwright/test';
import { mkdtemp, cp, writeFile, rm, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import bs58 from 'bs58';
import initWasm, { verifySignature, hashPublicKey } from '@nockbox/iris-wasm/iris_wasm.js';

const address = '4Lu3cSW34WPwvDkTwKh7xB6yMZrvh3bFW7w26UDJVdboxADGkr2bTnL';
const password = 'synthetic website provider password';

async function request(page: Page, method: string, params?: unknown, timeout = 120_000) {
  await page.evaluate(
    ({ method, params, timeout }) => {
      (window as any).reply = null;
      (window as any).nockster.request({ method, params, timeout }).then(
        (result: unknown) => {
          (window as any).reply = { result };
        },
        (error: { code?: number; message: string }) => {
          (window as any).reply = { error: { code: error.code, message: error.message } };
        }
      );
    },
    { method, params, timeout }
  );
}

async function approval(context: BrowserContext, page: Page, method: string, params?: unknown) {
  const opened = context.waitForEvent('page');
  await request(page, method, params);
  const popup = await opened;
  await popup.waitForURL(/approval\.html/);
  await popup.setViewportSize({ width: 420, height: 620 });
  return popup;
}

async function clickToCloseApproval(popup: Page, buttonName: string) {
  await Promise.all([
    popup.waitForEvent('close'),
    popup
      .getByRole('button', { name: buttonName, exact: true })
      .click()
      .catch(error => {
        // The worker can close the approval window before Chromium acknowledges the click.
        // Callers also verify the website's response to the completed request.
        if (
          !popup.isClosed() ||
          !(error instanceof Error) ||
          !error.message.includes('Target page, context or browser has been closed')
        ) {
          throw error;
        }
      })
  ]);
}

test('website provider gates connections and signatures on approval and binds requests to their origin', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'nockster-website-provider-'));
  const extension = join(dir, 'extension');
  await cp(resolve('apps/extension/ext'), extension, { recursive: true });
  await writeFile(join(extension, 'test.html'), '<!doctype html><title>Provider setup</title>');
  const context = await chromium.launchPersistentContext(join(dir, 'profile'), {
    channel: 'chromium',
    headless: true,
    args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
  });
  try {
    await context.route('https://**', route =>
      route.fulfill({
        contentType: 'text/html',
        body: '<!doctype html><title>Website provider test</title>'
      })
    );
    const worker = context.serviceWorkers()[0] || (await context.waitForEvent('serviceworker'));
    const extensionOrigin = `chrome-extension://${new URL(worker.url()).host}`;
    const setup = await context.newPage();
    await setup.goto(`${extensionOrigin}/test.html`);
    await setup.evaluate(async password => {
      await chrome.runtime.sendMessage({ type: 'vault:newVault', password });
      const reply = await chrome.runtime.sendMessage({
        type: 'vault:importWallet',
        nickname: 'Website wallet',
        key: 'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float'
      });
      if (!reply.success) throw new Error(reply.error);
      await chrome.runtime.sendMessage({ type: 'vault:lock' });
    }, password);
    const page = await context.newPage();
    await page.goto('https://login.example/');
    // Another wallet provider keeps its own namespace.
    await page.evaluate(() => {
      (window as any).nockchain = { provider: 'iris' };
    });
    expect(await page.evaluate(() => (window as any).nockster.provider)).toBe('nockster');
    expect(await page.evaluate(() => (window as any).nockchain.provider)).toBe('iris');

    await request(page, 'vault:exportWallet');
    await expect.poll(() => page.evaluate(() => (window as any).reply?.error?.code)).toBe(4200);
    await request(page, 'nock_signMessage', { message: 'No connection' });
    await expect.poll(() => page.evaluate(() => (window as any).reply?.error?.code)).toBe(4100);

    const connect = await approval(context, page, 'nock_connect');
    await expect(connect.getByText('https://login.example', { exact: true })).toBeVisible();
    await connect.getByLabel('Unlock Nockster').fill('incorrect password');
    await connect.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(connect.getByRole('alert')).toBeVisible();
    expect(await page.evaluate(() => (window as any).reply)).toBeNull();
    await connect.getByLabel('Unlock Nockster').fill(password);
    await connect.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(connect.getByRole('group', { name: 'Wallet', exact: true })).toContainText(
      address
    );
    await expect(connect.locator('select')).toHaveCount(0);
    await expect(connect.getByRole('radio')).toHaveCount(0);
    // A different extension tab cannot approve someone else's pending request.
    const spoof = await setup.evaluate(
      async id =>
        chrome.runtime.sendMessage({
          type: 'website:approve',
          id,
          approved: true,
          address: 'anything'
        }),
      new URL(connect.url()).searchParams.get('request')
    );
    expect(spoof.success).toBe(false);
    await connect.screenshot({ path: '/tmp/nockster-connect-approval.png' });
    await clickToCloseApproval(connect, 'Connect');
    await expect
      .poll(() => page.evaluate(() => (window as any).reply))
      .toEqual({
        result: { account: { address, type: 'v1' } }
      });

    const message = `Nockblocks Iris authentication\nAddress: ${address}\nOrigin: https://login.example\nChallenge: synthetic-challenge`;
    const sign = await approval(context, page, 'nock_signMessage', { message });
    await expect(sign.getByText('https://login.example', { exact: true })).toBeVisible();
    await expect(sign.getByLabel('Message to sign')).toHaveText(message);
    expect(await page.evaluate(() => (window as any).reply)).toBeNull();
    await sign.screenshot({ path: '/tmp/nockster-sign-approval.png' });
    await setup.evaluate(() => chrome.runtime.sendMessage({ type: 'vault:lock' }));
    await sign.getByRole('button', { name: 'Sign message', exact: true }).click();
    await expect(sign.getByLabel('Unlock Nockster')).toBeVisible();
    expect(await page.evaluate(() => (window as any).reply)).toBeNull();
    await sign.getByLabel('Unlock Nockster').fill(password);
    await sign.getByRole('button', { name: 'Unlock', exact: true }).click();
    await clickToCloseApproval(sign, 'Sign message');
    await expect
      .poll(() => page.evaluate(() => Boolean((window as any).reply?.result?.signature)))
      .toBe(true);
    const proof = await page.evaluate(() => (window as any).reply.result);
    await initWasm({
      module_or_path: await readFile('node_modules/@nockbox/iris-wasm/iris_wasm_bg.wasm')
    });
    const publicKey = bs58.decode(proof.publicKey);
    expect(hashPublicKey(publicKey)).toBe(address);
    expect(verifySignature(publicKey, proof.signature, message)).toBe(true);
    expect(verifySignature(publicKey, proof.signature, message + ' altered')).toBe(false);
    expect(JSON.stringify(proof)).not.toMatch(/privateKey|seedphrase|password/);

    const reject = await approval(context, page, 'nock_signMessage', { message });
    await clickToCloseApproval(reject, 'Cancel');
    await expect
      .poll(() => page.evaluate(() => (window as any).reply))
      .toEqual({
        error: { code: 4001, message: 'Nockster request rejected.' }
      });
    const closed = await approval(context, page, 'nock_signMessage', { message });
    await closed.close();
    await expect.poll(() => page.evaluate(() => (window as any).reply?.error?.code)).toBe(4001);

    const other = await context.newPage();
    await other.goto('https://other.example/');
    await request(other, 'nock_signMessage', { message });
    await expect.poll(() => other.evaluate(() => (window as any).reply?.error?.code)).toBe(4100);
    const navigated = await approval(context, other, 'nock_connect');
    const closedOnNavigation = navigated.waitForEvent('close');
    await other.goto('about:blank');
    await closedOnNavigation;

    const secondWallet = await setup.evaluate(async () => {
      const reply = await chrome.runtime.sendMessage({
        type: 'vault:importWallet',
        nickname: 'Second wallet with a long name to check wrapping in the approval window',
        key: [...Array(23).fill('abandon'), 'art'].join(' ')
      });
      if (!reply.success) throw new Error(reply.error);
      return reply.data.wallet;
    });
    const choose = await approval(context, page, 'nock_connect');
    await expect(choose.getByRole('radio')).toHaveCount(2);
    await expect(choose.locator('select')).toHaveCount(0);
    const firstChoice = choose.getByRole('radio').first();
    const secondChoice = choose.getByRole('radio').nth(1);
    await expect(firstChoice).toBeChecked();
    await secondChoice.check();
    await expect(secondChoice).toBeChecked();
    await secondChoice.press('ArrowUp');
    await expect(firstChoice).toBeChecked();
    await firstChoice.press('ArrowDown');
    await expect(secondChoice).toBeChecked();
    await choose.screenshot({ path: '/tmp/nockster-wallet-options.png' });
    await choose.setViewportSize({ width: 280, height: 620 });
    await choose.screenshot({ path: '/tmp/nockster-wallet-options-narrow.png' });
    expect(await choose.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true
    );
    await clickToCloseApproval(choose, 'Connect');
    await expect
      .poll(() => page.evaluate(() => (window as any).reply))
      .toEqual({
        result: { account: { address: secondWallet.publicKey, type: 'v1' } }
      });
    const secondSign = await approval(context, page, 'nock_signMessage', { message });
    await expect(secondSign.getByRole('group', { name: 'Wallet', exact: true })).toContainText(
      secondWallet.publicKey
    );
    await expect(secondSign.getByRole('radio')).toHaveCount(0);
    await expect(secondSign.locator('select')).toHaveCount(0);
    await clickToCloseApproval(secondSign, 'Sign message');
    await expect
      .poll(() => page.evaluate(() => Boolean((window as any).reply?.result?.signature)))
      .toBe(true);
    const secondProof = await page.evaluate(() => (window as any).reply.result);
    const secondPublicKey = bs58.decode(secondProof.publicKey);
    expect(hashPublicKey(secondPublicKey)).toBe(secondWallet.publicKey);
    expect(verifySignature(secondPublicKey, secondProof.signature, message)).toBe(true);

    const opened = context.waitForEvent('page');
    await request(page, 'nock_connect', undefined, 1000);
    const timedOut = await opened;
    const closedOnTimeout = timedOut.waitForEvent('close');
    await expect.poll(() => page.evaluate(() => (window as any).reply?.error?.code)).toBe(4001);
    await closedOnTimeout;
  } finally {
    await context.close();
    await rm(dir, { recursive: true, force: true });
  }
});
