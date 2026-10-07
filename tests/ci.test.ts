import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { matchesGlob } from 'node:path';
import { affectedPlatforms, releaseMetadata, publicationOptions } from '../scripts/ci/plan.mjs';

test('native changes select only their platform; shared changes select every consumer', () => {
  assert.deepEqual(affectedPlatforms(['apps/mobile/android/app/build.gradle']), { ios: false, android: true, extension: false, desktop: false });
  assert.deepEqual(affectedPlatforms(['apps/mobile/ios/App/App/WalletView.swift']), { ios: true, android: false, extension: false, desktop: false });
  assert.deepEqual(affectedPlatforms(['packages/wallet/src/platform/nativeBridge.ts']), { ios: true, android: true, extension: false, desktop: false });
  assert.deepEqual(affectedPlatforms(['apps/extension/src/background.ts']), { ios: false, android: false, extension: true, desktop: false });
  for (const path of ['packages/wallet-engine/src/lib.rs', 'packages/wallet/src/lib/utils/nicks.ts', 'package-lock.json', 'release-version.json', '.github/workflows/build.yml'])
    assert.deepEqual(affectedPlatforms([path]), { ios: true, android: true, extension: true, desktop: true });
  assert.deepEqual(affectedPlatforms(['docs/releases.md', '.gitignore']), { ios: false, android: false, extension: false, desktop: false });
  assert.deepEqual(affectedPlatforms(['apps/mobile/android/app/old.kt', 'apps/mobile/ios/App/new.swift']), { ios: true, android: true, extension: false, desktop: false });
});

test('standalone mobile bridge test changes do not select release builds', () => {
  const paths = ['tests/mobile/native.spec.ts', 'tests/mobile/rpcAuth.fixture.ts',
    'playwright.mobile.config.ts', '.github/workflows/mobile-bridge.yml'];
  for (const path of paths)
    assert.deepEqual(affectedPlatforms([path]), { ios: false, android: false, extension: false, desktop: false });
  assert.deepEqual(affectedPlatforms([...paths, 'packages/wallet/src/platform/nativeBridge.ts']),
    { ios: true, android: true, extension: false, desktop: false });
});

test('release metadata validates user input and gives retries distinct increasing builds', () => {
  assert.deepEqual(releaseMetadata('1.2.3', 12, 2), { version: '1.2.3', build_number: 1202, ios_build_number: '12.2', tag: 'v1.2.3-build.12.2' });
  for (const version of ['1.2', '01.2.3', '1.2.3;id', '1.2.65536', '0.0.0']) assert.throws(() => releaseMetadata(version, 1, 1));
  assert.throws(() => releaseMetadata('1.2.3', 1, 100));
  assert.ok(releaseMetadata('1.2.3', 13, 1).build_number > releaseMetadata('1.2.3', 12, 99).build_number);
});


test('pushes build artifacts and manual runs respect the TestFlight selection', () => {
  const ios = {ios:true,android:false,extension:false};
  const android = {ios:false,android:true,extension:false};
  assert.deepEqual(publicationOptions('push', 'refs/heads/master', ios), {publish:false,testflight:false,google_play:false});
  assert.deepEqual(publicationOptions('push', 'refs/heads/master', android), {publish:false,testflight:false,google_play:false});
  assert.equal(publicationOptions('push', 'refs/heads/feature', ios).testflight, false);
  assert.equal(publicationOptions('pull_request', 'refs/heads/master', ios).testflight, false);
  assert.equal(publicationOptions('workflow_dispatch', 'refs/heads/master', ios, {testflight:'true'}).testflight, true);
  assert.equal(publicationOptions('workflow_dispatch', 'refs/heads/master', ios, {testflight:'false'}).testflight, false);
  assert.throws(() => publicationOptions('workflow_dispatch', 'refs/heads/feature', ios, {testflight:'true'}));
  assert.throws(() => publicationOptions('workflow_dispatch', 'refs/heads/master', android, {testflight:'true'}));
});

