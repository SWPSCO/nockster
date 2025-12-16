import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent } from 'react';
import {
  Background,
  Controls,
  Handle,
  MiniMap,
  Node,
  Edge,
  Position,
  ReactFlow,
  ReactFlowInstance,
  ReactFlowProvider,
  addEdge,
  useEdgesState,
  useNodesState,
  type Connection,
  type OnSelectionChangeFunc,
  type NodeTypes,
  type NodeProps,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { compose_tx_v1_recipient_address, compose_tx_v1_unsigned } from 'siger-wasm';
import type { AddressBookEntry, AddressKind, MultisigDescriptor, NoteV1 } from './types';
import { loadAddressBook, newId, saveAddressBook } from './storage';

import './Composer.css';

type AddressNodeData = {
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
type NoteNodeData = {
  entryId: string;
  noteId: string;
  assets: number;
  originPage: number;
  nameFirst: string;
  nameLast: string;
};
type TxNodeData = {
  onCompose?: () => void;
  composing?: boolean;
  lastError?: string;
  result?: {
    psnt: Uint8Array;
    filename: string;
    summaryJson: string;
  };
};

type UnitMode = 'n' | 'ℕ';

type AddressFlowNode = Node<AddressNodeData, 'address'>;
type NoteFlowNode = Node<NoteNodeData, 'note'>;
type TxFlowNode = Node<TxNodeData, 'tx'>;
type ComposerNode = AddressFlowNode | NoteFlowNode | TxFlowNode;
type ComposerEdge = Edge;

const NICKS_PER_NOCK = 1n << 16n; // 65536
const NOCK_DEC_SCALE = 10n ** 16n;

function shortHash(h: string, keep = 4): string {
  const s = (h ?? '').trim();
  if (s.length <= keep * 2 + 3) return s;
  return `${s.slice(0, keep)}...${s.slice(-keep)}`;
}

function parsePkhListText(text: string): string[] {
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

function formatAmountNoUnit(nicks: number, unit: UnitMode): string {
  const n = BigInt(Math.trunc(nicks));
  if (unit === 'n') return n.toString();
  return formatNicksAsNocksExact(n);
}

function formatAmountWithUnit(nicks: number, unit: UnitMode): string {
  return `${formatAmountNoUnit(nicks, unit)} ${unit}`;
}

function parseAmountTextToNicks(text: string, unit: UnitMode): { nicks: number } | { error: string } {
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

function formatSummaryJson(raw: string): string {
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

function downloadBytes(name: string, bytes: Uint8Array) {
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

function AddressNode({ data, unitMode }: NodeProps<AddressFlowNode> & { unitMode: UnitMode }) {
  const io = data.io ?? { isInput: false, isOutput: false };
  const showNotes = io.isInput || (!io.isOutput && data.noteCount > 0);
  const showAmount = io.isOutput || data.amount.trim().length > 0;

  const parsed = data.amount.trim() ? parseAmountTextToNicks(data.amount, unitMode) : null;

  return (
    <div className="node-card">
      <div className="node-header address">
        <span>{data.kind === 'multisig' ? 'Multisig' : 'Address'}</span>
      </div>
      <div className="node-body">
        <div>{data.alias}</div>
        <div className="node-mono">{shortHash(data.address)}</div>
        {data.kind === 'multisig' && data.multisig && (
          <div className="inspector-help">
            {data.multisig.m}-of-{data.multisig.pkhs.length}
          </div>
        )}
        {showNotes && (
          <div className="inspector-help">
            {data.noteCount} notes · {formatAmountWithUnit(data.total, unitMode)}
          </div>
        )}
        {showAmount && (
          <input
            className="node-input node-input-compact"
            placeholder={`amount (${unitMode})`}
            value={data.amount}
            onChange={(e) => data.onChangeAmount(e.target.value)}
          />
        )}
        {parsed && 'error' in parsed && <div className="validation-text">{parsed.error}</div>}
      </div>
      <Handle type="target" id="in" position={Position.Left} />
      <Handle type="source" id="out" position={Position.Right} />
    </div>
  );
}

function NoteNode({ data, unitMode }: NodeProps<NoteFlowNode> & { unitMode: UnitMode }) {
  return (
    <div className="node-card">
      <div className="node-header note">
        <span>Note</span>
      </div>
      <div className="node-body">
        <div className="inspector-help">
          {formatAmountWithUnit(data.assets, unitMode)} · p{data.originPage}
        </div>
        <div className="node-mono">
          {shortHash(data.nameFirst)} {shortHash(data.nameLast)}
        </div>
      </div>
      <Handle type="source" id="out" position={Position.Right} />
    </div>
  );
}

function TxNode({ data }: NodeProps<TxFlowNode>) {
  const composing = data.composing ?? false;
  const onCompose = data.onCompose;

  return (
    <div className="node-card">
      <div className="node-header tx">
        <span>Tx</span>
      </div>
      <div className="node-body">
        <div className="node-actions">
          <button
            className="btn btn-success btn-small"
            onClick={() => onCompose?.()}
            disabled={!onCompose || composing}
          >
            {composing ? 'composing...' : 'compose'}
          </button>
        </div>
        {data.result && (
          <div className="composer-group" style={{ marginTop: '0.5rem' }}>
            <div className="composer-row">
              <button
                className="btn btn-secondary btn-small"
                onClick={() => downloadBytes(data.result!.filename, data.result!.psnt)}
              >
                download .psnt
              </button>
            </div>
          </div>
        )}
        {data.lastError && <div className="validation-text">{data.lastError}</div>}
      </div>
      <Handle type="target" id="in" position={Position.Left} />
      <Handle type="source" id="out" position={Position.Right} />
    </div>
  );
}

function sumAssets(notes: NoteV1[]): number {
  return notes.reduce((acc, n) => acc + (Number(n.assets) || 0), 0);
}

export function Composer({ wasmReady }: { wasmReady: boolean }) {
  const [addressBook, setAddressBook] = useState<AddressBookEntry[]>(() => loadAddressBook());

  const unitStorageKey = 'siger.composer.unit.v1';
  const [unitModePinned, setUnitModePinned] = useState<boolean>(() => {
    const stored = localStorage.getItem(unitStorageKey);
    return stored === 'n' || stored === 'ℕ';
  });
  const [unitMode, setUnitMode] = useState<UnitMode>(() => {
    const stored = localStorage.getItem(unitStorageKey);
    if (stored === 'n' || stored === 'ℕ') return stored;

    const entries = loadAddressBook();
    const hasWholeNocks = entries.some((e) =>
      (e.notes ?? []).some((n) => (Number(n.assets) || 0) >= Number(NICKS_PER_NOCK))
    );
    return hasWholeNocks ? 'ℕ' : 'n';
  });
  const unitModeRef = useRef<UnitMode>(unitMode);

  const setUnitModeUser = useCallback(
    (next: UnitMode) => {
      setUnitMode(next);
      setUnitModePinned(true);
      localStorage.setItem(unitStorageKey, next);
    },
    [unitStorageKey]
  );

  useEffect(() => {
    unitModeRef.current = unitMode;
  }, [unitMode]);

  useEffect(() => {
    if (unitModePinned) return;
    const hasWholeNocks = addressBook.some((e) =>
      (e.notes ?? []).some((n) => (Number(n.assets) || 0) >= Number(NICKS_PER_NOCK))
    );
    const next = hasWholeNocks ? 'ℕ' : 'n';
    setUnitMode((current) => (current === next ? current : next));
  }, [addressBook, unitModePinned]);

  const [selectedEntryId, setSelectedEntryId] = useState<string>(addressBook[0]?.id ?? '');
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedNodes, setSelectedNodes] = useState<ComposerNode[]>([]);
  const [selectedEdges, setSelectedEdges] = useState<ComposerEdge[]>([]);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [inspectorDrag, setInspectorDrag] = useState<{ dx: number; dy: number }>({ dx: 0, dy: 0 });
  const [inspectorDragging, setInspectorDragging] = useState(false);
  const inspectorDragRef = useRef<{
    startX: number;
    startY: number;
    startDx: number;
    startDy: number;
  } | null>(null);

  const [addressAddOpen, setAddressAddOpen] = useState(false);
  const [noteAddOpen, setNoteAddOpen] = useState(false);

  const [entryFormAlias, setEntryFormAlias] = useState('');
  const [entryFormKind, setEntryFormKind] = useState<AddressKind>('pkh');
  const [entryFormAddress, setEntryFormAddress] = useState('');
  const [entryFormMultisigM, setEntryFormMultisigM] = useState('2');
  const [entryFormMultisigPkhs, setEntryFormMultisigPkhs] = useState('');
  const [entryFormError, setEntryFormError] = useState<string | null>(null);

  const [noteFormFirst, setNoteFormFirst] = useState('');
  const [noteFormLast, setNoteFormLast] = useState('');
  const [noteFormOriginPage, setNoteFormOriginPage] = useState('');
  const [noteFormAssets, setNoteFormAssets] = useState('');
  const [noteFormError, setNoteFormError] = useState<string | null>(null);

  const multisigPreview = useMemo(() => {
    if (entryFormKind !== 'multisig') return { address: '', error: null as string | null };
    if (!wasmReady) return { address: '', error: 'WASM not ready yet' };
    const m = Number(entryFormMultisigM.trim());
    const pkhs = parsePkhListText(entryFormMultisigPkhs);
    if (!Number.isFinite(m) || m < 1) return { address: '', error: 'm must be >= 1' };
    if (pkhs.length === 0) return { address: '', error: 'enter at least one signer pkh' };
    if (m > pkhs.length) return { address: '', error: 'm must be <= number of signers' };
    try {
      const address = compose_tx_v1_recipient_address({ m, pkhs });
      return { address, error: null };
    } catch (err: any) {
      return { address: '', error: err?.message ?? String(err) };
    }
  }, [entryFormKind, entryFormMultisigM, entryFormMultisigPkhs, wasmReady]);

  const nodeTypes: NodeTypes = useMemo(
    () => ({
      address: ((props: any) => <AddressNode {...props} unitMode={unitMode} />) as any,
      note: ((props: any) => <NoteNode {...props} unitMode={unitMode} />) as any,
      tx: TxNode as any,
    }),
    [unitMode]
  );

  const entryById = useCallback((id: string) => addressBook.find((e) => e.id === id) ?? null, [addressBook]);

  const noteById = useCallback(
    (entryId: string, noteId: string) => entryById(entryId)?.notes?.find((n) => n.id === noteId) ?? null,
    [entryById]
  );

  const [nodes, setNodes, onNodesChange] = useNodesState<ComposerNode>([
    { id: 'tx-1', type: 'tx', position: { x: 200, y: 120 }, data: {} as TxNodeData },
  ]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<ComposerEdge>([]);
  const flowRef = useRef<HTMLDivElement | null>(null);
  const rfInstance = useRef<ReactFlowInstance<ComposerNode, ComposerEdge> | null>(null);
  const nodesRef = useRef<ComposerNode[]>([]);
  const edgesRef = useRef<ComposerEdge[]>([]);
  const addressBookRef = useRef<AddressBookEntry[]>([]);
  const prevUnitModeRef = useRef<UnitMode>(unitMode);

  useEffect(() => {
    nodesRef.current = nodes;
  }, [nodes]);

  useEffect(() => {
    edgesRef.current = edges;
  }, [edges]);

  useEffect(() => {
    const nodesNow = nodesRef.current;
    const edgesNow = edgesRef.current;
    const typeById = new Map(nodesNow.map((n) => [n.id, n.type]));

    const ioById = new Map<string, { isInput: boolean; isOutput: boolean }>();
    const ensure = (id: string) => {
      const existing = ioById.get(id);
      if (existing) return existing;
      const next = { isInput: false, isOutput: false };
      ioById.set(id, next);
      return next;
    };

    for (const e of edgesNow) {
      const sourceType = typeById.get(e.source);
      const targetType = typeById.get(e.target);
      if (targetType === 'tx' && e.targetHandle === 'in' && sourceType === 'address') {
        ensure(e.source).isInput = true;
      }
      if (sourceType === 'tx' && e.sourceHandle === 'out' && targetType === 'address') {
        ensure(e.target).isOutput = true;
      }
    }

    setNodes((current) => {
      let changed = false;
      const next = current.map((n) => {
        if (n.type !== 'address') return n;
        const want = ioById.get(n.id) ?? { isInput: false, isOutput: false };
        const data = n.data as AddressNodeData;
        const have = data.io ?? { isInput: false, isOutput: false };
        if (have.isInput === want.isInput && have.isOutput === want.isOutput) return n;
        changed = true;
        return { ...n, data: { ...data, io: want } } as any;
      });
      return changed ? next : current;
    });
  }, [edges, nodes, setNodes]);

  useEffect(() => {
    addressBookRef.current = addressBook;
  }, [addressBook]);

  useEffect(() => {
    const prev = prevUnitModeRef.current;
    if (prev === unitMode) return;

    setNodes((current) =>
      current.map((n) => {
        if (n.type !== 'address') return n;
        const data = n.data as AddressNodeData;
        const raw = (data.amount ?? '').trim();
        if (!raw) return n;
        const parsed = parseAmountTextToNicks(raw, prev);
        if ('error' in parsed) return n;
        return {
          ...n,
          data: {
            ...data,
            amount: formatAmountNoUnit(parsed.nicks, unitMode),
          },
        } as any;
      })
    );

    prevUnitModeRef.current = unitMode;
  }, [setNodes, unitMode]);

  useEffect(() => {
    if (addressBook.length === 0) {
      if (selectedEntryId) setSelectedEntryId('');
      return;
    }
    if (!selectedEntryId || !addressBook.some((e) => e.id === selectedEntryId)) {
      setSelectedEntryId(addressBook[0].id);
    }
  }, [addressBook, selectedEntryId]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSelectedNodeId(null);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  useEffect(() => {
    if (selectedNodeId) return;
    setInspectorDrag({ dx: 0, dy: 0 });
    setInspectorDragging(false);
    inspectorDragRef.current = null;
  }, [selectedNodeId]);

  useEffect(() => {
    if (!inspectorDragging) return;

    const onMove = (e: MouseEvent) => {
      const ctx = inspectorDragRef.current;
      if (!ctx) return;
      const dx = ctx.startDx + (e.clientX - ctx.startX);
      const dy = ctx.startDy + (e.clientY - ctx.startY);
      setInspectorDrag({ dx, dy });
    };

    const onUp = () => {
      setInspectorDragging(false);
      inspectorDragRef.current = null;
    };

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [inspectorDragging]);

  const updateNodeData = useCallback(
    (nodeId: string, patch: Record<string, unknown>) => {
      setNodes((current) =>
        current.map((n) =>
          n.id === nodeId ? ({ ...n, data: { ...(n.data as any), ...patch } } as any) : n
        )
      );
    },
    [setNodes]
  );

  const onSelectionChange: OnSelectionChangeFunc<ComposerNode, ComposerEdge> = useCallback(
    ({ nodes, edges }) => {
      setSelectedNodes(nodes);
      setSelectedEdges(edges);
    },
    []
  );

  const deleteSelection = useCallback(() => {
    const nodeIds = new Set(selectedNodes.filter((n) => n.type !== 'tx').map((n) => n.id));
    const edgeIds = new Set(selectedEdges.map((e) => e.id));
    if (nodeIds.size === 0 && edgeIds.size === 0) return;

    setEdges((current) =>
      current.filter((e) => !edgeIds.has(e.id) && !nodeIds.has(e.source) && !nodeIds.has(e.target))
    );
    setNodes((current) => current.filter((n) => !nodeIds.has(n.id)));
    setSelectedNodes([]);
    setSelectedEdges([]);
    setSelectedNodeId((prev) => (prev && nodeIds.has(prev) ? null : prev));
  }, [selectedEdges, selectedNodes, setEdges, setNodes]);

  const resetCanvas = useCallback(() => {
    setEdges([]);
    setNodes([{ id: `tx-${newId()}`, type: 'tx', position: { x: 200, y: 120 }, data: {} as TxNodeData }]);
    setSelectedNodes([]);
    setSelectedEdges([]);
    setSelectedNodeId(null);
    queueMicrotask(() => rfInstance.current?.fitView?.({ padding: 0.3, maxZoom: 0.9 }));
  }, [setEdges, setNodes]);

  const ensureTxNodeData = useCallback(
    (nodeId: string) => {
      const existing = nodes.find((n) => n.id === nodeId);
      if (!existing) return;
      if (existing.type !== 'tx') return;

      const data = existing.data as Partial<TxNodeData>;
      if (typeof data.onCompose === 'function') return;

      updateNodeData(nodeId, {
        composing: false,
        onCompose: async () => {
          if (!wasmReady) {
            updateNodeData(nodeId, { lastError: 'WASM not ready yet' });
            return;
          }

          const edgesNow = edgesRef.current;
          const nodesNow = nodesRef.current;
          const entriesNow = addressBookRef.current;

          const inputEdges = edgesNow.filter((e) => e.target === nodeId && e.targetHandle === 'in');
          if (inputEdges.length === 0) {
            updateNodeData(nodeId, { lastError: 'Connect an Address or Note node to the tx' });
            return;
          }

          const inputEntryIds = new Set<string>();
          const noteRefs: { entryId: string; noteId: string }[] = [];
          for (const e of inputEdges) {
            const src = nodesNow.find((n) => n.id === e.source);
            if (!src) continue;
            if (src.type === 'address') {
              const data = src.data as AddressNodeData;
              if (data.entryId) inputEntryIds.add(data.entryId);
            } else if (src.type === 'note') {
              const data = src.data as NoteNodeData;
              if (data.entryId && data.noteId) noteRefs.push({ entryId: data.entryId, noteId: data.noteId });
            }
          }

          const allEntryIds = new Set<string>([...inputEntryIds, ...noteRefs.map((n) => n.entryId)]);
          if (allEntryIds.size === 0) {
            updateNodeData(nodeId, { lastError: 'No usable inputs found. Connect an Address or Note.' });
            return;
          }
          if (allEntryIds.size > 1) {
            updateNodeData(nodeId, { lastError: 'All inputs must belong to the same address.' });
            return;
          }

          const entryId = allEntryIds.values().next().value as string;
          const entry = entriesNow.find((e) => e.id === entryId) ?? null;
          if (!entry) {
            updateNodeData(nodeId, { lastError: 'Input address not found in address book.' });
            return;
          }
          if ((entry.kind ?? 'pkh') !== 'pkh') {
            updateNodeData(nodeId, { lastError: 'Multisig address entries cannot be used as inputs yet.' });
            return;
          }

          const availableNotes = entry.notes ?? [];
          let inputNotes: NoteV1[] = [];
          if (noteRefs.length) {
            const byId = new Map(availableNotes.map((n) => [n.id, n]));
            const missing: string[] = [];
            for (const ref of noteRefs) {
              const note = byId.get(ref.noteId);
              if (note) inputNotes.push(note);
              else missing.push(ref.noteId);
            }
            if (missing.length) {
              updateNodeData(nodeId, { lastError: `Some selected notes are missing: ${missing.join(', ')}` });
              return;
            }
          } else {
            inputNotes = availableNotes;
          }
          if (inputNotes.length === 0) {
            updateNodeData(nodeId, { lastError: 'No input notes available for this address.' });
            return;
          }

          const recipientEdges = edgesNow.filter((e) => e.source === nodeId && e.sourceHandle === 'out');
          type Recipient = string | { m: number; pkhs: string[] };
          const outputs: { recipient: Recipient; amount: number; alias?: string }[] = [];
          const outputErrors: string[] = [];
          for (const e of recipientEdges) {
            const rNode = nodesNow.find((n) => n.id === e.target && n.type === 'address');
            const rData = rNode?.data as AddressNodeData | undefined;
            if (!rData) continue;
            const address = (rData.address ?? '').trim();
            if (!address) {
              outputErrors.push('recipient missing address');
              continue;
            }

            const parsedAmount = parseAmountTextToNicks(rData.amount ?? '', unitModeRef.current);
            if ('error' in parsedAmount) {
              outputErrors.push(`${rData.alias ?? address}: ${parsedAmount.error}`);
              continue;
            }
            if (parsedAmount.nicks <= 0) {
              outputErrors.push(`${rData.alias ?? address}: amount must be > 0`);
              continue;
            }

            if (rData.kind === 'multisig') {
              if (!rData.multisig || !Array.isArray(rData.multisig.pkhs) || rData.multisig.pkhs.length === 0) {
                outputErrors.push(`${rData.alias ?? address}: multisig is missing signer pubkeys`);
                continue;
              }
              outputs.push({
                recipient: { m: rData.multisig.m, pkhs: rData.multisig.pkhs },
                amount: parsedAmount.nicks,
                alias: rData.alias,
              });
            } else {
              outputs.push({ recipient: address, amount: parsedAmount.nicks, alias: rData.alias });
            }
          }

          if (outputErrors.length) {
            updateNodeData(nodeId, { lastError: outputErrors.join('\n') });
            return;
          }

          if (!outputs.length) {
            updateNodeData(nodeId, { lastError: 'Connect at least one Address node as an output' });
            return;
          }

          updateNodeData(nodeId, { composing: true, lastError: undefined });
          try {
            const result = compose_tx_v1_unsigned({
              source_pkh: entry.address,
              notes: inputNotes.map((n) => ({
                name_first: n.nameFirst,
                name_last: n.nameLast,
                origin_page: n.originPage,
                assets: n.assets,
                version: n.version ?? 1,
              })),
              outputs,
            });

            updateNodeData(nodeId, {
              composing: false,
              lastError: undefined,
              result: {
                filename: `draft-${Date.now()}.psnt`,
                psnt: result.wallet_jam,
                summaryJson: result.summary_json,
              },
            });
          } catch (err: any) {
            updateNodeData(nodeId, {
              composing: false,
              lastError: err?.message ?? String(err),
            });
          }
        },
      });
    },
    [nodes, updateNodeData, wasmReady]
  );

  useEffect(() => {
    nodes.filter((n) => n.type === 'tx').forEach((n) => ensureTxNodeData(n.id));
  }, [ensureTxNodeData, nodes]);

  const onConnect = useCallback(
    (params: Connection) => setEdges((eds) => addEdge({ ...params, animated: true }, eds)),
    [setEdges]
  );

  const isValidConnection = useCallback((conn: Connection | ComposerEdge) => {
    const source = conn.source;
    const target = conn.target;
    const sourceHandle = conn.sourceHandle ?? null;
    const targetHandle = conn.targetHandle ?? null;
    if (!source || !target) return false;
    if (source === target) return false;

    const nodesNow = nodesRef.current;
    const typeById = new Map(nodesNow.map((n) => [n.id, n.type]));
    const sourceType = typeById.get(source);
    const targetType = typeById.get(target);

    if (targetType === 'tx' && targetHandle === 'in' && (sourceType === 'address' || sourceType === 'note')) {
      return true;
    }

    if (sourceType === 'tx' && sourceHandle === 'out' && targetType === 'address') {
      return true;
    }

    return false;
  }, []);

  const onDragOver = useCallback((event: DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  }, []);

  const onDrop = useCallback(
    (event: DragEvent) => {
      event.preventDefault();
      if (!rfInstance.current) return;

      const raw = event.dataTransfer.getData('application/siger-node');
      if (!raw) return;
      const parsed = JSON.parse(raw) as {
        kind: string;
        entryId?: string;
        noteId?: string;
      };

      const position = rfInstance.current.screenToFlowPosition({
        x: event.clientX,
        y: event.clientY,
      });

      if (parsed.kind === 'address' && parsed.entryId) {
        const entry = entryById(parsed.entryId);
        if (!entry) return;
        const kind: AddressKind = entry.kind ?? 'pkh';
        const notes = kind === 'pkh' ? (entry.notes ?? []) : [];
        const id = `address-${newId()}`;
        setNodes((ns) =>
          ns.concat({
            id,
            type: 'address',
            position,
            data: {
              entryId: entry.id,
              alias: entry.alias,
              kind,
              address: entry.address,
              multisig: kind === 'multisig' ? entry.multisig : undefined,
              noteCount: notes.length,
              total: sumAssets(notes),
              amount: '',
              onChangeAmount: (next: string) => updateNodeData(id, { amount: next }),
            } satisfies AddressNodeData,
          })
        );
      } else if (parsed.kind === 'note' && parsed.entryId && parsed.noteId) {
	        const note = noteById(parsed.entryId, parsed.noteId);
	        if (!note) return;
	        const id = `note-${newId()}`;
	        setNodes((ns) =>
	          ns.concat({
	            id,
	            type: 'note',
	            position,
	            data: {
	              entryId: parsed.entryId!,
	              noteId: note.id,
	              assets: note.assets,
	              originPage: note.originPage,
	              nameFirst: note.nameFirst,
	              nameLast: note.nameLast,
	            } satisfies NoteNodeData,
	          })
	        );
      }
    },
    [entryById, noteById, setNodes, updateNodeData]
  );

  const selectedEntry = selectedEntryId ? entryById(selectedEntryId) : null;

  const addEntry = useCallback(async () => {
    const alias = entryFormAlias.trim();
    if (!alias) {
      setEntryFormError('Alias is required');
      return;
    }

    const kind: AddressKind = entryFormKind;
    let entry: AddressBookEntry;

    if (kind === 'multisig') {
      if (!wasmReady) {
        setEntryFormError('WASM not ready yet');
        return;
      }
      const m = Number(entryFormMultisigM.trim());
      const pkhs = parsePkhListText(entryFormMultisigPkhs);
      if (!Number.isFinite(m) || m < 1) {
        setEntryFormError('m must be >= 1');
        return;
      }
      if (pkhs.length === 0) {
        setEntryFormError('enter at least one signer pkh');
        return;
      }
      if (m > pkhs.length) {
        setEntryFormError('m must be <= number of signers');
        return;
      }
      let address: string;
      try {
        address = compose_tx_v1_recipient_address({ m, pkhs });
      } catch (err: any) {
        setEntryFormError(err?.message ?? String(err));
        return;
      }
      entry = { id: newId(), alias, kind, address, multisig: { m, pkhs } };
    } else {
      const address = entryFormAddress.trim();
      if (!address) {
        setEntryFormError('Address is required');
        return;
      }
      entry = { id: newId(), alias, kind: 'pkh', address, notes: [] };
    }

    const next = [...addressBook, entry];
    setAddressBook(next);
    saveAddressBook(next);
    setSelectedEntryId(entry.id);
    setEntryFormAlias('');
    setEntryFormKind('pkh');
    setEntryFormAddress('');
    setEntryFormMultisigM('2');
    setEntryFormMultisigPkhs('');
    setEntryFormError(null);
    setAddressAddOpen(false);
  }, [
    addressBook,
    entryFormAddress,
    entryFormAlias,
    entryFormKind,
    entryFormMultisigM,
    entryFormMultisigPkhs,
    wasmReady,
  ]);

  const removeEntry = useCallback(
    (entryId: string) => {
      const next = addressBook.filter((e) => e.id !== entryId);
      setAddressBook(next);
      saveAddressBook(next);
      if (selectedEntryId === entryId) {
        setSelectedEntryId(next[0]?.id ?? '');
      }
    },
    [addressBook, selectedEntryId]
  );

  const addNoteToSelectedEntry = useCallback(() => {
    if (!selectedEntry) {
      setNoteFormError('Select an address first');
      return;
    }
    if ((selectedEntry.kind ?? 'pkh') !== 'pkh') {
      setNoteFormError('Notes can only be added to single-sig (pkh) entries');
      return;
    }

    const first = noteFormFirst.trim();
    const last = noteFormLast.trim();
    const originPage = Number(noteFormOriginPage.trim());
    const assetsParsed = parseAmountTextToNicks(noteFormAssets, unitModeRef.current);

    if (!first || !last) {
      setNoteFormError('Note name_first + name_last are required');
      return;
    }
    if (!Number.isFinite(originPage) || originPage < 0) {
      setNoteFormError('origin_page must be a non-negative number');
      return;
    }
    if ('error' in assetsParsed) {
      setNoteFormError(`assets: ${assetsParsed.error}`);
      return;
    }
    const assets = assetsParsed.nicks;
    if (assets <= 0) {
      setNoteFormError('assets must be > 0');
      return;
    }

    const note: NoteV1 = {
      id: newId(),
      nameFirst: first,
      nameLast: last,
      originPage,
      assets,
      version: 1,
    };

    const next = addressBook.map((e) =>
      e.id === selectedEntry.id ? { ...e, notes: [...(e.notes ?? []), note] } : e
    );
    setAddressBook(next);
    saveAddressBook(next);
    setNoteFormFirst('');
    setNoteFormLast('');
    setNoteFormOriginPage('');
    setNoteFormAssets('');
    setNoteFormError(null);
    setNoteAddOpen(false);
  }, [addressBook, noteFormAssets, noteFormFirst, noteFormLast, noteFormOriginPage, selectedEntry]);

  const removeNote = useCallback(
    (entryId: string, noteId: string) => {
      const next = addressBook.map((e) =>
        e.id === entryId ? { ...e, notes: (e.notes ?? []).filter((n) => n.id !== noteId) } : e
      );
      setAddressBook(next);
      saveAddressBook(next);
    },
    [addressBook]
  );

  const dragData = (payload: any) => JSON.stringify(payload);

  return (
    <>
      <div className={`composer-layout ${sidebarCollapsed ? 'sidebar-collapsed' : ''}`}>
        {!sidebarCollapsed && (
          <div className="composer-sidebar">
            <div className="composer-row" style={{ justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ fontWeight: 700 }}>Library</div>
              <div className="composer-row" style={{ alignItems: 'center' }}>
                <div className="composer-unit-toggle" role="group" aria-label="unit">
                  <button
                    type="button"
                    className={`composer-unit-btn ${unitMode === 'n' ? 'active' : ''}`}
                    onClick={() => setUnitModeUser('n')}
                  >
                    n
                  </button>
                  <button
                    type="button"
                    className={`composer-unit-btn ${unitMode === 'ℕ' ? 'active' : ''}`}
                    onClick={() => setUnitModeUser('ℕ')}
                  >
                    ℕ
                  </button>
                </div>
                <button
                  type="button"
                  className="btn btn-small btn-secondary"
                  onClick={() => setSidebarCollapsed(true)}
                >
                  hide
                </button>
              </div>
            </div>

            <details className="composer-details" open>
              <summary className="composer-summary">
                <span>Canvas</span>
              </summary>
              <div className="composer-details-body">
                <div className="composer-row">
                  <button
                    type="button"
                    className="btn btn-small btn-danger"
                    onClick={deleteSelection}
                    disabled={selectedNodes.length === 0 && selectedEdges.length === 0}
                  >
                    delete selected
                  </button>
                  <button type="button" className="btn btn-small btn-secondary" onClick={resetCanvas}>
                    reset
                  </button>
                </div>
                <div className="inspector-help">
                  Connect: Address/Note → Tx → Address. Drag an address into the canvas, then wire it as an input or
                  output depending on which side you connect. Click a node to open inspector.
                </div>
              </div>
            </details>

            <details className="composer-details" open>
              <summary className="composer-summary">
                <span>Address book ({addressBook.length})</span>
              </summary>
              <div className="composer-details-body">
                <details
                  className="composer-subdetails"
                  open={addressAddOpen}
                  onToggle={(e) => setAddressAddOpen(e.currentTarget.open)}
                >
                  <summary className="composer-subsummary">Add address</summary>
                  <div className="composer-subbody">
                    <input
                      className="node-input"
                      placeholder="alias"
                      value={entryFormAlias}
                      onChange={(e) => {
                        setEntryFormAlias(e.target.value);
                        if (entryFormError) setEntryFormError(null);
                      }}
                    />
                    <select
                      className="node-input"
                      value={entryFormKind}
                      onChange={(e) => {
                        setEntryFormKind(e.target.value === 'multisig' ? 'multisig' : 'pkh');
                        if (entryFormError) setEntryFormError(null);
                      }}
                    >
                      <option value="pkh">single-sig (pkh)</option>
                      <option value="multisig">multisig (pkh lock)</option>
                    </select>
                    {entryFormKind === 'multisig' ? (
                      <>
                        <input
                          className="node-input"
                          placeholder="m (threshold)"
                          type="number"
                          min={1}
                          value={entryFormMultisigM}
                          onChange={(e) => {
                            setEntryFormMultisigM(e.target.value);
                            if (entryFormError) setEntryFormError(null);
                          }}
                        />
                        <textarea
                          className="node-input"
                          placeholder="signer pkhs (base58), one per line or space-separated"
                          rows={4}
                          value={entryFormMultisigPkhs}
                          onChange={(e) => {
                            setEntryFormMultisigPkhs(e.target.value);
                            if (entryFormError) setEntryFormError(null);
                          }}
                        />
                        <input
                          className="node-input node-input-compact"
                          placeholder="computed address (lock_root)"
                          value={multisigPreview.address || ''}
                          readOnly
                        />
                        {multisigPreview.error && <div className="validation-text">{multisigPreview.error}</div>}
                      </>
                    ) : (
                    <input
                      className="node-input"
                      placeholder="address pkh (base58)"
                      value={entryFormAddress}
                      onChange={(e) => {
                        setEntryFormAddress(e.target.value);
                        if (entryFormError) setEntryFormError(null);
                      }}
                    />
                    )}
                    <div className="composer-row">
                      <button type="button" className="btn btn-small btn-secondary" onClick={addEntry}>
                        add
                      </button>
                      <button
                        type="button"
                        className="btn btn-small btn-secondary"
                        onClick={() => {
                          setEntryFormAlias('');
                          setEntryFormKind('pkh');
                          setEntryFormAddress('');
                          setEntryFormMultisigM('2');
                          setEntryFormMultisigPkhs('');
                          setEntryFormError(null);
                          setAddressAddOpen(false);
                        }}
                      >
                        cancel
                      </button>
                    </div>
                    {entryFormError && <div className="validation-text">{entryFormError}</div>}
                  </div>
                </details>

                {addressBook.length === 0 ? (
                  <div className="inspector-help">Add an address, then drag it into the canvas.</div>
                ) : (
                  <div className="composer-list">
                    {addressBook.map((entry) => {
                      const kind: AddressKind = entry.kind ?? 'pkh';
                      const notes = kind === 'pkh' ? (entry.notes ?? []) : [];
                      const multisig = kind === 'multisig' ? entry.multisig : undefined;
                      return (
                        <div
                          key={entry.id}
                          className={`composer-item ${entry.id === selectedEntryId ? 'selected' : ''}`}
                          draggable
                          onDragStart={(e) =>
                            e.dataTransfer.setData(
                              'application/siger-node',
                              dragData({ kind: 'address', entryId: entry.id })
                            )
                          }
                          onClick={() => setSelectedEntryId(entry.id)}
                          role="button"
                          tabIndex={0}
                        >
                          <div className="composer-item-title">
                            <span>{entry.alias}</span>
                            <button
                              type="button"
                              className="btn btn-small btn-danger"
                              draggable={false}
                              onClick={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                removeEntry(entry.id);
                              }}
                            >
                              remove
                            </button>
                          </div>
                          <div className="composer-item-meta">{shortHash(entry.address)}</div>
                          {kind === 'multisig' && multisig ? (
                            <div className="composer-item-meta">
                              multisig: {multisig.m}-of-{multisig.pkhs.length}
                            </div>
                          ) : (
                            <div className="composer-item-meta">
                              notes: {notes.length} · total: {formatAmountWithUnit(sumAssets(notes), unitMode)}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </details>

            {selectedEntry && (selectedEntry.kind ?? 'pkh') === 'pkh' && (
              <details className="composer-details" open>
                <summary className="composer-summary">
                  <span>Notes ({(selectedEntry.notes ?? []).length})</span>
                </summary>
                <div className="composer-details-body">
                  <details
                    className="composer-subdetails"
                    open={noteAddOpen}
                    onToggle={(e) => setNoteAddOpen(e.currentTarget.open)}
                  >
                    <summary className="composer-subsummary">Add note</summary>
                    <div className="composer-subbody">
                      <input
                        className="node-input"
                        placeholder="name_first (base58)"
                        value={noteFormFirst}
                        onChange={(e) => {
                          setNoteFormFirst(e.target.value);
                          if (noteFormError) setNoteFormError(null);
                        }}
                      />
                      <input
                        className="node-input"
                        placeholder="name_last (base58)"
                        value={noteFormLast}
                        onChange={(e) => {
                          setNoteFormLast(e.target.value);
                          if (noteFormError) setNoteFormError(null);
                        }}
                      />
                      <div className="composer-row">
                        <input
                          className="node-input"
                          style={{ flex: 1 }}
                          type="number"
                          placeholder="origin_page"
                          value={noteFormOriginPage}
                          onChange={(e) => {
                            setNoteFormOriginPage(e.target.value);
                            if (noteFormError) setNoteFormError(null);
                          }}
                        />
                        <input
                          className="node-input"
                          style={{ flex: 1 }}
                          type="text"
                          inputMode={unitMode === 'ℕ' ? 'decimal' : 'numeric'}
                          placeholder={`assets (${unitMode})`}
                          value={noteFormAssets}
                          onChange={(e) => {
                            setNoteFormAssets(e.target.value);
                            if (noteFormError) setNoteFormError(null);
                          }}
                        />
                      </div>
                      <div className="composer-row">
                        <button type="button" className="btn btn-small btn-secondary" onClick={addNoteToSelectedEntry}>
                          add
                        </button>
                        <button
                          type="button"
                          className="btn btn-small btn-secondary"
                          onClick={() => {
                            setNoteFormFirst('');
                            setNoteFormLast('');
                            setNoteFormOriginPage('');
                            setNoteFormAssets('');
                            setNoteFormError(null);
                            setNoteAddOpen(false);
                          }}
                        >
                          cancel
                        </button>
                      </div>
                      {noteFormError && <div className="validation-text">{noteFormError}</div>}
                    </div>
                  </details>

                  {(selectedEntry.notes ?? []).length === 0 ? (
                    <div className="inspector-help">No notes yet for this address.</div>
                  ) : (
                    <div className="composer-list">
                      {(selectedEntry.notes ?? []).map((note) => (
                        <div
                          key={note.id}
                          className="composer-item"
                          draggable
                          onDragStart={(e) =>
                            e.dataTransfer.setData(
                              'application/siger-node',
                              dragData({ kind: 'note', entryId: selectedEntry.id, noteId: note.id })
                            )
                          }
                        >
                          <div className="composer-item-title">
                            <span>
                              {formatAmountWithUnit(note.assets, unitMode)} · p{note.originPage}
                            </span>
                            <button
                              type="button"
                              className="btn btn-small btn-danger"
                              draggable={false}
                              onClick={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                removeNote(selectedEntry.id, note.id);
                              }}
                            >
                              remove
                            </button>
                          </div>
                          <div className="composer-item-meta">
                            {shortHash(note.nameFirst)} {shortHash(note.nameLast)}
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </details>
            )}

            {selectedEntry && (selectedEntry.kind ?? 'pkh') === 'multisig' && (
              <details className="composer-details" open>
                <summary className="composer-summary">
                  <span>Multisig</span>
                </summary>
                <div className="composer-details-body">
                  <div className="inspector-help">
                    {(selectedEntry.multisig?.m ?? 0)}-of-{selectedEntry.multisig?.pkhs?.length ?? 0}
                  </div>
                  <div className="node-mono">{selectedEntry.address}</div>
                </div>
              </details>
            )}
          </div>
        )}

        <div className="composer-canvas" ref={flowRef}>
          {sidebarCollapsed && (
            <div className="composer-canvas-toolbar">
              <button type="button" className="btn btn-small btn-secondary" onClick={() => setSidebarCollapsed(false)}>
                library
              </button>
              <div className="composer-unit-toggle" role="group" aria-label="unit">
                <button
                  type="button"
                  className={`composer-unit-btn ${unitMode === 'n' ? 'active' : ''}`}
                  onClick={() => setUnitModeUser('n')}
                >
                  n
                </button>
                <button
                  type="button"
                  className={`composer-unit-btn ${unitMode === 'ℕ' ? 'active' : ''}`}
                  onClick={() => setUnitModeUser('ℕ')}
                >
                  ℕ
                </button>
              </div>
              <button
                type="button"
                className="btn btn-small btn-danger"
                onClick={deleteSelection}
                disabled={selectedNodes.length === 0 && selectedEdges.length === 0}
              >
                delete
              </button>
              <button type="button" className="btn btn-small btn-secondary" onClick={resetCanvas}>
                reset
              </button>
            </div>
          )}
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            isValidConnection={isValidConnection}
            onSelectionChange={onSelectionChange}
            deleteKeyCode={['Backspace', 'Delete']}
            onInit={(inst) => {
              rfInstance.current = inst;
            }}
            onNodeClick={(_, node) => setSelectedNodeId(node.id)}
            onPaneClick={() => setSelectedNodeId(null)}
            onDrop={onDrop}
            onDragOver={onDragOver}
            fitView
            fitViewOptions={{ padding: 0.3, maxZoom: 0.9 }}
          >
            <Background />
            <MiniMap pannable zoomable />
            <Controls />
          </ReactFlow>
        </div>
      </div>

      {selectedNodeId && (
        <div className="composer-modal-overlay" onClick={() => setSelectedNodeId(null)}>
          <div
            className="composer-modal"
            style={{ transform: `translate(${inspectorDrag.dx}px, ${inspectorDrag.dy}px)` }}
            onClick={(e) => {
              e.stopPropagation();
            }}
          >
            <div
              className="composer-modal-header"
              onMouseDown={(e) => {
                if (e.button !== 0) return;
                if ((e.target as HTMLElement | null)?.closest?.('button')) return;
                e.preventDefault();
                inspectorDragRef.current = {
                  startX: e.clientX,
                  startY: e.clientY,
                  startDx: inspectorDrag.dx,
                  startDy: inspectorDrag.dy,
                };
                setInspectorDragging(true);
              }}
            >
              <div className="composer-modal-title">Inspector</div>
              <button type="button" className="btn btn-small btn-secondary" onClick={() => setSelectedNodeId(null)}>
                close
              </button>
            </div>
            <div className="composer-modal-body">
              {(() => {
                const node = nodes.find((n) => n.id === selectedNodeId);
                if (!node) return <div className="inspector-help">Node not found.</div>;

                if (node.type === 'tx') {
                  const data = node.data as TxNodeData;
                  if (data.result) {
                    return (
                      <>
                        <div className="composer-row" style={{ marginBottom: '0.75rem' }}>
                          <button
                            type="button"
                            className="btn btn-small btn-secondary"
                            onClick={() => downloadBytes(data.result!.filename, data.result!.psnt)}
                          >
                            download .psnt
                          </button>
                        </div>
                        <pre className="inspector-pre">{formatSummaryJson(data.result.summaryJson)}</pre>
                      </>
                    );
                  }
                  if (data.lastError) return <div className="validation-text">{data.lastError}</div>;
                  return <div className="inspector-help">Compose the transaction to view the summary.</div>;
                }

                if (node.type === 'address') {
                  const data = node.data as AddressNodeData;
                  const parsed = data.amount.trim() ? parseAmountTextToNicks(data.amount, unitMode) : null;
                  return (
                    <div className="composer-list">
                      <div>
                        <strong>{data.alias}</strong>
                      </div>
                      <div className="node-mono">{data.address}</div>
                      {data.kind === 'multisig' && data.multisig ? (
                        <div className="inspector-help">
                          {data.multisig.m}-of-{data.multisig.pkhs.length}
                        </div>
                      ) : (
                        <div className="inspector-help">
                          {data.noteCount} notes · {formatAmountWithUnit(data.total, unitMode)}
                        </div>
                      )}
                      <div className="inspector-help">amount: {data.amount || '(unset)'}</div>
                      {parsed && 'nicks' in parsed && (
                        <div className="inspector-help">{formatAmountWithUnit(parsed.nicks, unitMode)}</div>
                      )}
                    </div>
                  );
                }

                if (node.type === 'note') {
                  const data = node.data as NoteNodeData;
                  return (
                    <div className="composer-list">
                      <div className="inspector-help">
                        {formatAmountWithUnit(data.assets, unitMode)} · p{data.originPage}
                      </div>
                      <div className="node-mono">
                        {data.nameFirst} {data.nameLast}
                      </div>
                    </div>
                  );
                }

                return <div className="inspector-help">Unknown node type.</div>;
              })()}
            </div>
          </div>
        </div>
      )}
    </>
  );
}

export function ComposerView({ wasmReady }: { wasmReady: boolean }) {
  return (
    <ReactFlowProvider>
      <Composer wasmReady={wasmReady} />
    </ReactFlowProvider>
  );
}
