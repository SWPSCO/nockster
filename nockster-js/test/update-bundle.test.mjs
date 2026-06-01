import assert from 'node:assert/strict';
import { createHash, webcrypto } from 'node:crypto';
import test from 'node:test';

import {
  MAX_UPDATE_IMAGE_SIZE,
  MAX_UPDATE_RELEASE_VERSION,
  UPDATE_MANIFEST_VERSION,
  UPDATE_OTA_STATE_INVALID,
  UPDATE_OTA_STATE_NEW,
  UPDATE_RELEASE_INDEX_FORMAT,
  UPDATE_SLOT_NONE,
  UPDATE_SLOT_OTA0,
  UPDATE_SLOT_OTA1,
  assertPrivateUpdateReleaseUrls,
  assertPostInstallUpdateBootStatus,
  assertUpdateBundleCompatible,
  assertUpdateFirmwareMatchesBundle,
  assertUpdateReleaseIndexMatchesBundle,
  fetchLatestUpdateRelease,
  fetchUpdateReleaseArtifacts,
  getPostInstallUpdateBootStatusFailures,
  getUpdateStreamStatusFailures,
  getUpdateBundleCompatibilityBlocker,
  assertUpdateStreamStatus,
  parseUpdateBundleJson,
  parseUpdateReleaseIndexJson,
  updateOtaStateName,
  updateSlotName,
} from '../dist/index.js';

if (!globalThis.crypto?.subtle) {
  Object.defineProperty(globalThis, 'crypto', { value: webcrypto });
}

function sha256Hex(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function validBundle(overrides = {}) {
  const { manifest: manifestOverrides = {}, ...bundleOverrides } = overrides;
  const manifest = {
    manifest_version: UPDATE_MANIFEST_VERSION,
    release_version: 7,
    image_size: 3,
    image_sha256_hex: '11'.repeat(32),
    signing_pubkey_sha256_hex: '22'.repeat(32),
    hardware_target: 'esp32s3-touch-lcd-1.47',
    build_profile: 'production',
    protocol_v: 1,
    git_commit: 'a'.repeat(40),
    tx_types_rev: 'b'.repeat(40),
    ...manifestOverrides,
  };

  return {
    format: 'nockster-update-bundle-v1',
    signature_scheme: 'secp256k1-ecdsa-sha256-prehash-v1',
    manifest,
    signing_pubkey_sec1_hex: '02' + '00'.repeat(32),
    signature_hex: '33'.repeat(64),
    ...bundleOverrides,
  };
}

function buildInfo(overrides = {}) {
  return {
    git_commit: 'f'.repeat(40),
    git_dirty: false,
    build_profile: 'production',
    protocol_v: 1,
    tx_types_rev: 'e'.repeat(40),
    ...overrides,
  };
}

function bootStatus(overrides = {}) {
  return {
    partition_table_ok: true,
    ota_data_present: true,
    ota0_present: true,
    ota1_present: true,
    current_slot: UPDATE_SLOT_OTA0,
    next_slot: UPDATE_SLOT_OTA1,
    ota_state: UPDATE_OTA_STATE_NEW,
    ota0_offset: 0x310000,
    ota0_size: 0x300000,
    ota1_offset: 0x610000,
    ota1_size: 0x300000,
    ...overrides,
  };
}

function updateStatus(bundle, overrides = {}) {
  return {
    active: true,
    manifest_verified: true,
    image_verified: false,
    release_version: bundle.manifest.release_version,
    bytes_received: 0,
    image_size: bundle.manifest.image_size,
    ...overrides,
  };
}

function responseFromJson(value, init = {}) {
  return new Response(JSON.stringify(value), {
    headers: { 'content-type': 'application/json', ...(init.headers ?? {}) },
    status: init.status ?? 200,
  });
}

function responseFromText(value, init = {}) {
  return new Response(value, {
    headers: { 'content-type': 'text/plain', ...(init.headers ?? {}) },
    status: init.status ?? 200,
  });
}

function responseFromBytes(value, init = {}) {
  return new Response(value, {
    headers: { 'content-length': String(value.byteLength), ...(init.headers ?? {}) },
    status: init.status ?? 200,
  });
}

test('parseUpdateBundleJson accepts a valid signed-update bundle shape', () => {
  const parsed = parseUpdateBundleJson(validBundle());

  assert.equal(parsed.manifest.manifest_version, UPDATE_MANIFEST_VERSION);
  assert.equal(parsed.manifest.release_version, 7);
  assert.equal(parsed.manifest.image_size, 3);
  assert.equal(parsed.manifest.image_sha256.length, 32);
  assert.equal(parsed.manifest.signing_pubkey_sha256.length, 32);
  assert.equal(parsed.signing_pubkey_sec1.length, 33);
  assert.equal(parsed.signature64.length, 64);
});

test('parseUpdateBundleJson rejects non-object bundle and manifest shapes', () => {
  assert.throws(
    () => parseUpdateBundleJson([]),
    /update bundle must be a JSON object/,
  );
  assert.throws(
    () => parseUpdateBundleJson({ ...validBundle(), manifest: [] }),
    /update bundle is missing manifest/,
  );
});

test('parseUpdateBundleJson rejects malformed numeric manifest fields', () => {
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { image_size: 'nan' } })),
    /image_size must be an integer/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { image_size: '3' } })),
    /image_size must be an integer/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { image_size: 0 } })),
    /image_size must be nonzero/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { image_size: MAX_UPDATE_IMAGE_SIZE + 1 } })),
    /image_size must be an integer/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { protocol_v: 256 } })),
    /protocol_v must be an integer/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { release_version: 1.5 } })),
    /release_version must be an integer/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { release_version: MAX_UPDATE_RELEASE_VERSION + 1 } })),
    /release_version must be an integer/,
  );
});