test('Google Play uploads require an Android master build and respect manual selection', () => {
  const android = {ios:false,android:true,extension:false};
  const all = {ios:true,android:true,extension:true};
  assert.deepEqual(publicationOptions('push', 'refs/heads/master', all),
    {publish:false,testflight:false,google_play:false});
  assert.equal(publicationOptions('push', 'refs/heads/master', affectedPlatforms(['docs/google-play.md'])).google_play, false);
  assert.equal(publicationOptions('push', 'refs/heads/master', affectedPlatforms(['apps/extension/src/background.ts'])).google_play, false);
  for (const event of ['push', 'pull_request'])
    assert.equal(publicationOptions(event, 'refs/heads/feature', android, {google_play:'true'}).google_play, false);
  assert.equal(publicationOptions('workflow_dispatch', 'refs/heads/master', android).google_play, false);
  assert.equal(publicationOptions('workflow_dispatch', 'refs/heads/master', android, {google_play:'false'}).google_play, false);
  assert.deepEqual(publicationOptions('workflow_dispatch', 'refs/heads/master', android, {google_play:'true'}),
    {publish:false,testflight:false,google_play:true});
  assert.deepEqual(publicationOptions('workflow_dispatch', 'refs/heads/master', all, {google_play:'true',testflight:'true',publish:'true'}),
    {publish:true,testflight:true,google_play:true});
  assert.throws(() => publicationOptions('workflow_dispatch', 'refs/heads/feature', android, {google_play:'true'}), /require master/);
  assert.throws(() => publicationOptions('workflow_dispatch', 'refs/heads/master', {ios:true,android:false,extension:false}, {google_play:'true'}), /requires selecting Android/);
});

test('GitHub release publication uploads only selected mobile platforms', () => {
  for (const selected of ['all', 'ios', 'android', 'extension', 'desktop']) {
    const platforms = Object.fromEntries(['ios', 'android', 'extension', 'desktop'].map(name => [name, selected === 'all' || selected === name]));
    assert.deepEqual(publicationOptions('workflow_dispatch', 'refs/heads/master', platforms,
      {publish:'true',testflight:'false',google_play:'false'}),
      {publish:true,testflight:platforms.ios,google_play:platforms.android});
    assert.deepEqual(publicationOptions('workflow_dispatch', 'refs/heads/master', platforms),
      {publish:false,testflight:false,google_play:false});
    assert.throws(() => publicationOptions('workflow_dispatch', 'refs/heads/feature', platforms, {publish:'true'}), /require master/);
    for (const event of ['push', 'pull_request'])
      assert.deepEqual(publicationOptions(event, 'refs/heads/master', platforms,
        {publish:'true',testflight:'true',google_play:'true'}),
        {publish:false,testflight:false,google_play:false});
  }
});


test('desktop bundles and shared screens select their consumers', () => {
  for (const path of ['apps/desktop/index.html', 'apps/desktop/src-tauri/src/main.rs', 'apps/desktop/src/DesktopApp.svelte', 'tools/macos-release/src/main.rs', '.github/workflows/desktop.yml', 'packages/wallet/src/platform/desktopStorage.ts', 'apps/desktop/vite.config.ts', 'playwright.desktop.config.ts', 'tests/desktop/wallet.spec.ts', 'tests/desktop-release.test.ts', 'scripts/ci/collect-desktop.mjs', 'scripts/ci/configure-desktop.mjs'])
    assert.deepEqual(affectedPlatforms([path]), { ios: false, android: false, extension: false, desktop: true });
  for (const path of ['packages/wallet/src/App.svelte', 'packages/wallet/src/lib/components/organisms/WalletDashboard.svelte', 'packages/wallet/public/nockster-logo.svg', 'packages/wallet/src/styles/themes.css'])
    assert.deepEqual(affectedPlatforms([path]), { ios: false, android: false, extension: true, desktop: true });
});

test('mobile-only changes skip desktop and extension builds', () => {
  for (const path of ['apps/mobile/index.html', 'packages/wallet/src/platform/nativeBridge.ts', 'apps/mobile/capacitor.config.ts', 'apps/mobile/vite.config.ts', 'scripts/configure-mobile.mjs'])
    assert.deepEqual(affectedPlatforms([path]), { ios: true, android: true, extension: false, desktop: false });
  assert.deepEqual(affectedPlatforms(['apps/mobile/ios/App/App/WalletView.swift', 'apps/desktop/src/main.ts']),
    { ios: true, android: false, extension: false, desktop: true });
});

