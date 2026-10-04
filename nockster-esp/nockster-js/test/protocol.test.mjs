import test from 'node:test';
import assert from 'node:assert/strict';

import {
  FEATURE_DEVICE_REBOOT,
  serializeRequest,
} from '../dist/index.js';

test('Reboot request stays append-only after update protocol requests', () => {
  assert.equal(FEATURE_DEVICE_REBOOT, 1 << 13);
  assert.deepEqual([...serializeRequest({ type: 'Reboot' })], [40]);
});