test('parseUpdateBundleJson rejects unsupported manifest versions', () => {
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { manifest_version: UPDATE_MANIFEST_VERSION + 1 } })),
    /unsupported update manifest version/,
  );
});

test('parseUpdateBundleJson rejects missing manifest strings', () => {
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { hardware_target: '' } })),
    /hardware_target must be a non-empty string/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { git_commit: null } })),
    /git_commit must be a non-empty string/,
  );
});

test('parseUpdateBundleJson rejects wrong-length cryptographic fields', () => {
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ signing_pubkey_sec1_hex: '02' })),
    /signing_pubkey_sec1 must be 33 bytes/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ signing_pubkey_sec1_hex: '04' + '00'.repeat(32) })),
    /signing_pubkey_sec1 must be a compressed SEC1 public key/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ signature_hex: '33' })),
    /signature64 must be 64 bytes/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { image_sha256_hex: '11' } })),
    /image_sha256 must be 32 bytes/,
  );
  assert.throws(
    () => parseUpdateBundleJson(validBundle({ manifest: { image_sha256_hex: '0g'.repeat(32) } })),
    /invalid image sha256/,
  );
});

test('parseUpdateReleaseIndexJson resolves release artifact URLs and metadata', () => {
  const parsed = parseUpdateReleaseIndexJson(
    {
      format: UPDATE_RELEASE_INDEX_FORMAT,
      bundle_url: 'nockster-fw.update.json',
      firmware_url: '../fw/nockster-fw.bin',
      release_version: 7,
      image_size: 3,
      image_sha256_hex: '11'.repeat(32),
      hardware_target: 'esp32s3-touch-lcd-1.47',
      build_profile: 'production',
      protocol_v: 1,
      git_commit: 'a'.repeat(40),
      tx_types_rev: 'b'.repeat(40),
    },
    'https://updates.example.test/releases/latest.json',
  );

  assert.equal(parsed.bundleUrl.href, 'https://updates.example.test/releases/nockster-fw.update.json');
  assert.equal(parsed.firmwareUrl.href, 'https://updates.example.test/fw/nockster-fw.bin');
  assert.equal(parsed.metadata.releaseVersion, 7);
  assert.equal(parsed.metadata.imageSha256Hex, '11'.repeat(32));
});

