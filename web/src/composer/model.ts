// Composer data model + pure helpers, extracted from Composer.tsx so the main
// component file holds wiring/JSX and this holds the (testable, React-free)
// types and formatting/parsing logic. Behavior is unchanged — verbatim move.
import type { Node, Edge } from '@xyflow/react';
import type { AddressKind, MultisigDescriptor, NoteV1, WalletAddress } from './types';

export type AddressNodeData = {
  entryId: string;
  alias: string;
  kind: AddressKind;
  address: string;
  multisig?: MultisigDescriptor;
  noteCount: number;
  total: number;
  amount: string;
  onChangeAmount: (next: string) => void;
  io?: { isInput: boolean; isOutput: boolean };
};
export type NoteNodeData = {
  entryId: string;
  noteId: string;
  assets: number;
  originPage: number;
  nameFirst: string;
  nameLast: string;
};
export type TxNodeData = {
  onCompose?: () => void;
  onSignDraft?: (draft: ComposedDraft) => void | Promise<void>;
  composing?: boolean;
  signingDraft?: boolean;
  canSignDraft?: boolean;
  signDraftDisabledReason?: string;
  lastError?: string;
  result?: ComposedDraft;
};

export type ComposedDraft = {
  psnt: Uint8Array;
  filename: string;
  summaryJson: string;
};

// `recipient` mirrors the wasm RecipientInput: a base58 pkh string, or a
// multisig object. This is the lock the device will display and verify.
export type SummaryRecipient = string | { m?: number; pkhs?: string[] };
export type ComposeSummary = {
  outputs?: Array<{ amount?: number; alias?: string; recipient?: SummaryRecipient }>;
  total_fees?: number;
  minimum_fee?: number;
  inputs_used?: Array<{ assets?: number }>;
};

export type ReviewPrimitive =
  | { kind: 'pkh'; m: number; n: number }
  | { kind: 'timelock'; abs_min?: number; abs_max?: number; rel_min?: number; rel_max?: number }
  | { kind: 'hax'; n: number }
  | { kind: 'burn' };
export type ReviewOutput = {
  recipient_b58: string;
  gift: number;
  is_refund: boolean;
  bridge_evm_addr?: string | null;
  lock?: ReviewPrimitive[] | null;
};
export type DraftReview = {
  outputs: ReviewOutput[];
  input_count: number;
  external_total: number;
  refund_total: number;
  fee_total: number;
  multisig_inputs: Array<{
    m: number;
    n: number;
    present: number;
    we_authorized: boolean;
    we_signed: boolean;
  }>;
};

export type ImportedTxPreview = {
  name: string;
  info: {
    tx_id: string;
    shape: string;
    version: number;
    input_count: number;
  };
  details: PreviewTxDetails;
  review?: DraftReview | null;
};

export type PreviewSeed = {
  gift?: unknown;
  recipient_pkh?: unknown;
  lock_root?: unknown;
  parent_hash?: unknown;
};

export type PreviewSpend = {
  name_first?: unknown;
  name_last?: unknown;
  fee?: unknown;
  seeds?: PreviewSeed[];
};

export type PreviewTxDetails = {
  version?: unknown;
  transaction_id?: unknown;
  spend_count?: unknown;
  spends?: PreviewSpend[];
};

export type PreviewNodeData = {
  label: string;
  title: string;
  meta?: string[];
  mono?: string;
  copyValue?: string;
  copyLabel?: string;
  feeNicks?: number;
  giftNicks?: number;
};

export type UnitMode = 'n' | 'ℕ';
export type ComposerSidebarPanel = 'send' | 'wallet' | 'preview' | 'canvas';

export type AddressFlowNode = Node<AddressNodeData, 'address'>;
export type NoteFlowNode = Node<NoteNodeData, 'note'>;
export type TxFlowNode = Node<TxNodeData, 'tx'>;
export type PreviewFlowNode = Node<PreviewNodeData, 'preview'>;
export type ComposerNode = AddressFlowNode | NoteFlowNode | TxFlowNode | PreviewFlowNode;
export type ComposerEdge = Edge;

export const NICKS_PER_NOCK = 1n << 16n; // 65536
const NOCK_DEC_SCALE = 10n ** 16n;

