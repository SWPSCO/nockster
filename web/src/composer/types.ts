export type NoteV1 = {
  id: string;
  nameFirst: string; // base58 hash
  nameLast: string; // base58 hash
  originPage: number;
  assets: number;
  version?: number;
};

export type AddressKind = 'pkh' | 'multisig';

export type MultisigDescriptor = {
  m: number;
  pkhs: string[]; // base58 hashes
};

export type AddressBookEntry = {
  id: string;
  alias: string;
  kind?: AddressKind; // defaults to 'pkh' for legacy entries
  address: string; // base58 hash (pkh for single-sig; lock_root for multisig)
  multisig?: MultisigDescriptor;
  notes?: NoteV1[];
};