test('parseUpdateReleaseIndexJson accepts camelCase fields for web tooling', () => {
  const parsed = parseUpdateReleaseIndexJson(
    {
      bundleUrl: 'https://updates.example.test/bundle.json',
      firmwareUrl: 'https://updates.example.test/firmware.bin',
      releaseVersion: 8,
      imageSize: 4,
      imageSha256Hex: '44'.repeat(32),
      hardwareTarget: 'esp32s3-touch-lcd-1.47',
      buildProfile: 'production',
      protocolV: 1,
      gitCommit: 'c'.repeat(40),
      txTypesRev: 'd'.repeat(40),
    },
    'https://updates.example.test/latest.json',
  );

  assert.equal(parsed.bundleUrl.href, 'https://updates.example.test/bundle.json');
  assert.equal(parsed.firmwareUrl.href, 'https://updates.example.test/firmware.bin');
  assert.equal(parsed.metadata.releaseVersion, 8);
  assert.equal(parsed.metadata.imageSize, 4);
});

test('parseUpdateReleaseIndexJson rejects malformed release indexes', () => {
  assert.throws(
    () => parseUpdateReleaseIndexJson([], 'https://updates.example.test/latest.json'),
    /release index must be a JSON object/,
  );
  assert.throws(
    () => parseUpdateReleaseIndexJson(
      { firmware_url: 'firmware.bin' },
      'https://updates.example.test/latest.json',
    ),
    /release index is missing bundle_url/,
  );
  assert.throws(
    () => parseUpdateReleaseIndexJson(
      { format: 'unknown', bundle_url: 'bundle.json', firmware_url: 'firmware.bin' },
      'https://updates.example.test/latest.json',
    ),
    /unsupported release index format/,
  );
  assert.throws(
    () => parseUpdateReleaseIndexJson(
      { bundle_url: 'bundle.json', firmware_url: 'javascript:alert(1)' },
      'https://updates.example.test/latest.json',
    ),
    /firmware URL: firmware URL must use http or https/,
  );
  assert.throws(
    () => parseUpdateReleaseIndexJson(
      { bundle_url: 'bundle.json', firmware_url: 'firmware.bin', release_version: -1 },
      'https://updates.example.test/latest.json',
    ),
    /release index release_version must be a non-negative integer/,
  );
  assert.throws(
    () => parseUpdateReleaseIndexJson(
      { bundle_url: 'bundle.json', firmware_url: 'firmware.bin', release_version: MAX_UPDATE_RELEASE_VERSION + 1 },
      'https://updates.example.test/latest.json',
    ),
    /release index release_version must be at most/,
  );
  assert.throws(
    () => parseUpdateReleaseIndexJson(
      { bundle_url: 'bundle.json', firmware_url: 'firmware.bin', image_size: MAX_UPDATE_IMAGE_SIZE + 1 },
      'https://updates.example.test/latest.json',
    ),
    /release index image_size must be at most/,
  );
  assert.throws(
    () => parseUpdateReleaseIndexJson(
      { bundle_url: 'bundle.json', firmware_url: 'firmware.bin', protocol_v: 256 },
      'https://updates.example.test/latest.json',
    ),
    /release index protocol_v must be at most/,
  );
  assert.throws(
    () => parseUpdateReleaseIndexJson(
      { bundle_url: 'bundle.json', firmware_url: 'firmware.bin', image_sha256_hex: '0g'.repeat(32) },
      'https://updates.example.test/latest.json',
    ),
    /invalid release index image_sha256_hex/,
  );
});

test('assertUpdateReleaseIndexMatchesBundle catches stale publication metadata', () => {
  const bundle = parseUpdateBundleJson(validBundle());

  assert.doesNotThrow(() => assertUpdateReleaseIndexMatchesBundle({
    releaseVersion: 7,
    imageSize: 3,
    imageSha256Hex: '11'.repeat(32),
    hardwareTarget: 'esp32s3-touch-lcd-1.47',
    buildProfile: 'production',
    protocolV: 1,
    gitCommit: 'a'.repeat(40),
    txTypesRev: 'b'.repeat(40),
  }, bundle));

  assert.throws(
    () => assertUpdateReleaseIndexMatchesBundle({ releaseVersion: 6 }, bundle),
    /release index metadata mismatch/,
  );
  assert.throws(
    () => assertUpdateReleaseIndexMatchesBundle({ imageSha256Hex: 'ff'.repeat(32) }, bundle),
    /image_sha256_hex does not match bundle manifest/,
  );
});

