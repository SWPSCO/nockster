import { execFileSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export function affectedPlatforms(paths) {
  const selected = new Set();
  const all = () => ['ios', 'android', 'extension', 'desktop'].forEach(platform => selected.add(platform));
  for (const path of paths) {
    if (/^(nockster-esp\/(crates\/(nockster-core|nockster-wasm)\/|Cargo\.(toml|lock)$|rust-toolchain\.toml$)|scripts\/build-hardware-wasm\.sh$|tests\/desktop-hardware\.test\.ts$)/.test(path)) {
      selected.add('desktop');
      continue;
    }
    if (/^(nockster-esp\/|examples\/|docs\/|.*\.md$|\.gitignore$|\.gitmodules$|\.claude\/|\.github\/workflows\/(esp-ci|esp-desktop-release|firmware-release|deploy-web)\.yml$)/.test(path)) continue;
    if (/^(tests\/mobile\/|playwright\.mobile\.config\.ts$|\.github\/workflows\/mobile-bridge\.yml$)/.test(path)) continue;
    if (/^(apps\/desktop\/|packages\/wallet\/src\/platform\/desktopStorage\.ts$|playwright\.desktop\.config\.ts$|tests\/desktop\/|tests\/desktop-(release|updates)\.test\.ts$|tools\/macos-release\/|scripts\/ci\/(configure-desktop|collect-desktop|desktop-updates)\.mjs$|\.github\/workflows\/desktop\.yml$)/.test(path)) selected.add('desktop');
    else if (/^(apps\/mobile\/ios\/|\.github\/workflows\/ios\.yml$|scripts\/ci\/ios-|tests\/ci\/ios_)/.test(path)) selected.add('ios');
    else if (/^(apps\/mobile\/android\/|\.github\/workflows\/android\.yml$|scripts\/ci\/android-)/.test(path)) selected.add('android');
    else if (/^(apps\/mobile\/|packages\/wallet\/src\/platform\/nativeBridge\.ts$|scripts\/configure-mobile\.mjs$)/.test(path)) {
      selected.add('ios');
      selected.add('android');
    } else if (/^(packages\/wallet\/(src\/(components\/|lib\/components\/|App\.svelte$|app\.css$|styles\/)|public\/)|svelte\.config\.js$)/.test(path)) {
      selected.add('extension');
      selected.add('desktop');
    } else if (/^(apps\/extension\/|tests\/extension\/|tests\/extension-package\.test\.ts$|playwright\.extension\.config\.ts$|scripts\/(generate-manifest\.cjs|package-extension\.mjs)$|\.github\/workflows\/extension\.yml$)/.test(path)) selected.add('extension');
    // Shared code, dependencies, Rust, and unclassified build inputs affect every app.
    else all();
  }
  return Object.fromEntries(['ios', 'android', 'extension', 'desktop'].map(platform => [platform, selected.has(platform)]));
}

export function releaseMetadata(version, run, attempt, offset = 0) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version) ||
      version.split('.').some(part => Number(part) > 65535) || version === '0.0.0')
    throw new Error('Version must be a nonzero three-part version with each component at most 65535.');
  if (!Number.isInteger(offset) || offset < 0) throw new Error('Build sequence offset must be a nonnegative integer.');
  if (!Number.isInteger(run) || run < 1) throw new Error('Build run must be a positive integer.');
  run += offset;
  if (!Number.isInteger(run) || run < 1 || run > 9999 || !Number.isInteger(attempt) || attempt < 1 || attempt > 99)
    throw new Error('Build numbering requires run 1–9999 and attempt 1–99.');
  return { version, build_number: run * 100 + attempt, ios_build_number: `${run}.${attempt}`, tag: `v${version}-build.${run}.${attempt}` };
}

export function publicationOptions(eventName, ref, platforms, inputs = {}) {
  const manual = eventName === 'workflow_dispatch';
  const publish = manual && inputs.publish === 'true';
  const testflight = (publish && platforms.ios)
    || (manual && inputs.testflight === 'true');
  const google_play = (publish && platforms.android)
    || (manual && inputs.google_play === 'true');
  if (testflight && !platforms.ios) throw new Error('TestFlight requires selecting iOS or all platforms.');
  if (google_play && !platforms.android) throw new Error('Google Play requires selecting Android or all platforms.');
  if ((publish || testflight || google_play) && ref !== 'refs/heads/master')
    throw new Error('Release publication, TestFlight, and Google Play uploads require master.');
  return { publish, testflight, google_play };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  const manual = process.env.GITHUB_EVENT_NAME === 'workflow_dispatch';
  let platforms;
  if (manual) {
    const platform = event.inputs?.platform ?? 'all';
    if (!['all', 'ios', 'android', 'extension', 'desktop'].includes(platform)) throw new Error('Invalid platform');
    platforms = Object.fromEntries(['ios', 'android', 'extension', 'desktop'].map(name => [name, platform === 'all' || platform === name]));
  } else {
    const before = event.before;
    const paths = before && !/^0+$/.test(before)
      ? execFileSync('git', ['diff', '--name-only', '--no-renames', '-z', before, process.env.GITHUB_SHA], { encoding: 'utf8' }).split('\0').filter(Boolean)
      : execFileSync('git', ['ls-files', '-z'], { encoding: 'utf8' }).split('\0').filter(Boolean);
    platforms = affectedPlatforms(paths);
  }
  const { publish, testflight, google_play } = publicationOptions(process.env.GITHUB_EVENT_NAME, process.env.GITHUB_REF, platforms, event.inputs);
  const release = JSON.parse(readFileSync('release-version.json', 'utf8'));
  if (!Number.isInteger(release.buildSequenceOffset) || release.buildSequenceOffset < 0)
    throw new Error('release-version.json requires a nonnegative buildSequenceOffset.');
  const version = event.inputs?.version?.trim() || release.version;
  const outputs = { ...platforms, any: Object.values(platforms).some(Boolean), publish, testflight, google_play,
    ...releaseMetadata(version, Number(process.env.GITHUB_RUN_NUMBER), Number(process.env.GITHUB_RUN_ATTEMPT), release.buildSequenceOffset) };
  appendFileSync(process.env.GITHUB_OUTPUT, Object.entries(outputs).map(([key, value]) => `${key}=${value}\n`).join(''));
  appendFileSync(process.env.GITHUB_STEP_SUMMARY, `Version: ${version}\n\niOS build: ${outputs.ios_build_number}\n\nAndroid version code: ${outputs.build_number}\n\nSelected platforms: ${Object.keys(platforms).filter(key => platforms[key]).join(', ') || 'none'}\n\nTestFlight: ${testflight ? 'upload requested' : 'not requested for this run'}\n\nGoogle Play: ${google_play ? 'Internal testing release requested' : 'not requested for this run'}\n\nGitHub release: ${publish ? 'requested' : 'not requested'}\n`);
}
