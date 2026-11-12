import { multisig, type AddressValidator } from './composer.ts';

// A saved multisig: the lock's signers and required signature count under a
// name. The lock address identifies it. Nothing here is secret; the setup file
// lets co-signers load the same lock without retyping it.
export type SavedMultisig = { name: string; m: number; pkhs: string[]; address: string };
export type MultisigLock = { m: number; pkhs: string[] };
export type LockAddress = (lock: MultisigLock) => string;

export const SETUP_FORMAT = 'nockster-multisig-setup';
export const SETUP_VERSION = 1;
const MAX_NAME = 64;
const MAX_SETUP_BYTES = 64 * 1024;

function cleanName(value: unknown): string {
  const name = typeof value === 'string' ? value.trim().replace(/\s+/g, ' ') : '';
  if (!name) throw new Error('Enter a name for this multisig.');
  if (name.length > MAX_NAME)
    throw new Error(`Multisig names are limited to ${MAX_NAME} characters.`);
  return name;
}

export function savedMultisig(
  name: string,
  threshold: string,
  signers: string,
  valid: AddressValidator,
  lockAddress: LockAddress
): SavedMultisig {
  const lock = multisig(threshold, signers, valid);
  return { name: cleanName(name), ...lock, address: lockAddress(lock) };
}

// Keeps the stored entries that are still well formed. The address is
// recomputed so an edited store cannot pair a name with another lock.
export function storedMultisigs(
  value: unknown,
  valid: AddressValidator,
  lockAddress: LockAddress
): SavedMultisig[] {
  if (!Array.isArray(value)) return [];
  const wallets: SavedMultisig[] = [];
  for (const entry of value) {
    try {
      const { name, m, pkhs } = entry as Partial<SavedMultisig>;
      if (!Array.isArray(pkhs) || !pkhs.every(pkh => typeof pkh === 'string')) continue;
      wallets.push(savedMultisig(name as string, String(m), pkhs.join('\n'), valid, lockAddress));
    } catch {
      // Skip entries that no longer validate.
    }
  }
  return addMultisigs([], wallets);
}

// One entry per lock; saving a lock again renames it. Names stay unique so
// the picker is unambiguous.
export function addMultisigs(list: SavedMultisig[], added: SavedMultisig[]): SavedMultisig[] {
  let next = list;
  for (const wallet of added) {
    const clash = next.find(item => item.name === wallet.name && item.address !== wallet.address);
    if (clash) throw new Error(`Another multisig is already named “${wallet.name}”.`);
    next = [...next.filter(item => item.address !== wallet.address), wallet];
  }
  return [...next].sort((a, b) => a.name.localeCompare(b.name));
}

export function removeMultisig(list: SavedMultisig[], address: string): SavedMultisig[] {
  return list.filter(item => item.address !== address);
}

export function findMultisig(
  list: SavedMultisig[],
  threshold: string,
  signers: string,
  valid: AddressValidator,
  lockAddress: LockAddress
): SavedMultisig | undefined {
  try {
    const address = lockAddress(multisig(threshold, signers, valid));
    return list.find(item => item.address === address);
  } catch {
    return undefined;
  }
}

export function setupFile(wallet: SavedMultisig): string {
  return `${JSON.stringify(
    {
      format: SETUP_FORMAT,
      version: SETUP_VERSION,
      name: wallet.name,
      required_signatures: wallet.m,
      signers: wallet.pkhs,
      address: wallet.address
    },
    null,
    2
  )}\n`;
}

export function setupFileName(wallet: SavedMultisig): string {
  const slug = wallet.name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return `${slug || 'multisig'}.multisig.json`;
}

// Reads a setup file from a co-signer. The address in the file must match the
// address this wallet derives from the signers and threshold, so a damaged or
// altered file is refused instead of producing a different lock.
export function multisigFromSetup(
  text: string,
  valid: AddressValidator,
  lockAddress: LockAddress
): SavedMultisig {
  if (text.length > MAX_SETUP_BYTES)
    throw new Error('This file is too large to be a multisig setup.');
  let setup: Record<string, unknown>;
  try {
    setup = JSON.parse(text);
  } catch {
    throw new Error('This file is not a multisig setup.');
  }
  if (!setup || typeof setup !== 'object' || setup.format !== SETUP_FORMAT)
    throw new Error('This file is not a multisig setup.');
  if (setup.version !== SETUP_VERSION)
    throw new Error('This multisig setup was made by a newer version of Nockster.');
  const signers = setup.signers;
  if (!Array.isArray(signers) || !signers.every(signer => typeof signer === 'string'))
    throw new Error('The multisig setup does not list its signers.');
  if (!Number.isSafeInteger(setup.required_signatures))
    throw new Error('The multisig setup does not state its required signatures.');
  const wallet = savedMultisig(
    setup.name as string,
    String(setup.required_signatures),
    signers.join('\n'),
    valid,
    lockAddress
  );
  if (setup.address !== wallet.address)
    throw new Error('The address in this setup does not match its signers. Ask for a new copy.');
  return wallet;
}