test('assertUpdateFirmwareMatchesBundle validates firmware size and digest', async () => {
  const firmware = new Uint8Array([1, 2, 3]);
  const bundle = parseUpdateBundleJson(validBundle({
    manifest: {
      image_size: firmware.length,
      image_sha256_hex: sha256Hex(firmware),
    },
  }));

  await assert.doesNotReject(() => assertUpdateFirmwareMatchesBundle(bundle, firmware));
  await assert.rejects(
    () => assertUpdateFirmwareMatchesBundle(bundle, new Uint8Array([1, 2])),
    /firmware size mismatch/,
  );
  await assert.rejects(
    () => assertUpdateFirmwareMatchesBundle(bundle, new Uint8Array([1, 2, 4])),
    /firmware sha256 mismatch/,
  );
});

test('getUpdateBundleCompatibilityBlocker mirrors browser update manifest policy', () => {
  const bundle = parseUpdateBundleJson(validBundle({
    manifest: {
      release_version: 8,
      build_profile: 'production',
    },
  }));

  assert.equal(getUpdateBundleCompatibilityBlocker(bundle, {
    releaseVersion: 7,
    buildInfo: buildInfo(),
  }), null);
  assert.doesNotThrow(() => assertUpdateBundleCompatible(bundle, {
    releaseVersion: 7,
    buildInfo: buildInfo(),
  }));

  assert.match(
    getUpdateBundleCompatibilityBlocker(parseUpdateBundleJson(validBundle({
      manifest: { hardware_target: 'other-board' },
    })), { releaseVersion: 0, buildInfo: buildInfo() }),
    /Bundle target other-board does not match/,
  );
  assert.match(
    getUpdateBundleCompatibilityBlocker(parseUpdateBundleJson(validBundle({
      manifest: { protocol_v: 2 },
    })), { releaseVersion: 0, buildInfo: buildInfo({ protocol_v: 1 }) }),
    /Bundle protocol 2 does not match device protocol 1/,
  );
  assert.match(
    getUpdateBundleCompatibilityBlocker(bundle, {
      releaseVersion: 8,
      buildInfo: buildInfo(),
    }),
    /not newer than device release 8/,
  );
  assert.match(
    getUpdateBundleCompatibilityBlocker(parseUpdateBundleJson(validBundle({
      manifest: { build_profile: 'dev' },
    })), { releaseVersion: 0, buildInfo: buildInfo({ build_profile: 'production' }) }),
    /Bundle profile dev is not accepted by device profile production/,
  );
  assert.equal(
    getUpdateBundleCompatibilityBlocker(parseUpdateBundleJson(validBundle({
      manifest: { build_profile: 'production' },
    })), { releaseVersion: 0, buildInfo: buildInfo({ build_profile: 'dev' }) }),
    null,
  );
  assert.match(
    getUpdateBundleCompatibilityBlocker(parseUpdateBundleJson(validBundle({
      manifest: { build_profile: 'unknown' },
    })), { releaseVersion: 0, buildInfo: buildInfo({ build_profile: 'dev' }) }),
    /Bundle profile unknown is not accepted by device profile dev/,
  );
});

test('post-install OTA boot status helper validates activation metadata', () => {
  assert.equal(updateSlotName(UPDATE_SLOT_NONE), 'factory/none');
  assert.equal(updateSlotName(UPDATE_SLOT_OTA0), 'ota_0');
  assert.equal(updateOtaStateName(UPDATE_OTA_STATE_NEW), 'new');
  assert.equal(updateOtaStateName(UPDATE_OTA_STATE_INVALID), 'invalid');
  assert.deepEqual(getPostInstallUpdateBootStatusFailures(bootStatus()), []);
  assert.doesNotThrow(() => assertPostInstallUpdateBootStatus(bootStatus()));

  const failures = getPostInstallUpdateBootStatusFailures(bootStatus({
    partition_table_ok: false,
    ota1_present: false,
    current_slot: UPDATE_SLOT_NONE,
    ota_state: UPDATE_OTA_STATE_INVALID,
  }));
  assert.deepEqual(failures, [
    'partition table is not readable',
    'both OTA app slots must be present',
    'selected boot slot is factory/none, expected ota_0 or ota_1',
    'selected OTA image state is invalid, expected new',
  ]);
  assert.throws(
    () => assertPostInstallUpdateBootStatus(bootStatus({
      ota_data_present: false,
    })),
    /post-install activation validation failed: otadata partition is missing/,
  );
});

