import { test, expect, type Page } from '@playwright/test';
import { desktopIO, source } from './native.fixture';
import { HardwarePeer, address } from './hardware.fixture';
import { createHash } from 'node:crypto';

const firmwareImage = Buffer.alloc(600, 42);
function firmwareBundle(hash = createHash('sha256').update(firmwareImage).digest('hex')) {
  return Buffer.from(
    JSON.stringify({
      format: 'nockster-update-bundle-v1',
      signature_scheme: 'secp256k1-ecdsa-sha256-prehash-v1',
      manifest: {
        manifest_version: 1,
        release_version: 12,
        image_size: firmwareImage.length,
        image_sha256_hex: hash,
        signing_pubkey_sha256_hex: '22'.repeat(32),
        hardware_target: 'esp32s3-touch-lcd-1.47',
        build_profile: 'production',
        protocol_v: 1,
        git_commit: 'a'.repeat(40),
        tx_types_rev: 'synthetic-test-types'
      },
      signing_pubkey_sec1_hex: '02' + '00'.repeat(32),
      signature_hex: '33'.repeat(64)
    })
  );
}

async function loadFirmware(page: Page, bundle = firmwareBundle()) {
  await page.getByRole('button', { name: 'Firmware', exact: true }).click();
  await page.getByText('Install from files', { exact: true }).click();
  await page
    .getByLabel('Update manifest (.bundle.json)')
    .setInputFiles({ name: 'test.bundle.json', mimeType: 'application/json', buffer: bundle });
  await page.getByLabel('Firmware image (.bin)').setInputFiles({
    name: 'test.bin',
    mimeType: 'application/octet-stream',
    buffer: firmwareImage
  });
  await page.getByRole('button', { name: 'Check files', exact: true }).click();
}

