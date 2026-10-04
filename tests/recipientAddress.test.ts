import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseRecipientAddress } from '../packages/wallet/src/lib/utils/recipientAddress.ts';
import { submissionStatusLabel } from '../packages/wallet/src/lib/utils/submissionStatus.ts';
const address = 'AcsPkuhXQoGeEsF91yynpm1kcW17PQ2Z1MEozgx7YnDPkZwrtzLuuqd';
test('QR recipients accept an address or one Nockster payment recipient', () => {
  assert.equal(parseRecipientAddress(` ${address}\n`), address);
  assert.equal(parseRecipientAddress(`nockster://send?to=${address}&amount=1`), address);
  assert.equal(parseRecipientAddress(`web+nockster://pay?to=${address}`), address);
  for (const invalid of ['https://example.com', 'hello', `nockster://send?to=${address}&to=${address}`, `nockster://user@send?to=${address}`, 'nockster://account-link?code=anything', 'x'.repeat(4097)])
    assert.throws(() => parseRecipientAddress(invalid));
});
test('submission uncertainty displays as pending without implying confirmation', () => {
  assert.equal(submissionStatusLabel('unknown'), 'Pending');
  assert.equal(submissionStatusLabel('acknowledged'), 'Submitted');
  assert.equal(submissionStatusLabel('accepted'), 'Accepted');
  assert.equal(submissionStatusLabel('confirmed'), 'Confirmed');
});