test('update stream status helper validates progress and manifest metadata', () => {
  const bundle = parseUpdateBundleJson(validBundle({ manifest: { image_size: 3 } }));
  const beginExpectation = {
    expectedActive: true,
    expectedManifestVerified: true,
    expectedImageVerified: false,
    expectedBytesReceived: 0,
  };
  const finishExpectation = {
    expectedActive: false,
    expectedManifestVerified: true,
    expectedImageVerified: true,
    expectedBytesReceived: 3,
  };

  assert.deepEqual(getUpdateStreamStatusFailures(updateStatus(bundle), bundle, beginExpectation), []);
  assert.doesNotThrow(() => assertUpdateStreamStatus(
    updateStatus(bundle, { active: false, image_verified: true, bytes_received: 3 }),
    bundle,
    'finish update stream',
    finishExpectation,
  ));

  const failures = getUpdateStreamStatusFailures(updateStatus(bundle, {
    active: false,
    manifest_verified: false,
    release_version: 6,
    bytes_received: 1,
  }), bundle, beginExpectation);
  assert.deepEqual(failures, [
    'active is no, expected yes',
    'manifest_verified is no, expected yes',
    'release_version is 6, expected 7',
    'bytes_received is 1, expected 0',
  ]);
  assert.throws(
    () => assertUpdateStreamStatus(
      updateStatus(bundle, { image_size: 2 }),
      bundle,
      'stream update chunk',
      { ...beginExpectation, expectedBytesReceived: 3 },
    ),
    /stream update chunk: invalid device update status: image_size is 2, expected 3; bytes_received is 0, expected 3/,
  );
});

test('fetchLatestUpdateRelease fetches and validates a hosted release', async () => {
  const firmware = new Uint8Array([9, 8, 7, 6]);
  const bundleJson = validBundle({
    manifest: {
      release_version: 8,
      image_size: firmware.length,
      image_sha256_hex: sha256Hex(firmware),
      git_commit: 'c'.repeat(40),
      tx_types_rev: 'd'.repeat(40),
    },
  });
  const calls = [];
  const fetchImpl = async (input, init) => {
    const url = input instanceof URL ? input.href : String(input);
    calls.push({ url, init });
    if (url === 'https://updates.example.test/releases/latest.json') {
      return responseFromJson({
        format: UPDATE_RELEASE_INDEX_FORMAT,
        bundle_url: 'nockster-fw.update.json',
        firmware_url: '../fw/nockster-fw.bin',
        release_version: 8,
        image_size: firmware.length,
        image_sha256_hex: sha256Hex(firmware),
        hardware_target: 'esp32s3-touch-lcd-1.47',
        build_profile: 'production',
        protocol_v: 1,
        git_commit: 'c'.repeat(40),
        tx_types_rev: 'd'.repeat(40),
      });
    }
    if (url === 'https://updates.example.test/releases/nockster-fw.update.json') {
      return responseFromText(JSON.stringify(bundleJson));
    }
    if (url === 'https://updates.example.test/fw/nockster-fw.bin') {
      return responseFromBytes(firmware);
    }
    throw new Error(`unexpected fetch URL ${url}`);
  };

  const release = await fetchLatestUpdateRelease('https://updates.example.test/releases/latest.json', {
    fetchImpl,
    origin: 'https://updates.example.test',
  });

  assert.equal(release.bundle.manifest.release_version, 8);
  assert.equal(release.bundleName, 'nockster-fw.update.json');
  assert.equal(release.firmwareName, 'nockster-fw.bin');
  assert.deepEqual(Array.from(release.firmware), Array.from(firmware));
  assert.equal(release.index.bundleUrl.href, 'https://updates.example.test/releases/nockster-fw.update.json');
  assert.deepEqual(calls.map((call) => call.url), [
    'https://updates.example.test/releases/latest.json',
    'https://updates.example.test/releases/nockster-fw.update.json',
    'https://updates.example.test/fw/nockster-fw.bin',
  ]);
  assert.ok(calls.every((call) => call.init.cache === 'no-store'));
  assert.ok(calls.every((call) => call.init.credentials === 'same-origin'));
});