async function connect(page: Page, peer: HardwarePeer) {
  await desktopIO(page, peer.invoke);
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Hardware', exact: true }).click();
  await page.getByRole('button', { name: 'Connect', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Unlock your device' })).toBeVisible();
  await page.getByLabel('Device PIN', { exact: true }).fill('123456');
  await page.getByRole('button', { name: 'Unlock device', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Hardware savings', exact: true })).toBeVisible();
}

test('hardware-only setup imports public addresses and keeps one device session across navigation', async ({
  page
}) => {
  const peer = new HardwarePeer();
  await connect(page, peer);
  await expect(page.locator('.hw-address').first()).toHaveText(address);
  await page.getByRole('button', { name: 'Use wallet', exact: true }).click();
  await expect(page.locator('.desktop-wallets')).toContainText('Hardware savings');
  await expect(page.locator('.desktop-wallets')).toContainText('Hardware');
  const nativeStore = await page.evaluate(() =>
    JSON.parse(localStorage.getItem('desktop-test-native-store') || '{}')
  );
  expect(JSON.stringify(nativeStore)).not.toContain('123456');
  expect(peer.opens).toBe(1);
  await page.getByRole('button', { name: 'Hardware', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Hardware savings', exact: true })).toBeVisible();
  peer.slot = 3;
  await page.getByRole('button', { name: 'Verify address', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('Address verified');
  expect(peer.requests).toContain(47);
  expect(peer.opens).toBe(1);
});

test('device rejection and unplug end approval without reconnecting or retrying', async ({
  page
}) => {
  const peer = new HardwarePeer();
  await connect(page, peer);
  peer.holdApproval = true;
  await page.getByRole('button', { name: 'Verify address', exact: true }).click();
  await expect.poll(() => peer.approvalId).not.toBeNull();
  peer.rejectApproval();
  await expect(page.getByRole('alert')).toContainText(/reject/i);
  await expect(page.getByRole('button', { name: 'Verify address', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Verify address', exact: true }).click();
  await expect.poll(() => peer.approvalId).not.toBeNull();
  peer.plugged = false;
  await expect(page.getByRole('heading', { name: 'Connect your Nockster' })).toBeVisible();
  await expect(page.getByRole('alert')).toContainText(/disconnected/i);
  expect(peer.requests.filter(request => request === 47)).toHaveLength(2);
  expect(peer.opens).toBe(1);
});

test('destructive actions require explicit confirmation and cancellation sends no reset', async ({
  page
}) => {
  const peer = new HardwarePeer();
  await connect(page, peer);
  await page.getByText('Wallet options', { exact: true }).click();
  await page.getByRole('button', { name: 'Remove from device' }).click();
  await expect(page.getByRole('button', { name: 'Remove wallet', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('button', { name: 'Security', exact: true }).click();
  await page.getByRole('button', { name: 'Factory reset', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Erase device', exact: true })).toBeDisabled();
  await page.getByLabel('Type RESET to confirm').fill('RESET');
  await expect(page.getByRole('button', { name: 'Erase device', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(peer.requests).not.toContain(15);
  expect(peer.requests).not.toContain(21);
  expect(peer.hasSeed).toBe(true);
});

test('invalid firmware files cannot reach device verification or flash writes', async ({
  page
}) => {
  const peer = new HardwarePeer();
  await connect(page, peer);
  await page.getByRole('button', { name: 'Firmware', exact: true }).click();
  await page.getByText('Install from files', { exact: true }).click();
  await page.getByLabel('Update manifest (.bundle.json)').setInputFiles({
    name: 'invalid.bundle.json',
    mimeType: 'application/json',
    buffer: Buffer.from('{}')
  });
  await page.getByLabel('Firmware image (.bin)').setInputFiles({
    name: 'firmware.bin',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from([1, 2, 3])
  });
  await page.getByRole('button', { name: 'Check files', exact: true }).click();
  await expect(page.getByRole('alert')).toBeVisible();
  expect(peer.requests).not.toContain(32);
  expect(peer.requests).not.toContain(33);
  expect(peer.guard).toEqual([]);
});

test('hardware management stays available while a software vault is locked', async ({ page }) => {
  const peer = new HardwarePeer();
  await connect(page, peer);
  await page.evaluate(async source => {
    const vault = await import(`${source}/lib/utils/vaultBridge.ts`);
    const wallet = await import(`${source}/lib/stores/wallet.ts`);
    await vault.createVault('synthetic-lock-test-password');
    await vault.lockVaultSession();
    wallet.walletStore.lock();
  }, source);
  await expect(page.getByRole('heading', { name: 'Hardware savings' })).toBeVisible();
  await page.getByRole('button', { name: 'Use wallet', exact: true }).click();
  await expect(
    page
      .getByRole('navigation', { name: 'Main' })
      .getByRole('button', { name: 'Receive', exact: true })
  ).toBeEnabled();
  const locked = await page.evaluate(async source => {
    const vault = await import(`${source}/lib/utils/vaultBridge.ts`);
    return !(await vault.getVaultStatus()).unlocked;
  }, source);
  expect(locked).toBe(true);
});

test('firmware image hashes and device rejection both block flash writes', async ({ page }) => {
  const peer = new HardwarePeer();
  await connect(page, peer);
  await loadFirmware(page, firmwareBundle('00'.repeat(32)));
  await expect(page.getByRole('alert')).toContainText(/hash|sha256/i);
  expect(peer.requests).not.toContain(32);
  await page.getByLabel('Update manifest (.bundle.json)').setInputFiles({
    name: 'test.bundle.json',
    mimeType: 'application/json',
    buffer: firmwareBundle()
  });
  await page.getByRole('button', { name: 'Check files', exact: true }).click();
  await page.getByRole('button', { name: 'Review update', exact: true }).click();
  peer.rejectManifest = true;
  await page.getByRole('button', { name: 'Install update', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText(/reject/i);
  expect(peer.requests).toContain(32);
  expect(peer.requests).not.toContain(33);
  expect(peer.guard).toEqual([]);
});

test('firmware installation requires review, transfers exact bytes, and releases the close guard', async ({
  page
}) => {
  const peer = new HardwarePeer();
  await connect(page, peer);
  await loadFirmware(page);
  await expect(page.getByRole('heading', { name: 'Release 12', exact: true })).toBeVisible();
  expect(peer.requests).not.toContain(32);
  await page.getByRole('button', { name: 'Review update', exact: true }).click();
  await page.getByRole('button', { name: 'Install update', exact: true }).click();
  await expect(
    page.getByRole('status').filter({ hasText: 'Firmware written and verified' })
  ).toBeVisible();
  expect(Buffer.from(peer.updateBytes)).toEqual(firmwareImage);
  expect(peer.guard).toEqual([true, false]);
  expect(peer.requests.filter(request => request === 34)).toHaveLength(3);
  await expect(page.getByRole('button', { name: 'Restart device', exact: true })).toBeEnabled();
});

test('approval prompts and rejection stay visible when signing below the fold', async ({
  page
}) => {
  await page.setViewportSize({ width: 800, height: 640 });
  const peer = new HardwarePeer();
  await connect(page, peer);
  await page.getByRole('button', { name: 'Tools', exact: true }).click();
  await page.getByLabel('Tool', { exact: true }).selectOption('Sign & verify');
  await page.getByLabel('Message text', { exact: true }).fill('Synthetic approval test');
  await page.getByRole('button', { name: 'Sign on device', exact: true }).click();
  await expect.poll(() => peer.approvalId).not.toBeNull();
  await page.locator('.hardware-workspace').evaluate(element => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(page.getByRole('status').filter({ hasText: 'Review and approve' })).toBeInViewport();
  await page.screenshot({ path: '.impeccable/review/signing-pending-800.png' });
  peer.rejectApproval();
  await expect(page.getByRole('alert')).toContainText(/reject/i);
  await expect(page.getByRole('alert')).toBeInViewport();
  await page.screenshot({ path: '.impeccable/review/signing-rejected-800.png' });
  expect(peer.requests.filter(request => request === 48)).toHaveLength(1);
});

test('forgotten PIN reset reaches device setup without unlocking', async ({ page }) => {
  const peer = new HardwarePeer();
  await desktopIO(page, peer.invoke);
  await page.goto('/');
  await page.getByRole('button', { name: 'Hardware', exact: true }).click();
  await page.getByRole('button', { name: 'Connect', exact: true }).click();
  await page.getByText('Forgot your device PIN?', { exact: true }).click();
  await page.getByRole('button', { name: 'Factory reset', exact: true }).click();
  await page.getByLabel('Type RESET to confirm').fill('RESET');
  await page.getByRole('button', { name: 'Erase device', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Set up Nockster', exact: true })).toBeVisible();
  expect(peer.requests).toContain(21);
  expect(peer.requests).not.toContain(16);
  const phrase = [...Array(23).fill('abandon'), 'art'].join(' ');
  await page.getByLabel('24-word recovery phrase', { exact: true }).fill(phrase);
  await page.getByLabel('Device PIN', { exact: true }).fill('654321');
  await page.getByLabel('Confirm PIN', { exact: true }).fill('654321');
  await page.getByRole('button', { name: 'Set up device', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Hardware savings', exact: true })).toBeVisible();
  expect(peer.requests).toContain(13);
  const stored = await page.evaluate(() => localStorage.getItem('desktop-test-native-store'));
  expect(stored || '').not.toContain(phrase);
  expect(stored || '').not.toContain('654321');
  await expect(page.getByLabel('24-word recovery phrase', { exact: true })).toHaveCount(0);
});
