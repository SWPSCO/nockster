import type { NoteV1 } from './types';

const RPC_PROXY_PATH = '/__nockblocks/rpc';
export const NOCKBLOCKS_API_KEY_STORAGE_KEY = 'nockster.composer.nockblocks.apiKey.v1';

type JsonRpcEnvelope<T> = {
  jsonrpc?: string;
  id?: string | number | null;
  result?: T;
  error?: {
    code?: number;
    message?: string;
    data?: unknown;
  };
};

type NotesResult = {
  nicks?: number | string;
  notes?: unknown[];
  multisig?: unknown[];
};

export type ImportedNotes = {
  notes: NoteV1[];
  nicks: number;
  skipped: number;
  multisig: number;
};

export async function fetchNockblocksNotes(args: {
  address: string;
  apiKey: string;
}): Promise<ImportedNotes> {
  const address = args.address.trim();
  const apiKey = args.apiKey.trim();
  if (!address) throw new Error('address required');
  if (!apiKey) throw new Error('Nockblocks API key required');

  const response = await fetch(RPC_PROXY_PATH, {
    method: 'POST',
    headers: {
      accept: 'application/json',
      authorization: `Bearer ${apiKey}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify({
      jsonrpc: '2.0',
      method: 'getNotes',
      params: [{ address }],
      id: `notes-${Date.now()}`,
    }),
  });

  const envelope = (await response.json().catch(() => null)) as JsonRpcEnvelope<NotesResult> | null;
  if (!response.ok) {
    const message = envelope?.error?.message ?? response.statusText ?? 'request failed';
    throw new Error(`Nockblocks HTTP ${response.status}: ${message}`);
  }
  if (!envelope) throw new Error('Nockblocks returned invalid JSON');
  if (envelope.error) {
    throw new Error(envelope.error.message || `Nockblocks RPC error ${envelope.error.code ?? ''}`.trim());
  }

  const result = envelope.result ?? {};
  const standard = Array.isArray(result.notes) ? result.notes : [];
  const multisig = Array.isArray(result.multisig) ? result.multisig : [];
  const notes: NoteV1[] = [];
  let skipped = 0;

  for (const raw of standard) {
    const note = normalizeNote(raw);
    if (note) {
      notes.push(note);
    } else {
      skipped += 1;
    }
  }

  return {
    notes,
    nicks: toSafeNumber(result.nicks) ?? sumAssets(notes),
    skipped,
    multisig: multisig.length,
  };
}

function normalizeNote(raw: unknown): NoteV1 | null {
  if (!raw || typeof raw !== 'object') return null;
  const obj = raw as Record<string, unknown>;
  const version = toSafeNumber(obj.version) ?? 1;
  if (version !== 1) return null;

  const nameObj = valueObject(obj.name);
  const nameFirst = stringValue(
    nameObj?.firstName ?? nameObj?.first_name ?? nameObj?.nameFirst ?? obj.nameFirst ?? obj.name_first
  );
  const nameLast = stringValue(
    nameObj?.lastName ?? nameObj?.last_name ?? nameObj?.nameLast ?? obj.nameLast ?? obj.name_last
  );
  const originPage = toSafeNumber(obj.originPage ?? obj.origin_page);
  const assets = toSafeNumber(obj.assets);

  if (!nameFirst || !nameLast) return null;
  if (originPage === null || originPage < 0) return null;
  if (assets === null || assets <= 0) return null;

  return {
    id: `${nameFirst}:${nameLast}:${originPage}`,
    nameFirst,
    nameLast,
    originPage,
    assets,
    version: 1,
  };
}

function valueObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : null;
}

function stringValue(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

function toSafeNumber(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return Number.isSafeInteger(value) ? value : null;
  }
  if (typeof value === 'bigint') {
    return value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : null;
  }
  if (typeof value === 'string' && /^\d+$/.test(value.trim())) {
    const parsed = BigInt(value.trim());
    return parsed <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(parsed) : null;
  }
  return null;
}

function sumAssets(notes: NoteV1[]): number {
  return notes.reduce((acc, note) => acc + note.assets, 0);
}