test('fetchLatestUpdateRelease rejects non-local HTTP release indexes', async () => {
  await assert.rejects(
    () => fetchLatestUpdateRelease('http://updates.example.test/latest.json', {
      fetchImpl: async () => responseFromJson({}),
    }),
    /release index URL must use HTTPS/,
  );
});

test('fetchUpdateReleaseArtifacts validates bundle policy and firmware length before buffering', async () => {
  const firmware = new Uint8Array([1, 2, 3, 4]);
  const bundleJson = validBundle({
    manifest: {
      image_size: firmware.length,
      image_sha256_hex: sha256Hex(firmware),
    },
  });
  let firmwareFetched = false;
  const blockedFetch = async (input) => {
    const url = input instanceof URL ? input.href : String(input);
    if (url.endsWith('/bundle.json')) {
      return responseFromText(JSON.stringify(bundleJson));
    }
    if (url.endsWith('/firmware.bin')) {
      firmwareFetched = true;
      return responseFromBytes(firmware);
    }
    throw new Error(`unexpected fetch URL ${url}`);
  };

  await assert.rejects(
    () => fetchUpdateReleaseArtifacts(
      'https://updates.example.test/bundle.json',
      'https://updates.example.test/firmware.bin',
      {
        fetchImpl: blockedFetch,
        validateBundle: () => {
          throw new Error('bundle is incompatible with this device');
        },
      },
    ),
    /bundle is incompatible/,
  );
  assert.equal(firmwareFetched, false);

  const mismatchedLengthFetch = async (input) => {
    const url = input instanceof URL ? input.href : String(input);
    if (url.endsWith('/bundle.json')) {
      return responseFromText(JSON.stringify(bundleJson));
    }
    if (url.endsWith('/firmware.bin')) {
      return responseFromBytes(firmware, { headers: { 'content-length': '99' } });
    }
    throw new Error(`unexpected fetch URL ${url}`);
  };

  await assert.rejects(
    () => fetchUpdateReleaseArtifacts(
      'https://updates.example.test/bundle.json',
      'https://updates.example.test/firmware.bin',
      { fetchImpl: mismatchedLengthFetch },
    ),
    /server reports 99/,
  );
});

test('fetchUpdateReleaseArtifacts applies bearer token policy and headers', async () => {
  const firmware = new Uint8Array([5, 6, 7]);
  const bundleJson = validBundle({
    manifest: {
      image_size: firmware.length,
      image_sha256_hex: sha256Hex(firmware),
    },
  });
  const authHeaders = [];
  const credentials = [];
  const fetchImpl = async (input, init) => {
    const url = input instanceof URL ? input.href : String(input);
    authHeaders.push(init.headers?.get('authorization') ?? null);
    credentials.push(init.credentials);
    if (url.endsWith('/bundle.json')) {
      return responseFromText(JSON.stringify(bundleJson));
    }
    if (url.endsWith('/firmware.bin')) {
      return responseFromBytes(firmware);
    }
    throw new Error(`unexpected fetch URL ${url}`);
  };

  await assert.doesNotReject(() => fetchUpdateReleaseArtifacts(
    'https://updates.example.test/bundle.json',
    'https://updates.example.test/firmware.bin',
    {
      fetchImpl,
      bearerToken: ' secret ',
    },
  ));
  assert.deepEqual(authHeaders, ['Bearer secret', 'Bearer secret']);
  assert.deepEqual(credentials, ['omit', 'omit']);

  await assert.rejects(
    () => fetchUpdateReleaseArtifacts(
      'https://updates.example.test/bundle.json',
      'https://cdn.example.test/firmware.bin',
      {
        fetchImpl,
        bearerToken: 'secret',
      },
    ),
    /same origin/,
  );
});