/** Device-parity description of an output's lock: short badge + address. */
export function describeRecipient(recipient: SummaryRecipient | undefined): {
  badge: string;
  address: string;
} {
  if (recipient && typeof recipient === 'object') {
    const n = Array.isArray(recipient.pkhs) ? recipient.pkhs.length : 0;
    const m = Number(recipient.m) || 0;
    return { badge: `${m}-of-${n} multisig`, address: recipient.pkhs?.[0] ?? '' };
  }
  return { badge: 'p2pkh', address: typeof recipient === 'string' ? recipient : '' };
}

export function shortAddr(addr: string): string {
  if (addr.length <= 16) return addr;
  return `${addr.slice(0, 8)}…${addr.slice(-6)}`;
}

/** Verbatim lock-primitive description matching the device's tap-to-expand. */
export function describePrimitive(p: ReviewPrimitive): string {
  switch (p.kind) {
    case 'pkh':
      return p.n > p.m ? `${p.m}-of-${p.n} multisig` : 'p2pkh';
    case 'timelock': {
      const parts: string[] = [];
      if (p.abs_min != null) parts.push(`abs>=${p.abs_min}`);
      if (p.abs_max != null) parts.push(`abs<=${p.abs_max}`);
      if (p.rel_min != null) parts.push(`rel>=${p.rel_min}`);
      if (p.rel_max != null) parts.push(`rel<=${p.rel_max}`);
      return `timelock ${parts.join(' ')}`.trim();
    }
    case 'hax':
      return `hashlock x${p.n}`;
    case 'burn':
      return 'burn';
  }
}

export function describeReviewOutputBadge(output: ReviewOutput): string {
  if (output.bridge_evm_addr) return 'bridge';
  const prims = output.lock ?? [];
  const pkh = prims.find((p): p is Extract<ReviewPrimitive, { kind: 'pkh' }> => p.kind === 'pkh');
  if (pkh && pkh.n > pkh.m) return `${pkh.m}-of-${pkh.n}`;
  if (prims.some((p) => p.kind === 'timelock')) return 'timelock';
  if (prims.some((p) => p.kind === 'hax')) return 'hashlock';
  if (prims.some((p) => p.kind === 'burn')) return 'burn';
  return output.is_refund ? 'change' : 'p2pkh';
}

export function shortHash(h: string, keep = 4): string {
  const s = (h ?? '').trim();
  if (s.length <= keep * 2 + 3) return s;
  return `${s.slice(0, keep)}...${s.slice(-keep)}`;
}

// Label a wallet slot as "<nickname or abcd...wxyz> · slot N".
// Falls back to a truncated address when the alias is just the default "wallet slot N".
export function walletSlotLabel(wallet: WalletAddress): string {
  const nick = wallet.alias?.trim();
  const isDefault = !nick || /^wallet slot \d+$/i.test(nick);
  const base = isDefault ? shortHash(wallet.address, 4) : nick;
  return `${base} · slot ${wallet.slot}`;
}

// Display label for any address-book entry. Rewrites a stale default "wallet slot N"
// alias into "<abcd...wxyz> · slot N" so persisted entries read well without re-import.
export function entryDisplayLabel(entry: { alias?: string; address: string }): string {
  const alias = (entry.alias ?? '').trim();
  const slotMatch = alias.match(/^wallet slot (\d+)$/i);
  if (slotMatch) return `${shortHash(entry.address, 4)} · slot ${slotMatch[1]}`;
  return alias || shortHash(entry.address, 4);
}

export function parsePkhListText(text: string): string[] {
  return (text ?? '')
    .split(/[\s,]+/g)
    .map((s) => s.trim())
    .filter(Boolean);
}

function bigintToSafeNumber(value: bigint): number | null {
  const max = BigInt(Number.MAX_SAFE_INTEGER);
  if (value > max) return null;
  return Number(value);
}

function formatNicksAsNocksExact(nicks: bigint): string {
  const whole = nicks / NICKS_PER_NOCK;
  const frac = nicks % NICKS_PER_NOCK;
  if (frac === 0n) return whole.toString();

  const fracScaled = (frac * NOCK_DEC_SCALE) / NICKS_PER_NOCK;
  const fracStr = fracScaled.toString().padStart(16, '0').replace(/0+$/, '');
  return `${whole.toString()}.${fracStr}`;
}

export function formatAmountNoUnit(nicks: number, unit: UnitMode): string {
  const n = BigInt(Math.trunc(nicks));
  if (unit === 'n') return n.toString();
  return formatNicksAsNocksExact(n);
}

