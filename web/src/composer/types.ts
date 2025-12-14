export type NoteV1 = {
  id: string;
  nameFirst: string; // base58 hash
  nameLast: string; // base58 hash
  originPage: number;
  assets: number;
  version?: number;
};

export type AddressBookEntry = {
  id: string;
  alias: string;
  address: string; // base58 hash
  notes?: NoteV1[];
};