test('mobile bridge workflow excludes desktop and extension UI for pushes and pull requests', () => {
  const workflow = readFileSync(new URL('../.github/workflows/mobile-bridge.yml', import.meta.url), 'utf8');
  const triggers = workflow.split(/^  (?:push|pull_request):$/m).slice(1);
  assert.equal(triggers.length, 2);
  for (const trigger of triggers) {
    const patterns = [...trigger.matchAll(/^      - '([^']+)'$/gm)].map(match => match[1]);
    const triggersFor = (path: string) => patterns.reduce((selected, pattern) => {
      const excluded = pattern.startsWith('!');
      return matchesGlob(path, excluded ? pattern.slice(1) : pattern) ? !excluded : selected;
    }, false);
    for (const path of ['apps/desktop/src/main.ts', 'packages/wallet/src/platform/desktopStorage.ts', 'apps/desktop/src-tauri/src/main.rs', 'apps/desktop/index.html', 'apps/extension/src/background.ts', 'packages/wallet/src/App.svelte', 'packages/wallet/src/lib/components/organisms/Settings.svelte', 'packages/wallet/src/styles/themes.css'])
      assert.equal(triggersFor(path), false, path);
    for (const path of ['apps/mobile/index.html', 'packages/wallet/src/platform/nativeBridge.ts', 'packages/wallet/src/platform/vault.ts', 'packages/wallet/src/lib/stores/wallet.ts', 'packages/wallet-engine/src/vault.rs', 'tests/mobile/native.spec.ts', 'package-lock.json'])
      assert.equal(triggersFor(path), true, path);
  }
});

test('desktop UI checks follow the desktop platform selection', () => {
  const workflow = readFileSync(new URL('../.github/workflows/build.yml', import.meta.url), 'utf8');
  assert.match(workflow, /name: Test desktop wallet flows\n\s+if: needs\.plan\.outputs\.desktop == 'true'/);
});

test('hardware crypto selects desktop while independent firmware and examples select no wallet releases', () => {
  const none = { ios: false, android: false, extension: false, desktop: false };
  for (const path of ['nockster-esp/crates/nockster-fw/src/bin/main.rs', 'nockster-esp/web/package.json', '.github/workflows/esp-ci.yml', '.github/workflows/esp-desktop-release.yml', '.github/workflows/firmware-release.yml', '.github/workflows/deploy-web.yml', 'examples/design-reference/package.json'])
    assert.deepEqual(affectedPlatforms([path]), none);
  for (const path of ['nockster-esp/crates/nockster-core/src/lib.rs', 'nockster-esp/crates/nockster-wasm/src/lib.rs', 'nockster-esp/Cargo.toml', 'nockster-esp/Cargo.lock', 'nockster-esp/rust-toolchain.toml', 'scripts/build-hardware-wasm.sh'])
    assert.deepEqual(affectedPlatforms([path]), { ...none, desktop: true });
  assert.deepEqual(affectedPlatforms(['nockster-esp/Cargo.toml', 'apps/extension/src/main.ts']), { ...none, extension: true, desktop: true });
});

test('release numbering reserves the configured sequence and validates its bounds', () => {
  const { buildSequenceOffset } = JSON.parse(readFileSync('release-version.json', 'utf8'));
  const first = releaseMetadata('1.2.3', 1, 1, buildSequenceOffset);
  assert.equal(first.build_number, 100101);
  assert.equal(first.ios_build_number, '1001.1');
  assert.equal(first.tag, 'v1.2.3-build.1001.1');
  assert.ok(first.build_number > releaseMetadata('1.2.3', 1000, 99).build_number);
  assert.equal(releaseMetadata('1.2.3', 8999, 99, buildSequenceOffset).build_number, 999999);
  for (const offset of [-1, 0.5, NaN, Infinity]) assert.throws(() => releaseMetadata('1.2.3', 1, 1, offset));
  assert.throws(() => releaseMetadata('1.2.3', 9000, 1, buildSequenceOffset));
  for (const run of [0, -1, 1.5, NaN]) assert.throws(() => releaseMetadata('1.2.3', run, 1, buildSequenceOffset));
});