export function formatAmountWithUnit(nicks: number, unit: UnitMode): string {
  return `${formatAmountNoUnit(nicks, unit)} ${unit}`;
}

export function parseAmountTextToNicks(
  text: string,
  unit: UnitMode,
): { nicks: number } | { error: string } {
  const raw = (text ?? '').trim().replace(/[,_]/g, '');
  if (!raw) return { error: 'amount required' };

  if (unit === 'n') {
    if (!/^\d+$/.test(raw)) return { error: "amount must be an integer number of nicks ('n')" };
    const v = BigInt(raw);
    const asNumber = bigintToSafeNumber(v);
    if (asNumber === null) return { error: 'amount too large' };
    return { nicks: asNumber };
  }

  const m = raw.match(/^(\d+)(?:\.(\d+))?$/);
  if (!m) return { error: "amount must be a decimal number of nocks ('ℕ')" };
  const wholeStr = m[1] ?? '0';
  const fracStr = m[2] ?? '';
  if (fracStr.length > 16) return { error: 'too many decimal places (max 16)' };

  const whole = BigInt(wholeStr);
  const frac = fracStr ? BigInt(fracStr) : 0n;
  const denom = 10n ** BigInt(fracStr.length);
  const numer = whole * denom + frac;
  const scaled = numer * NICKS_PER_NOCK;
  if (scaled % denom !== 0n) {
    return { error: 'amount is not an exact multiple of 1/65536 ℕ (one nick)' };
  }
  const nicks = scaled / denom;
  const asNumber = bigintToSafeNumber(nicks);
  if (asNumber === null) return { error: 'amount too large' };
  return { nicks: asNumber };
}

export function formatSummaryJson(raw: string): string {
  const text = (raw ?? '').trim();
  if (!text) return '';
  try {
    const parsed: any = JSON.parse(text);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      delete parsed.tx_id;
      delete parsed.coinbase_rel_min;
    }
    return JSON.stringify(parsed, null, 2);
  } catch {
    return text;
  }
}

export function formatPreviewDetails(value: unknown): string {
  try {
    return JSON.stringify(normalizeWasmValue(value), null, 2);
  } catch {
    return String(value);
  }
}

function normalizeWasmValue(value: unknown): unknown {
  if (typeof value === 'bigint') return value.toString();
  if (value instanceof Map) {
    const result: Record<string, unknown> = {};
    for (const [key, mapValue] of value.entries()) {
      result[String(key)] = normalizeWasmValue(mapValue);
    }
    return result;
  }
  if (Array.isArray(value)) {
    return value.map(normalizeWasmValue);
  }
  if (value && typeof value === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, objectValue] of Object.entries(value as Record<string, unknown>)) {
      result[key] = normalizeWasmValue(objectValue);
    }
    return result;
  }
  return value;
}

export function normalizePreviewDetails(value: unknown): PreviewTxDetails {
  const normalized = normalizeWasmValue(value);
  if (!normalized || typeof normalized !== 'object' || Array.isArray(normalized)) {
    return {};
  }

  const details = normalized as PreviewTxDetails;
  return {
    ...details,
    spends: Array.isArray(details.spends) ? details.spends : [],
  };
}

export function previewNumber(value: unknown): number | null {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

export function previewString(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

export function parseComposeSummary(raw: string | undefined): ComposeSummary | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' ? (parsed as ComposeSummary) : null;
  } catch {
    return null;
  }
}

export function summaryOutputTotal(summary: ComposeSummary | null): number {
  return (summary?.outputs ?? []).reduce((acc, output) => acc + (Number(output.amount) || 0), 0);
}

export function summaryInputTotal(summary: ComposeSummary | null): number {
  return (summary?.inputs_used ?? []).reduce((acc, input) => acc + (Number(input.assets) || 0), 0);
}

export function isHighFeeSummary(summary: ComposeSummary | null): boolean {
  const fee = Number(summary?.total_fees) || 0;
  const minimum = Number(summary?.minimum_fee);
  if (!Number.isFinite(minimum)) return false;
  return fee > minimum;
}

export function downloadBytes(name: string, bytes: Uint8Array) {
  const ab = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(ab).set(bytes);
  const blob = new Blob([ab], { type: 'application/octet-stream' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

export function stopNodeInputEvent(event: { stopPropagation: () => void }) {
  event.stopPropagation();
}

export function sumAssets(notes: NoteV1[]): number {
  return notes.reduce((acc, n) => acc + (Number(n.assets) || 0), 0);
}
