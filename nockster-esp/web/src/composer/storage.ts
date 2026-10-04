import type { AddressBookEntry, AddressKind, MultisigDescriptor, NoteV1 } from './types';

const KEY_WALLETS = 'nockster.composer.wallets.v1';
const KEY_ADDRESS_BOOK = 'nockster.composer.addressBook.v1';

function safeParseJson<T>(raw: string | null, fallback: T): T {
  if (!raw) return fallback;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

type LegacyWallet = {
  id: string;
  alias: string;
  pkh: string;
  notes: NoteV1[];
};

export function loadAddressBook(): AddressBookEntry[] {
  const raw = safeParseJson<any[]>(localStorage.getItem(KEY_ADDRESS_BOOK), []);
  const entries: AddressBookEntry[] = raw
    .map((e) => {
      if (!e || typeof e !== 'object') return null;
      const id = typeof e.id === 'string' ? e.id : null;
      const alias = typeof e.alias === 'string' ? e.alias : null;
      const address = typeof e.address === 'string' ? e.address : null;
      if (!id || !alias || !address) return null;

      const kindRaw = (e as any).kind;
      const kind: AddressKind = kindRaw === 'multisig' ? 'multisig' : 'pkh';

      if (kind === 'multisig') {
        const ms = (e as any).multisig as Partial<MultisigDescriptor> | undefined;
        const m = Number(ms?.m);
        const pkhsRaw = Array.isArray(ms?.pkhs) ? (ms!.pkhs as any[]) : [];
        const pkhs = pkhsRaw.filter((v) => typeof v === 'string') as string[];
        if (!Number.isFinite(m) || m < 1) return null;
        if (pkhs.length === 0) return null;
        if (m > pkhs.length) return null;
        return {
          id,
          alias,
          kind,
          address,
          multisig: { m, pkhs },
        };
      }

      const notesRaw = Array.isArray((e as any).notes) ? ((e as any).notes as any[]) : [];
      const notes: NoteV1[] = notesRaw
        .map((n) => {
          if (!n || typeof n !== 'object') return null;
          const noteId = typeof n.id === 'string' ? n.id : null;
          const nameFirst = typeof n.nameFirst === 'string' ? n.nameFirst : null;
          const nameLast = typeof n.nameLast === 'string' ? n.nameLast : null;
          const originPage = Number((n as any).originPage);
          const assets = Number((n as any).assets);
          const version = Number((n as any).version);
          if (!noteId || !nameFirst || !nameLast) return null;
          if (!Number.isFinite(originPage) || originPage < 0) return null;
          if (!Number.isFinite(assets) || assets < 0) return null;
          const note: NoteV1 = {
            id: noteId,
            nameFirst,
            nameLast,
            originPage,
            assets,
          };
          if (Number.isFinite(version) && version > 0) note.version = version;
          return note;
        })
        .filter(Boolean) as NoteV1[];
      return { id, alias, kind, address, notes };
    })
    .filter(Boolean) as AddressBookEntry[];

  const legacyWallets = safeParseJson<LegacyWallet[]>(localStorage.getItem(KEY_WALLETS), []);
  if (legacyWallets.length) {
    for (const wallet of legacyWallets) {
      if (!wallet || typeof wallet !== 'object') continue;
      const address = typeof wallet.pkh === 'string' ? wallet.pkh : '';
      if (!address) continue;
      const existing = entries.find((e) => e.address === address);
      if (existing) {
        if (!existing.notes || existing.notes.length === 0) {
          existing.notes = Array.isArray(wallet.notes) ? wallet.notes : [];
        } else if (Array.isArray(wallet.notes) && wallet.notes.length) {
          const byId = new Map(existing.notes.map((n) => [n.id, n]));
          for (const note of wallet.notes) byId.set(note.id, note);
          existing.notes = Array.from(byId.values());
        }
      } else {
        entries.push({
          id: typeof wallet.id === 'string' ? wallet.id : newId(),
          alias: typeof wallet.alias === 'string' ? wallet.alias : 'wallet',
          kind: 'pkh',
          address,
          notes: Array.isArray(wallet.notes) ? wallet.notes : [],
        });
      }
    }

    saveAddressBook(entries);
    localStorage.removeItem(KEY_WALLETS);
  }

  return entries;
}

export function saveAddressBook(entries: AddressBookEntry[]) {
  localStorage.setItem(KEY_ADDRESS_BOOK, JSON.stringify(entries));
}

export function newId(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
