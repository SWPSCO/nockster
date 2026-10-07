import {
  parseNocksInput,
  formatNocksWithSeparator
} from '../../../../packages/wallet/src/lib/utils/nicks.ts';

export type Units = 'NOCK' | 'nicks';
export function formatComposerAmount(nicks: bigint | number, units: Units): string {
  if (units === 'nicks') return `${BigInt(nicks).toLocaleString('en-US')} nicks`;
  return `${formatNocksWithSeparator(BigInt(nicks), 8).replace(/\.?0+$/, '') || '0'} NOCK`;
}
export type Recipient = string | { m: number; pkhs: string[] };
export type LockBranch = {
  recipient: Recipient;
  hashlock?: string[];
  timelock?: { abs_min: number };
};
export type LockForm = {
  recipientKind: 'address' | 'multisig';
  address: string;
  threshold: string;
  signers: string;
  condition: 'plain' | 'timelock' | 'hashlock' | 'htlc' | 'burn';
  height: string;
  commitments: string;
  refundAddress: string;
  refundHeight: string;
  privateOutput: boolean;
};
export type OutputForm = LockForm & { id: number; amount: string };
export type Note = {
  name_first: string;
  name_last: string;
  origin_page: number;
  assets: number;
  version: number;
};
export type SourceForm = {
  kind: 'wallet' | 'address' | 'multisig' | 'htlc';
  walletId: string;
  address: string;
  threshold: string;
  signers: string;
  claimAddress: string;
  commitments: string;
  refundAddress: string;
  refundHeight: string;
  branch: 'claim' | 'refund';
};
export type Output = {
  recipient: Recipient;
  amount: number;
  timelock?: { abs_min: number };
  hashlock?: string[];
  burn?: boolean;
  lock_root_only?: boolean;
  or_branches?: LockBranch[];
};
export type ComposeInput = {
  source_pkh: string;
  source_multisig?: { m: number; pkhs: string[] };
  source_or_lock?: { branches: LockBranch[]; spend_branch: number };
  notes: Note[];
  outputs: Output[];
  current_height: number;
};
export type ComposeSummary = {
  total_fees: number;
  minimum_fee: number;
  inputs_used: Array<{ name_first: string; name_last: string; assets: number }>;
  spends: Array<{ input: string; fee: number; refund: number }>;
  outputs: Output[];
};
export type AddressValidator = (address: string) => boolean;

export function newOutput(id: number): OutputForm {
  return {
    id,
    amount: '',
    recipientKind: 'address',
    address: '',
    threshold: '2',
    signers: '',
    condition: 'plain',
    height: '',
    commitments: '',
    refundAddress: '',
    refundHeight: '',
    privateOutput: false
  };
}

export function wholeNumber(value: string, label: string, minimum = 0): number {
  if (!/^\d+$/.test(value.trim())) throw new Error(`${label} must be a whole number.`);
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < minimum)
    throw new Error(`${label} must be at least ${minimum} and within the supported range.`);
  return number;
}

export function amountInNicks(value: string, units: Units): number {
  if (units === 'nicks') return wholeNumber(value, 'Amount', 1);
  const amount = parseNocksInput(value);
  if (amount === null || amount <= 0n)
    throw new Error('Enter an amount greater than zero, within the supported range.');
  return Number(amount);
}

export function address(value: string, label: string, valid: AddressValidator): string {
  const trimmed = value.trim();
  if (!valid(trimmed)) throw new Error(`Enter a valid ${label}.`);
  return trimmed;
}

export function hashes(value: string, label: string, valid: AddressValidator): string[] {
  const entries = value
    .trim()
    .split(/[\s,]+/)
    .filter(Boolean);
  if (!entries.length) throw new Error(`Enter at least one ${label}.`);
  if (new Set(entries).size !== entries.length) throw new Error(`${label} entries must be unique.`);
  return entries.map(entry => address(entry, label, valid));
}

export function multisig(threshold: string, signers: string, valid: AddressValidator) {
  const pkhs = hashes(signers, 'signer address', valid);
  const m = wholeNumber(threshold, 'Required signatures', 1);
  if (m > pkhs.length) throw new Error('Required signatures cannot exceed the number of signers.');
  return { m, pkhs };
}

export function htlcBranches(
  claim: Recipient,
  commitments: string,
  refund: string,
  height: string,
  valid: AddressValidator
): LockBranch[] {
  return [
    { recipient: claim, hashlock: hashes(commitments, 'preimage commitment', valid) },
    {
      recipient: address(refund, 'refund address', valid),
      timelock: { abs_min: wholeNumber(height, 'Refund block height', 1) }
    }
  ];
}

export function outputFromForm(form: OutputForm, units: Units, valid: AddressValidator): Output {
  // Burn locks ignore the recipient; use a valid hash supplied by the caller.
  const recipient =
    form.recipientKind === 'multisig'
      ? multisig(form.threshold, form.signers, valid)
      : address(form.address, 'recipient address', valid);
  const output: Output = {
    recipient,
    amount: amountInNicks(form.amount, units),
    lock_root_only: form.privateOutput
  };
  if (form.condition === 'timelock')
    output.timelock = { abs_min: wholeNumber(form.height, 'Unlock block height', 1) };
  if (form.condition === 'hashlock')
    output.hashlock = hashes(form.commitments, 'preimage commitment', valid);
  if (form.condition === 'htlc')
    output.or_branches = htlcBranches(
      recipient,
      form.commitments,
      form.refundAddress,
      form.refundHeight,
      valid
    );
  if (form.condition === 'burn') output.burn = true;
  return output;
}

export function noteId(note: Note): string {
  return `${note.name_first}/${note.name_last}`;
}

export function validateNotes(notes: Note[], valid: AddressValidator): Note[] {
  if (!notes.length) throw new Error('Select at least one input note.');
  if (new Set(notes.map(noteId)).size !== notes.length)
    throw new Error('Each input note can only be selected once.');
  for (const note of notes) {
    address(note.name_first, 'first note hash', valid);
    address(note.name_last, 'last note hash', valid);
    if (note.version !== 1) throw new Error('Select V1 notes for this transaction.');
    if (
      !Number.isSafeInteger(note.assets) ||
      note.assets <= 0 ||
      !Number.isSafeInteger(note.origin_page) ||
      note.origin_page < 0
    )
      throw new Error('Note amounts and block heights must be valid whole numbers.');
  }
  return notes;
}