test('fetchUpdateReleaseArtifacts lets callers explicitly override tokened credentials', async () => {
  const firmware = new Uint8Array([5, 6, 7]);
  const bundleJson = validBundle({
    manifest: {
      image_size: firmware.length,
      image_sha256_hex: sha256Hex(firmware),
    },
  });
  const credentials = [];
  const fetchImpl = async (input, init) => {
    const url = input instanceof URL ? input.href : String(input);
    credentials.push(init.credentials);
    if (url.endsWith('/bundle.json')) {
      return responseFromText(JSON.stringify(bundleJson));
    }
    if (url.endsWith('/firmware.bin')) {
      return responseFromBytes(firmware);
    }
    throw new Error(`unexpected fetch URL ${url}`);
  };

  await fetchUpdateReleaseArtifacts(
    'https://updates.example.test/bundle.json',
    'https://updates.example.test/firmware.bin',
    {
      fetchImpl,
      bearerToken: 'secret',
      bundleInit: { credentials: 'include' },
      firmwareInit: { credentials: 'include' },
    },
  );
  assert.deepEqual(credentials, ['include', 'include']);
});

test('fetchLatestUpdateRelease does not send bearer tokens to artifact origins from another index origin', async () => {
  const fetchImpl = async (input) => {
    const url = input instanceof URL ? input.href : String(input);
    if (url === 'https://updates.example.test/latest.json') {
      return responseFromJson({
        format: UPDATE_RELEASE_INDEX_FORMAT,
        bundle_url: 'https://cdn.example.test/bundle.json',
        firmware_url: 'https://cdn.example.test/firmware.bin',
      });
    }
    throw new Error(`unexpected fetch URL ${url}`);
  };

  await assert.rejects(
    () => fetchLatestUpdateRelease('https://updates.example.test/latest.json', {
      fetchImpl,
      bearerToken: 'secret',
    }),
    /requires index, bundle, and firmware URLs on the same origin/,
  );
});

test('fetchLatestUpdateRelease applies bearer headers and omits credentials across one private origin', async () => {
  const firmware = new Uint8Array([7, 7, 7]);
  const bundleJson = validBundle({
    manifest: {
      image_size: firmware.length,
      image_sha256_hex: sha256Hex(firmware),
    },
  });
  const authHeaders = [];
  const credentials = [];
  const fetchImpl = async (input, init) => {
    const url = input instanceof URL ? input.href : String(input);
    authHeaders.push(init.headers?.get('authorization') ?? null);
    credentials.push(init.credentials);
    if (url === 'https://updates.example.test/latest.json') {
      return responseFromJson({
        format: UPDATE_RELEASE_INDEX_FORMAT,
        bundle_url: 'bundle.json',
        firmware_url: 'firmware.bin',
        release_version: 7,
        image_size: firmware.length,
        image_sha256_hex: sha256Hex(firmware),
      });
    }
    if (url === 'https://updates.example.test/bundle.json') {
      return responseFromText(JSON.stringify(bundleJson));
    }
    if (url === 'https://updates.example.test/firmware.bin') {
      return responseFromBytes(firmware);
    }
    throw new Error(`unexpected fetch URL ${url}`);
  };

  await fetchLatestUpdateRelease('https://updates.example.test/latest.json', {
    fetchImpl,
    bearerToken: 'secret',
  });

  assert.deepEqual(authHeaders, ['Bearer secret', 'Bearer secret', 'Bearer secret']);
  assert.deepEqual(credentials, ['omit', 'omit', 'omit']);
});

test('assertPrivateUpdateReleaseUrls enforces bearer-token origin and transport policy', () => {
  assert.doesNotThrow(() => assertPrivateUpdateReleaseUrls(
    'http://updates.example.test/bundle.json',
    'http://other.example.test/firmware.bin',
    '',
  ));
  assert.doesNotThrow(() => assertPrivateUpdateReleaseUrls(
    'https://updates.example.test/bundle.json',
    'https://updates.example.test/firmware.bin',
    'secret',
  ));
  assert.doesNotThrow(() => assertPrivateUpdateReleaseUrls(
    'http://[::1]:3000/bundle.json',
    'http://[::1]:3000/firmware.bin',
    'secret',
  ));

  assert.throws(
    () => assertPrivateUpdateReleaseUrls(
      'https://updates.example.test/bundle.json',
      'https://cdn.example.test/firmware.bin',
      'secret',
    ),
    /same origin/,
  );
  assert.throws(
    () => assertPrivateUpdateReleaseUrls(
      'http://updates.example.test/bundle.json',
      'http://updates.example.test/firmware.bin',
      'secret',
    ),
    /requires HTTPS/,
  );
});
