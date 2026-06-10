import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent } from 'react';
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowInstance,
  ReactFlowProvider,
  addEdge,
  useEdgesState,
  useNodesState,
  type Connection,
  type OnSelectionChangeFunc,
  type NodeTypes,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import type { DeviceAddressBookEntry } from 'nockster-js';

import {
  ParsedTransaction,
  compose_tx_v1_recipient_address,
  compose_tx_v1_unsigned,
  review_draft,
  inspect_tx,
  merge_signed_tx,
  is_valid_pkh,
} from 'nockster-wasm';
import { TxTree } from './TxTree';
import type { TxTreeNode } from './model';

// One branch of an OR-composed lock (mirrors wasm LockBranch).
type OutputLockBranch = {
  recipient?: string | { m: number; pkhs: string[] };
  timelock?: { abs_min: number };
  hashlock?: string[];
  burn?: boolean;
};
import type { AddressBookEntry, AddressKind, NoteV1, WalletAddress } from './types';
import { loadAddressBook, newId, saveAddressBook } from './storage';
import {
  NOCKBLOCKS_API_KEY_STORAGE_KEY,
  fetchNockblocksNotes,
} from './nockblocks';

import './Composer.css';

import {
  shortAddr,
  describePrimitive,
  describeReviewOutputBadge,
  shortHash,
  walletSlotLabel,
  entryDisplayLabel,
  parsePkhListText,
  formatAmountNoUnit,
  formatAmountWithUnit,
  parseAmountTextToNicks,
  formatSummaryJson,
  normalizePreviewDetails,
  previewNumber,
  previewString,
  parseComposeSummary,
  summaryOutputTotal,
  isHighFeeSummary,
  downloadBytes,
  sumAssets,
  NICKS_PER_NOCK,
} from './model';
import type {
  AddressNodeData,
  NoteNodeData,
  TxNodeData,
  ComposedDraft,
  DraftReview,
  ImportedTxPreview,
  PreviewNodeData,
  UnitMode,
  ComposerSidebarPanel,
  AddressFlowNode,
  NoteFlowNode,
  PreviewFlowNode,
  ComposerNode,
  ComposerEdge,
} from './model';

import { AddressNode, NoteNode, TxNode, PreviewNode } from './nodes';

export function Composer({
  wasmReady,
  walletAddresses = [],
  deviceAddressBook = [],
  onSignDraft,
  canSignDraft = false,
  signingDraft = false,
  signDraftDisabledReason = 'connect and unlock device to sign',
}: {
  wasmReady: boolean;
  walletAddresses?: WalletAddress[];
  deviceAddressBook?: DeviceAddressBookEntry[];
  onSignDraft?: (draft: ComposedDraft) => void | Promise<void>;
  canSignDraft?: boolean;
  signingDraft?: boolean;
  signDraftDisabledReason?: string;
}) {
  const [addressBook, setAddressBook] = useState<AddressBookEntry[]>(() => loadAddressBook());

  const unitStorageKey = 'nockster.composer.unit.v1';
  const nockblocksApiKeyStorageKey = NOCKBLOCKS_API_KEY_STORAGE_KEY;
  const [unitModePinned, setUnitModePinned] = useState<boolean>(true);
  const [unitMode, setUnitMode] = useState<UnitMode>(() => {
    const stored = localStorage.getItem(unitStorageKey);
    if (stored === 'n' || stored === 'ℕ') return stored;
    return 'n';
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
  const [sidebarPanel, setSidebarPanel] = useState<ComposerSidebarPanel>('send');
  const [mergeA, setMergeA] = useState<Uint8Array | null>(null);
  const [mergeB, setMergeB] = useState<Uint8Array | null>(null);
  const [mergeStatus, setMergeStatus] = useState('');
  const emptyHtlc = {
    sourceAddr: '',
    nameFirst: '',
    nameLast: '',
    originPage: '',
    assets: '',
    claimRecipient: '',
    commitment: '',
    refundAddr: '',
    refundHeight: '',
    branch: 'claim' as 'claim' | 'refund',
    outRecipient: '',
    outAmount: '',
  };
  const [htlc, setHtlc] = useState(emptyHtlc);
  const [htlcStatus, setHtlcStatus] = useState('');
  const [htlcDraft, setHtlcDraft] = useState<ComposedDraft | null>(null);
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
  const [apiKey, setApiKey] = useState(() => localStorage.getItem(nockblocksApiKeyStorageKey)?.trim() || '');
  const [apiStatus, setApiStatus] = useState('');
  const [syncingNotes, setSyncingNotes] = useState(false);
  const [walletImportStatus, setWalletImportStatus] = useState('');

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
  const [quickRecipient, setQuickRecipient] = useState('');
  const [quickAmount, setQuickAmount] = useState('');
  const [quickDraft, setQuickDraft] = useState<ComposedDraft | null>(null);
  const [quickStatus, setQuickStatus] = useState('');
  const [importedTxPreview, setImportedTxPreview] = useState<ImportedTxPreview | null>(null);
  const [importedTxStatus, setImportedTxStatus] = useState('');
  const walletAutoSyncKeyRef = useRef('');
  const walletByAddress = useMemo(() => {
    const byAddress = new Map<string, WalletAddress>();
    for (const wallet of walletAddresses) {
      const address = wallet.address.trim();
      if (address) byAddress.set(address, wallet);
    }
    return byAddress;
  }, [walletAddresses]);
  const walletAddressKey = useMemo(
    () =>
      walletAddresses
        .map((wallet) => wallet.address.trim())
        .filter(Boolean)
        .sort()
        .join('|'),
    [walletAddresses]
  );

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
      tx: ((props: any) => <TxNode {...props} unitMode={unitMode} />) as any,
      preview: ((props: any) => <PreviewNode {...props} unitMode={unitMode} />) as any,
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
    setNodes((current) => {
      let changed = false;
      const next = current.flatMap((n): ComposerNode[] => {
        if (n.type === 'address') {
          const data = n.data as AddressNodeData;
          const entry = addressBook.find((e) => e.id === data.entryId);
          if (!entry) {
            changed = true;
            return [];
          }

          const kind: AddressKind = entry.kind ?? 'pkh';
          const notes = kind === 'pkh' ? entry.notes ?? [] : [];
          const multisig = kind === 'multisig' ? entry.multisig : undefined;
          const total = sumAssets(notes);
          if (
            data.alias === entry.alias &&
            data.kind === kind &&
            data.address === entry.address &&
            data.multisig === multisig &&
            data.noteCount === notes.length &&
            data.total === total
          ) {
            return [n];
          }

          changed = true;
          return [
            {
              ...n,
              data: {
                ...data,
                alias: entry.alias,
                kind,
                address: entry.address,
                multisig,
                noteCount: notes.length,
                total,
              },
            } as AddressFlowNode,
          ];
        }

        if (n.type === 'note') {
          const data = n.data as NoteNodeData;
          const note = addressBook
            .find((e) => e.id === data.entryId)
            ?.notes?.find((candidate) => candidate.id === data.noteId);
          if (!note) {
            changed = true;
            return [];
          }

          if (
            data.assets === note.assets &&
            data.originPage === note.originPage &&
            data.nameFirst === note.nameFirst &&
            data.nameLast === note.nameLast
          ) {
            return [n];
          }

          changed = true;
          return [
            {
              ...n,
              data: {
                ...data,
                assets: note.assets,
                originPage: note.originPage,
                nameFirst: note.nameFirst,
                nameLast: note.nameLast,
              },
            } as NoteFlowNode,
          ];
        }

        return [n];
      });

      return changed ? next : current;
    });
  }, [addressBook, setNodes]);

  useEffect(() => {
    const nodeIds = new Set(nodes.map((n) => n.id));
    setEdges((current) => {
      const next = current.filter((e) => nodeIds.has(e.source) && nodeIds.has(e.target));
      return next.length === current.length ? current : next;
    });
  }, [nodes, setEdges]);

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

  const clearImportedPreviewGraph = useCallback(() => {
    setEdges((current) => current.filter((edge) => !edge.id.startsWith('preview-')));
    setNodes((current) => current.filter((node) => node.type !== 'preview'));
  }, [setEdges, setNodes]);

  const showImportedPreviewGraph = useCallback(
    (preview: ImportedTxPreview) => {
      const base = `preview-${newId()}`;
      const txNodeId = `${base}-tx`;
      const spends = Array.isArray(preview.details.spends) ? preview.details.spends : [];
      const outputs = spends.flatMap((spend, spendIndex) =>
        (Array.isArray(spend.seeds) ? spend.seeds : []).map((seed, seedIndex) => ({
          spendIndex,
          seedIndex,
          seed,
        }))
      );
      const visibleSpends = spends.slice(0, 8);
      const visibleOutputs = outputs.slice(0, 12);
      const hiddenCount = Math.max(0, spends.length - visibleSpends.length)
        + Math.max(0, outputs.length - visibleOutputs.length);

      const previewNodes: ComposerNode[] = [
        {
          id: txNodeId,
          type: 'preview',
          position: { x: 520, y: 180 },
          data: {
            label: 'Imported tx',
            title: shortHash(preview.info.tx_id, 6),
            meta: [
              `${preview.info.shape} · v${preview.info.version}`,
              `${preview.info.input_count} spend${preview.info.input_count === 1 ? '' : 's'}`,
            ],
            mono: preview.info.tx_id,
            copyValue: preview.info.tx_id,
            copyLabel: 'tx id',
          },
        } as PreviewFlowNode,
      ];
      const previewEdges: ComposerEdge[] = [];

      if (visibleSpends.length === 0) {
        const id = `${base}-spend-empty`;
        previewNodes.push({
          id,
          type: 'preview',
          position: { x: 60, y: 180 },
          data: {
            label: 'Spends',
            title: 'No parsed spend detail',
            meta: ['The jam parsed, but the preview did not expose spend rows.'],
          },
        } as PreviewFlowNode);
        previewEdges.push({
          id: `${base}-edge-empty-spend`,
          source: id,
          sourceHandle: 'out',
          target: txNodeId,
          targetHandle: 'in',
          type: 'smoothstep',
        });
      }

      visibleSpends.forEach((spend, index) => {
        const id = `${base}-spend-${index}`;
        const fee = previewNumber(spend.fee);
        const first = previewString(spend.name_first);
        const last = previewString(spend.name_last);
        const seeds = Array.isArray(spend.seeds) ? spend.seeds.length : 0;
        const meta = [`${seeds} output${seeds === 1 ? '' : 's'}`];
        previewNodes.push({
          id,
          type: 'preview',
          position: { x: 60, y: 40 + index * 190 },
          data: {
            label: `Spend ${index + 1}`,
            title: first || last ? `${shortHash(first, 5)} ${shortHash(last, 5)}` : 'Input note',
            meta,
            mono: first || last ? `${first} ${last}`.trim() : undefined,
            feeNicks: fee === null ? undefined : fee,
            copyValue: first || last ? `${first} ${last}`.trim() : undefined,
            copyLabel: 'note id',
          },
        } as PreviewFlowNode);
        previewEdges.push({
          id: `${base}-edge-spend-${index}`,
          source: id,
          sourceHandle: 'out',
          target: txNodeId,
          targetHandle: 'in',
          type: 'smoothstep',
        });
      });

      if (visibleOutputs.length === 0) {
        const id = `${base}-output-empty`;
        previewNodes.push({
          id,
          type: 'preview',
          position: { x: 980, y: 180 },
          data: {
            label: 'Outputs',
            title: 'No parsed output detail',
          },
        } as PreviewFlowNode);
        previewEdges.push({
          id: `${base}-edge-empty-output`,
          source: txNodeId,
          sourceHandle: 'out',
          target: id,
          targetHandle: 'in',
          type: 'smoothstep',
        });
      }

      visibleOutputs.forEach(({ seed, spendIndex, seedIndex }, index) => {
        const id = `${base}-output-${index}`;
        const gift = previewNumber(seed.gift);
        const recipient = previewString(seed.recipient_pkh);
        const lockRoot = previewString(seed.lock_root);
        const parent = previewString(seed.parent_hash);
        const meta = [`spend ${spendIndex + 1}.${seedIndex + 1}`];
        previewNodes.push({
          id,
          type: 'preview',
          position: { x: 980, y: 40 + index * 165 },
          data: {
            label: `Output ${index + 1}`,
            title: recipient ? shortHash(recipient, 6) : lockRoot ? shortHash(lockRoot, 6) : 'Recipient',
            meta,
            mono: recipient || lockRoot || parent || undefined,
            giftNicks: gift === null ? undefined : gift,
            copyValue: recipient || lockRoot || parent || undefined,
            copyLabel: recipient ? 'address' : 'value',
          },
        } as PreviewFlowNode);
        previewEdges.push({
          id: `${base}-edge-output-${index}`,
          source: txNodeId,
          sourceHandle: 'out',
          target: id,
          targetHandle: 'in',
          type: 'smoothstep',
        });
      });

      if (hiddenCount > 0) {
        previewNodes.push({
          id: `${base}-hidden`,
          type: 'preview',
          position: { x: 520, y: 380 },
          data: {
            label: 'Preview',
            title: `+${hiddenCount} hidden row${hiddenCount === 1 ? '' : 's'}`,
            meta: ['Large imported transactions are truncated on the canvas.'],
          },
        } as PreviewFlowNode);
      }

      setNodes((current) => {
        const connectedNodeIds = new Set<string>();
        for (const edge of edgesRef.current) {
          connectedNodeIds.add(edge.source);
          connectedNodeIds.add(edge.target);
        }
        const retained = current.filter((node) => {
          if (node.type === 'preview') return false;
          if (node.type !== 'tx') return true;
          const data = node.data as Partial<TxNodeData>;
          return connectedNodeIds.has(node.id) || !!data.result;
        });
        return [...retained, ...previewNodes];
      });
      setEdges((current) => current.filter((edge) => !edge.id.startsWith('preview-')).concat(previewEdges));
      queueMicrotask(() => rfInstance.current?.fitView?.({ padding: 0.25, maxZoom: 0.9 }));
    },
    [setEdges, setNodes]
  );

  const ensureTxNodeData = useCallback(
    (nodeId: string) => {
      const existing = nodes.find((n) => n.id === nodeId);
      if (!existing) return;
      if (existing.type !== 'tx') return;

      const data = existing.data as Partial<TxNodeData>;
      if (typeof data.onCompose === 'function') {
        if (
          data.onSignDraft !== onSignDraft ||
          data.canSignDraft !== canSignDraft ||
          data.signingDraft !== signingDraft ||
          data.signDraftDisabledReason !== signDraftDisabledReason
        ) {
          updateNodeData(nodeId, {
            onSignDraft,
            canSignDraft,
            signingDraft,
            signDraftDisabledReason,
          });
        }
        return;
      }

      updateNodeData(nodeId, {
        composing: false,
        onSignDraft,
        canSignDraft,
        signingDraft,
        signDraftDisabledReason,
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
          const outputs: {
            recipient: Recipient;
            amount: number;
            alias?: string;
            timelock?: { abs_min: number };
            hashlock?: string[];
            burn?: boolean;
            or_branches?: OutputLockBranch[];
          }[] = [];
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
            if (rData.kind !== 'multisig' && !is_valid_pkh(address)) {
              outputErrors.push(`${rData.alias ?? address}: recipient is not a valid address`);
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

            // Optional output lock condition (timelock / hashlock / burn / HTLC).
            const lock = rData.lock ?? { kind: 'plain' as const };
            let timelock: { abs_min: number } | undefined;
            let hashlock: string[] | undefined;
            let burn = false;
            let orBranches: OutputLockBranch[] | undefined;
            if (lock.kind === 'burn') {
              burn = true;
            } else if (lock.kind === 'timelock') {
              const h = Number((lock.absMin ?? '').trim());
              if (!Number.isInteger(h) || h <= 0) {
                outputErrors.push(`${rData.alias ?? address}: timelock needs a block height`);
                continue;
              }
              timelock = { abs_min: h };
            } else if (lock.kind === 'hashlock') {
              const commitments = parsePkhListText(lock.commitments ?? '');
              if (commitments.length === 0) {
                outputErrors.push(`${rData.alias ?? address}: hashlock needs commitment hash(es)`);
                continue;
              }
              if (commitments.some((c) => !is_valid_pkh(c))) {
                outputErrors.push(`${rData.alias ?? address}: hashlock commitments are not valid hashes`);
                continue;
              }
              hashlock = commitments;
            } else if (lock.kind === 'htlc') {
              const commitments = parsePkhListText(lock.commitments ?? '');
              const refundAddr = (lock.refundAddress ?? '').trim();
              const refundH = Number((lock.refundHeight ?? '').trim());
              if (commitments.length === 0 || !refundAddr || !Number.isInteger(refundH) || refundH <= 0) {
                outputErrors.push(
                  `${rData.alias ?? address}: HTLC needs claim commitment, refund address, and refund height`,
                );
                continue;
              }
              if (!is_valid_pkh(refundAddr) || commitments.some((c) => !is_valid_pkh(c))) {
                outputErrors.push(`${rData.alias ?? address}: HTLC refund address / commitments are not valid hashes`);
                continue;
              }
              // Branch 0 (claim): recipient reveals the preimage.
              // Branch 1 (refund): refund address can reclaim after the height.
              orBranches = [
                { recipient: address, hashlock: commitments },
                { recipient: refundAddr, timelock: { abs_min: refundH } },
              ];
            }

            if (orBranches) {
              outputs.push({
                recipient: address,
                amount: parsedAmount.nicks,
                alias: rData.alias,
                or_branches: orBranches,
              });
            } else if (rData.kind === 'multisig') {
              if (!rData.multisig || !Array.isArray(rData.multisig.pkhs) || rData.multisig.pkhs.length === 0) {
                outputErrors.push(`${rData.alias ?? address}: multisig is missing signer pubkeys`);
                continue;
              }
              if (rData.multisig.pkhs.some((p) => !is_valid_pkh(p))) {
                outputErrors.push(`${rData.alias ?? address}: multisig has an invalid signer pubkey`);
                continue;
              }
              outputs.push({
                recipient: { m: rData.multisig.m, pkhs: rData.multisig.pkhs },
                amount: parsedAmount.nicks,
                alias: rData.alias,
                timelock,
                hashlock,
                burn,
              });
            } else {
              outputs.push({
                recipient: address,
                amount: parsedAmount.nicks,
                alias: rData.alias,
                timelock,
                hashlock,
                burn,
              });
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
              // Spending from a multisig source: inputs are reconstructed as
              // multisig and the .psnt round-trips through each co-signer.
              source_multisig:
                entry.kind === 'multisig' && entry.multisig
                  ? { m: entry.multisig.m, pkhs: entry.multisig.pkhs }
                  : undefined,
              notes: inputNotes.map((n) => ({
                name_first: n.nameFirst,
                name_last: n.nameLast,
                origin_page: n.originPage,
                assets: n.assets,
                version: n.version ?? 1,
              })),
              outputs,
            });

            let tree: TxTreeNode | null = null;
            try {
              tree = inspect_tx(result.wallet_jam) as TxTreeNode;
            } catch {
              tree = null;
            }
            updateNodeData(nodeId, {
              composing: false,
              lastError: undefined,
              result: {
                filename: `draft-${Date.now()}.psnt`,
                psnt: result.wallet_jam,
                summaryJson: result.summary_json,
                tree,
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
    [canSignDraft, nodes, onSignDraft, signDraftDisabledReason, signingDraft, updateNodeData, wasmReady]
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

      const raw = event.dataTransfer.getData('application/nockster-node');
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
              lock: { kind: 'plain' },
              onChangeLock: (next) => updateNodeData(id, { lock: next }),
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
  const pkhEntries = useMemo(
    () => addressBook.filter((entry) => (entry.kind ?? 'pkh') === 'pkh'),
    [addressBook]
  );
  // Saved address-book recipients, minus any that are already shown as live wallet slots.
  const savedRecipients = useMemo(
    () => addressBook.filter((entry) => !walletAddresses.some((w) => w.address.trim() === entry.address)),
    [addressBook, walletAddresses]
  );
  const quickSourceId =
    selectedEntry && (selectedEntry.kind ?? 'pkh') === 'pkh' ? selectedEntryId : pkhEntries[0]?.id ?? '';
  const quickSourceEntry = quickSourceId ? entryById(quickSourceId) : null;

  const composeQuickPayment = useCallback(
    async (signAfterCompose: boolean) => {
      if (!wasmReady) {
        setQuickStatus('WASM not ready yet');
        return;
      }
      if (!quickSourceEntry || (quickSourceEntry.kind ?? 'pkh') !== 'pkh') {
        setQuickStatus('select a source pkh');
        return;
      }
      const notes = quickSourceEntry.notes ?? [];
      if (notes.length === 0) {
        setQuickStatus('source has no notes');
        return;
      }
      const recipient = quickRecipient.trim();
      if (!recipient) {
        setQuickStatus('recipient required');
        return;
      }
      const parsedAmount = parseAmountTextToNicks(quickAmount, unitModeRef.current);
      if ('error' in parsedAmount) {
        setQuickStatus(parsedAmount.error);
        return;
      }

      try {
        setQuickStatus('composing...');
        const result = compose_tx_v1_unsigned({
          source_pkh: quickSourceEntry.address,
          source_multisig:
            quickSourceEntry.kind === 'multisig' && quickSourceEntry.multisig
              ? { m: quickSourceEntry.multisig.m, pkhs: quickSourceEntry.multisig.pkhs }
              : undefined,
          notes: notes.map((note) => ({
            name_first: note.nameFirst,
            name_last: note.nameLast,
            origin_page: note.originPage,
            assets: note.assets,
            version: note.version ?? 1,
          })),
          outputs: [{ recipient, amount: parsedAmount.nicks, alias: 'recipient' }],
        });
        let quickTree: TxTreeNode | null = null;
        try {
          quickTree = inspect_tx(result.wallet_jam) as TxTreeNode;
        } catch {
          quickTree = null;
        }
        const draft: ComposedDraft = {
          filename: `draft-${Date.now()}.psnt`,
          psnt: result.wallet_jam,
          summaryJson: result.summary_json,
          tree: quickTree,
        };
        setQuickDraft(draft);

        const summary = parseComposeSummary(draft.summaryJson);
        const fee = Number(summary?.total_fees) || 0;
        const external = summaryOutputTotal(summary);
        const warning = isHighFeeSummary(summary) ? ' · fee exceeds calculated minimum' : '';
        setQuickStatus(
          `ready · send ${formatAmountWithUnit(external, unitModeRef.current)} · fee ${formatAmountWithUnit(
            fee,
            unitModeRef.current
          )}${warning}`
        );

        if (signAfterCompose) {
          if (!onSignDraft || !canSignDraft) {
            setQuickStatus(signDraftDisabledReason);
            return;
          }
          await onSignDraft(draft);
        }
      } catch (err: any) {
        setQuickDraft(null);
        setQuickStatus(err?.message ?? String(err));
      }
    },
    [
      canSignDraft,
      onSignDraft,
      quickAmount,
      quickRecipient,
      quickSourceEntry,
      signDraftDisabledReason,
      wasmReady,
    ]
  );

  const loadTransactionPreview = useCallback(async (file: File | null | undefined) => {
    if (!file) return;
    setImportedTxStatus(`loading ${file.name}...`);
    setImportedTxPreview(null);
    clearImportedPreviewGraph();

    let parsed: ParsedTransaction | null = null;
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      parsed = new ParsedTransaction(bytes);
      const info = parsed.info();
      const previewInfo = {
        tx_id: info.tx_id,
        shape: info.shape,
        version: info.version,
        input_count: info.input_count,
      };
      info.free();
      const details = normalizePreviewDetails(parsed.get_details());
      // Device-parity review: outputs, locks, bridge, multisig — keyed by the
      // device's own address (best-effort: refund/we-signed detection needs it).
      let review: DraftReview | null = null;
      try {
        review = review_draft(bytes, walletAddresses[0]?.address ?? '') as DraftReview;
      } catch {
        review = null;
      }
      let tree: TxTreeNode | null = null;
      try {
        tree = inspect_tx(bytes) as TxTreeNode;
      } catch {
        tree = null;
      }
      const preview: ImportedTxPreview = {
        name: file.name,
        info: previewInfo,
        details,
        review,
        tree,
        bytes,
      };
      setImportedTxPreview(preview);
      showImportedPreviewGraph(preview);
      setImportedTxStatus(`loaded ${previewInfo.tx_id}`);
    } catch (err: any) {
      setImportedTxPreview(null);
      clearImportedPreviewGraph();
      setImportedTxStatus(`transaction preview failed: ${err?.message ?? String(err)}`);
    } finally {
      parsed?.free();
    }
  }, [clearImportedPreviewGraph, showImportedPreviewGraph, walletAddresses]);

  const mergeSignedCopies = useCallback(() => {
    if (!mergeA || !mergeB) return;
    try {
      const merged = merge_signed_tx(mergeA, mergeB) as Uint8Array;
      downloadBytes(`merged-${Date.now()}.psnt`, merged);
      setMergeStatus('merged signatures, downloaded combined .psnt');
    } catch (err: any) {
      setMergeStatus(`merge failed: ${err?.message ?? String(err)}`);
    }
  }, [mergeA, mergeB]);

  const readFileBytes = async (file: File | null | undefined): Promise<Uint8Array | null> =>
    file ? new Uint8Array(await file.arrayBuffer()) : null;

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

  const saveNockblocksSettings = useCallback(() => {
    const trimmedKey = apiKey.trim();

    if (trimmedKey) {
      localStorage.setItem(nockblocksApiKeyStorageKey, trimmedKey);
      setApiKey(trimmedKey);
      setApiStatus('saved Nockblocks API key in this browser');
    } else {
      localStorage.removeItem(nockblocksApiKeyStorageKey);
      setApiStatus('cleared saved Nockblocks API key');
    }
  }, [apiKey, nockblocksApiKeyStorageKey]);

  const clearNockblocksSettings = useCallback(() => {
    localStorage.removeItem(nockblocksApiKeyStorageKey);
    setApiKey('');
    setApiStatus('cleared Nockblocks API key from this browser');
  }, [nockblocksApiKeyStorageKey]);

  const upsertWalletEntries = useCallback(
    (entries: AddressBookEntry[]) => {
      const walletPkhs = walletAddresses
        .map((wallet) => ({ ...wallet, address: wallet.address.trim() }))
        .filter((wallet) => wallet.address.length > 0);

      const next = [...entries];
      let added = 0;
      let updated = 0;
      let firstAddedId = '';

      for (const wallet of walletPkhs) {
        const alias = walletSlotLabel(wallet);
        const existingIndex = next.findIndex((entry) => entry.address === wallet.address);
        if (existingIndex >= 0) {
          const existing = next[existingIndex];
          if (existing.alias !== alias) {
            next[existingIndex] = { ...existing, alias };
            updated += 1;
          }
          continue;
        }

        const id = newId();
        if (!firstAddedId) firstAddedId = id;
        next.push({
          id,
          alias,
          kind: 'pkh',
          address: wallet.address,
          notes: [],
        });
        added += 1;
      }

      return { added, updated, firstAddedId, next, walletPkhs };
    },
    [walletAddresses]
  );

  const importWalletAddresses = useCallback(
    (quiet = false) => {
      const { added, updated, firstAddedId, next, walletPkhs } = upsertWalletEntries(addressBook);

      if (walletPkhs.length === 0) {
        if (!quiet) setWalletImportStatus('connect and unlock the device to import wallet pkhs');
        return;
      }

      if (added > 0 || updated > 0) {
        setAddressBook(next);
        saveAddressBook(next);
        if (!selectedEntryId && firstAddedId) {
          setSelectedEntryId(firstAddedId);
        }
      }

      if (!quiet) {
        setWalletImportStatus(
          added > 0 || updated > 0
            ? `imported ${added} wallet pkh${added === 1 ? '' : 's'}`
              + (updated > 0 ? ` · updated ${updated} nickname${updated === 1 ? '' : 's'}` : '')
            : 'wallet pkhs already imported'
        );
      }
    },
    [addressBook, selectedEntryId, upsertWalletEntries]
  );

  const importDeviceAddressBook = useCallback(
    (quiet = false) => {
      const candidates = deviceAddressBook
        .map((entry) => ({
          label: entry.label.trim(),
          pkh: entry.pkh.trim(),
        }))
        .filter((entry) => entry.label && entry.pkh);

      if (candidates.length === 0) {
        if (!quiet) setWalletImportStatus('device address book is empty');
        return;
      }

      const next = [...addressBook];
      let added = 0;
      let updated = 0;
      let firstAddedId = '';

      for (const entry of candidates) {
        const existingIndex = next.findIndex((candidate) => candidate.address === entry.pkh);
        if (existingIndex >= 0) {
          const existing = next[existingIndex];
          if (existing.alias !== entry.label) {
            next[existingIndex] = { ...existing, alias: entry.label };
            updated += 1;
          }
          continue;
        }

        const id = newId();
        if (!firstAddedId) firstAddedId = id;
        next.push({
          id,
          alias: entry.label,
          kind: 'pkh',
          address: entry.pkh,
          notes: [],
        });
        added += 1;
      }

      if (added > 0 || updated > 0) {
        setAddressBook(next);
        saveAddressBook(next);
        if (!selectedEntryId && firstAddedId) {
          setSelectedEntryId(firstAddedId);
        }
      }

      if (!quiet) {
        setWalletImportStatus(
          added > 0 || updated > 0
            ? `imported ${added} and updated ${updated} from device book`
            : 'device book already imported'
        );
      }
    },
    [addressBook, deviceAddressBook, selectedEntryId]
  );

  useEffect(() => {
    if (walletAddresses.length === 0) return;
    importWalletAddresses(true);
  }, [importWalletAddresses, walletAddresses.length]);

  useEffect(() => {
    if (deviceAddressBook.length === 0) return;
    importDeviceAddressBook(true);
  }, [deviceAddressBook.length, importDeviceAddressBook]);

  const syncWalletNotes = useCallback(
    async (quiet = false) => {
      const key = apiKey.trim();
      const {
        added,
        updated,
        firstAddedId,
        next: withWalletEntries,
        walletPkhs,
      } = upsertWalletEntries(addressBook);

      if (walletPkhs.length === 0) {
        if (!quiet) setWalletImportStatus('connect and unlock the device to sync wallet notes');
        return;
      }
      if (!key) {
        if (added > 0 || updated > 0) {
          setAddressBook(withWalletEntries);
          saveAddressBook(withWalletEntries);
          if (!selectedEntryId && firstAddedId) {
            setSelectedEntryId(firstAddedId);
          }
        }
        if (!quiet) setApiStatus('save a Nockblocks API key before syncing wallet notes');
        return;
      }

      setSyncingNotes(true);
      if (!quiet) setApiStatus(`syncing notes for ${walletPkhs.length} wallet pkh${walletPkhs.length === 1 ? '' : 's'}...`);

      let next = withWalletEntries;
      let synced = 0;
      let noteCount = 0;
      let totalNicks = 0;
      let skipped = 0;
      let multisig = 0;
      const failures: string[] = [];

      try {
        for (const wallet of walletPkhs) {
          try {
            const imported = await fetchNockblocksNotes({
              address: wallet.address,
              apiKey: key,
            });
            next = next.map((entry) =>
              entry.address === wallet.address && (entry.kind ?? 'pkh') === 'pkh'
                ? { ...entry, notes: imported.notes }
                : entry
            );
            synced += 1;
            noteCount += imported.notes.length;
            totalNicks += sumAssets(imported.notes);
            skipped += imported.skipped;
            multisig += imported.multisig;
          } catch (err: any) {
            failures.push(`${wallet.alias}: ${err?.message ?? String(err)}`);
          }
        }

        setAddressBook(next);
        saveAddressBook(next);
        if (!selectedEntryId && firstAddedId) {
          setSelectedEntryId(firstAddedId);
        }

        const parts = [
          `synced ${noteCount} notes for ${synced}/${walletPkhs.length} wallet pkhs`,
          `${formatAmountWithUnit(totalNicks, unitModeRef.current)} imported`,
        ];
        if (added > 0) parts.push(`${added} pkh${added === 1 ? '' : 's'} added`);
        if (updated > 0) parts.push(`${updated} nickname${updated === 1 ? '' : 's'} updated`);
        if (skipped > 0) parts.push(`${skipped} skipped`);
        if (multisig > 0) parts.push(`${multisig} multisig notes not imported`);
        if (failures.length > 0) parts.push(`${failures.length} failed`);
        if (!quiet || failures.length > 0) setApiStatus(parts.join(' · '));
        if (!quiet && failures.length > 0) setWalletImportStatus(failures.join(' · '));
      } finally {
        setSyncingNotes(false);
      }
    },
    [addressBook, apiKey, selectedEntryId, upsertWalletEntries]
  );

  useEffect(() => {
    const key = apiKey.trim();
    if (!walletAddressKey || !key) return;
    const syncKey = `${walletAddressKey}:${key}`;
    if (walletAutoSyncKeyRef.current === syncKey) return;
    walletAutoSyncKeyRef.current = syncKey;
    syncWalletNotes(true);
  }, [apiKey, syncWalletNotes, walletAddressKey]);

  const syncSelectedEntryNotes = useCallback(async () => {
    if (!selectedEntry) {
      setApiStatus('select a pkh address first');
      return;
    }
    if ((selectedEntry.kind ?? 'pkh') !== 'pkh') {
      setApiStatus('note sync supports pkh address entries only');
      return;
    }
    if (!apiKey.trim()) {
      setApiStatus('Nockblocks API key required');
      return;
    }

    setSyncingNotes(true);
    setApiStatus(`fetching notes for ${selectedEntry.alias}...`);
    try {
      const imported = await fetchNockblocksNotes({
        address: selectedEntry.address,
        apiKey,
      });
      const next = addressBook.map((entry) =>
        entry.id === selectedEntry.id ? { ...entry, notes: imported.notes } : entry
      );
      setAddressBook(next);
      saveAddressBook(next);

      const importedTotal = sumAssets(imported.notes);
      const parts = [
        `synced ${imported.notes.length} V1 notes`,
        `${formatAmountWithUnit(importedTotal, unitModeRef.current)} imported`,
      ];
      if (imported.nicks !== importedTotal) {
        parts.push(`${formatAmountWithUnit(imported.nicks, unitModeRef.current)} reported by API`);
      }
      if (imported.skipped > 0) parts.push(`${imported.skipped} skipped`);
      if (imported.multisig > 0) parts.push(`${imported.multisig} multisig notes not imported`);
      setApiStatus(parts.join(' · '));
    } catch (err: any) {
      setApiStatus(`sync failed: ${err?.message ?? String(err)}`);
    } finally {
      setSyncingNotes(false);
    }
  }, [addressBook, apiKey, selectedEntry]);

  const dragData = (payload: any) => JSON.stringify(payload);

  return (
    <>
      <div className={`composer-layout ${sidebarCollapsed ? 'sidebar-collapsed' : ''}`}>
        {!sidebarCollapsed && (
          <div className="composer-sidebar">
            <div className="composer-sidebar-header">
              <div className="composer-sidebar-title">Composer</div>
              <div className="composer-row composer-sidebar-tools">
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

            <div className="composer-sidebar-tabs" role="tablist" aria-label="Composer panels">
              {([
                ['send', 'Send / Sign'],
                ['wallet', 'Wallet'],
              ] as const).map(([panel, label]) => (
                <button
                  key={panel}
                  type="button"
                  className={`composer-sidebar-tab ${sidebarPanel === panel ? 'active' : ''}`}
                  onClick={() => setSidebarPanel(panel)}
                >
                  {label}
                </button>
              ))}
            </div>

            {/* Canvas controls live at the top of every tab. */}
            <div className="composer-canvas-controls">
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
                  reset canvas
                </button>
              </div>
            </div>

            {sidebarPanel === 'send' && (
            <details className="composer-details" open>
              <summary className="composer-summary">
                <span>Simple send</span>
              </summary>
              <div className="composer-details-body">
                <label className="composer-field">
                  <span>Source</span>
                  <select
                    className="node-input"
                    value={quickSourceId}
                    onChange={(event) => setSelectedEntryId(event.target.value)}
                    disabled={pkhEntries.length === 0}
                  >
                    {pkhEntries.length === 0 ? (
                      <option value="">no wallet pkh</option>
                    ) : (
                      pkhEntries.map((entry) => (
                        <option key={entry.id} value={entry.id}>
                          {entryDisplayLabel(entry)} · {(entry.notes ?? []).length} notes
                        </option>
                      ))
                    )}
                  </select>
                </label>
                <label className="composer-field">
                  <span>Recipient</span>
                  {(walletAddresses.length > 0 || savedRecipients.length > 0 || deviceAddressBook.length > 0) && (
                    <select
                      className="node-input"
                      value=""
                      onChange={(event) => {
                        const next = event.target.value;
                        if (next) setQuickRecipient(next);
                      }}
                    >
                      <option value="">pick a slot or saved address…</option>
                      {walletAddresses.length > 0 && (
                        <optgroup label="wallet slots">
                          {walletAddresses.map((wallet) => (
                            <option key={`slot-${wallet.slot}-${wallet.address}`} value={wallet.address}>
                              {walletSlotLabel(wallet)}
                            </option>
                          ))}
                        </optgroup>
                      )}
                      {savedRecipients.length > 0 && (
                        <optgroup label="saved addresses">
                          {savedRecipients.map((entry) => (
                            <option key={`saved-${entry.id}`} value={entry.address}>
                              {entryDisplayLabel(entry)}
                            </option>
                          ))}
                        </optgroup>
                      )}
                      {deviceAddressBook.length > 0 && (
                        <optgroup label="device address book">
                          {deviceAddressBook.map((entry, index) => (
                            <option key={`abk-${index}-${entry.pkh}`} value={entry.pkh}>
                              {entry.label} · {shortHash(entry.pkh, 4)}
                            </option>
                          ))}
                        </optgroup>
                      )}
                    </select>
                  )}
                  <input
                    className="node-input"
                    value={quickRecipient}
                    onChange={(event) => setQuickRecipient(event.target.value)}
                    placeholder="recipient pkh — pick above or paste"
                    spellCheck={false}
                  />
                </label>
                <label className="composer-field">
                  <span>Amount ({unitMode})</span>
                  <input
                    className="node-input"
                    value={quickAmount}
                    onChange={(event) => setQuickAmount(event.target.value)}
                    inputMode={unitMode === 'ℕ' ? 'decimal' : 'numeric'}
                    placeholder={`amount in ${unitMode}`}
                  />
                </label>
                <div className="composer-row composer-action-row">
                  <button
                    type="button"
                    className="btn btn-small btn-secondary"
                    onClick={() => void composeQuickPayment(false)}
                    disabled={pkhEntries.length === 0}
                  >
                    compose
                  </button>
                  <button
                    type="button"
                    className="btn btn-small btn-primary"
                    onClick={() => void composeQuickPayment(true)}
                    disabled={pkhEntries.length === 0 || signingDraft || !canSignDraft}
                    title={!canSignDraft ? signDraftDisabledReason : undefined}
                  >
                    {signingDraft ? 'signing...' : 'compose + sign'}
                  </button>
                  {quickDraft && (
                    <button
                      type="button"
                      className="btn btn-small btn-secondary"
                      onClick={() => downloadBytes(quickDraft.filename, quickDraft.psnt)}
                    >
                      download
                    </button>
                  )}
                </div>
                {quickStatus && <div className="composer-api-status">{quickStatus}</div>}
              </div>
            </details>
            )}

            {sidebarPanel === 'send' && (
            <details className="composer-details" open>
              <summary className="composer-summary">
                <span>Upload &amp; sign a transaction</span>
              </summary>
              <div className="composer-details-body">
                <label className="composer-field">
                  <span>Jam file (.psnt / .tx)</span>
                  <input
                    className="node-input"
                    type="file"
                    accept=".jam,.draft,.psnt,.wallet,application/octet-stream"
                    onChange={(event) => void loadTransactionPreview(event.target.files?.[0])}
                  />
                </label>
                {importedTxPreview && (
                  <div className="composer-result">
                    <div className="composer-result-grid">
                      <span>file</span>
                      <strong>{importedTxPreview.name}</strong>
                      <span>shape</span>
                      <strong>{importedTxPreview.info.shape}</strong>
                      <span>spends</span>
                      <strong>{importedTxPreview.info.input_count}</strong>
                      <span>tx</span>
                      <strong>{shortHash(importedTxPreview.info.tx_id, 5)}</strong>
                    </div>
                    {importedTxPreview.review && (
                      <div className="composer-review">
                        <div className="composer-output-note">what the device will show</div>
                        {importedTxPreview.review.multisig_inputs.map((ms, i) => (
                          <div className="composer-review-msig" key={`ms-${i}`}>
                            spend {ms.m}-of-{ms.n} · {ms.present}/{ms.m} signed
                            {ms.we_authorized && (ms.we_signed ? ' · you signed' : ' · you can sign')}
                          </div>
                        ))}
                        <div className="composer-output-list">
                          {importedTxPreview.review.outputs.map((output, i) => {
                            const badge = describeReviewOutputBadge(output);
                            const detail = (output.lock ?? []).map(describePrimitive).join(' · ');
                            const addr = output.bridge_evm_addr || output.recipient_b58;
                            return (
                              <div className="composer-output-row" key={`o-${i}`}>
                                <span
                                  className={`composer-lock-badge ${badge !== 'p2pkh' && badge !== 'change' ? 'multisig' : ''}`}
                                  title={detail || undefined}
                                >
                                  {badge}
                                </span>
                                <span className="composer-output-addr" title={addr}>
                                  {shortAddr(addr)}
                                </span>
                                <strong>
                                  {formatAmountWithUnit(Number(output.gift) || 0, unitMode)}
                                </strong>
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    )}
                    {importedTxPreview.tree && (
                      <div className="composer-review">
                        <div className="composer-output-note">transaction structure</div>
                        <TxTree node={importedTxPreview.tree} />
                      </div>
                    )}
                    {importedTxPreview.bytes && (
                      <div className="composer-row composer-action-row">
                        <button
                          type="button"
                          className="btn btn-primary btn-small"
                          title={!canSignDraft ? signDraftDisabledReason : undefined}
                          disabled={!onSignDraft || !canSignDraft || signingDraft}
                          onClick={() =>
                            void onSignDraft?.({
                              filename: importedTxPreview.name,
                              psnt: importedTxPreview.bytes!,
                              summaryJson: '',
                            })
                          }
                        >
                          {signingDraft ? 'signing...' : 'sign on device'}
                        </button>
                      </div>
                    )}
                  </div>
                )}
                {importedTxStatus && <div className="composer-api-status">{importedTxStatus}</div>}
              </div>
            </details>
            )}

            {sidebarPanel === 'send' && (
            <details className="composer-details">
              <summary className="composer-summary">
                <span>Combine multisig signatures</span>
              </summary>
              <div className="composer-details-body">
                <div className="inspector-help">
                  Merge two partially-signed copies of the same multisig transaction (parallel
                  co-signing) into one combined <code>.psnt</code>.
                </div>
                <label className="composer-field">
                  <span>Signed copy A</span>
                  <input
                    className="node-input"
                    type="file"
                    accept=".jam,.draft,.psnt,.wallet,application/octet-stream"
                    onChange={async (e) => {
                      setMergeA(await readFileBytes(e.target.files?.[0]));
                      setMergeStatus('');
                    }}
                  />
                </label>
                <label className="composer-field">
                  <span>Signed copy B</span>
                  <input
                    className="node-input"
                    type="file"
                    accept=".jam,.draft,.psnt,.wallet,application/octet-stream"
                    onChange={async (e) => {
                      setMergeB(await readFileBytes(e.target.files?.[0]));
                      setMergeStatus('');
                    }}
                  />
                </label>
                <button
                  type="button"
                  className="btn btn-small btn-primary"
                  disabled={!mergeA || !mergeB}
                  onClick={mergeSignedCopies}
                >
                  merge &amp; download
                </button>
                {mergeStatus && <div className="composer-api-status">{mergeStatus}</div>}
              </div>
            </details>
            )}

            {sidebarPanel === 'wallet' && (
            <>
            <details className="composer-details" open>
              <summary className="composer-summary">
                <span>Nockblocks</span>
              </summary>
              <div className="composer-details-body nockblocks-panel">
                <label className="composer-field">
                  <span>API key</span>
                  <input
                    className="node-input"
                    type="password"
                    autoComplete="off"
                    spellCheck={false}
                    placeholder="paste Nockblocks API key"
                    value={apiKey}
                    onChange={(e) => {
                      setApiKey(e.target.value);
                    }}
                  />
                </label>
                <div className="composer-row composer-action-row">
                  <button type="button" className="btn btn-small btn-primary" onClick={saveNockblocksSettings}>
                    save key
                  </button>
                  <button type="button" className="btn btn-small btn-secondary" onClick={clearNockblocksSettings}>
                    clear
                  </button>
                  <button
                    type="button"
                    className="btn btn-small btn-secondary"
                    onClick={syncSelectedEntryNotes}
                    disabled={
                      !selectedEntry ||
                      (selectedEntry.kind ?? 'pkh') !== 'pkh' ||
                      !apiKey.trim() ||
                      syncingNotes
                    }
                  >
                    {syncingNotes ? 'syncing...' : 'sync selected'}
                  </button>
                </div>
                {selectedEntry ? (
                  <div className="composer-item-meta">
                    selected: {selectedEntry.alias} · {shortHash(selectedEntry.address, 5)}
                  </div>
                ) : (
                  <div className="composer-item-meta">select a pkh address to sync notes</div>
                )}
                {apiStatus && <div className="composer-api-status">{apiStatus}</div>}
              </div>
            </details>

            <details className="composer-details" open>
              <summary className="composer-summary">
                <span>Address book ({addressBook.length})</span>
              </summary>
              <div className="composer-details-body">
                <div className="composer-row composer-action-row">
                  <button
                    type="button"
                    className="btn btn-small btn-primary"
                    onClick={() => syncWalletNotes(false)}
                    disabled={walletAddresses.length === 0 || syncingNotes}
                  >
                    {syncingNotes ? 'syncing...' : 'sync wallet notes'}
                  </button>
                  <button
                    type="button"
                    className="btn btn-small btn-secondary"
                    onClick={() => importWalletAddresses(false)}
                    disabled={walletAddresses.length === 0}
                  >
                    import pkhs
                  </button>
                  <button
                    type="button"
                    className="btn btn-small btn-secondary"
                    onClick={() => importDeviceAddressBook(false)}
                    disabled={deviceAddressBook.length === 0}
                  >
                    import device book
                  </button>
                  <button
                    type="button"
                    className="btn btn-small btn-primary"
                    onClick={() => setAddressAddOpen(true)}
                  >
                    add address
                  </button>
                </div>
                {walletAddresses.length === 0 && (
                  <div className="inspector-help">Connect and unlock the device to import wallet pkhs.</div>
                )}
                {walletImportStatus && <div className="composer-api-status">{walletImportStatus}</div>}
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
                              'application/nockster-node',
                              dragData({ kind: 'address', entryId: entry.id })
                            )
                          }
                          onClick={() => setSelectedEntryId(entry.id)}
                          role="button"
                          tabIndex={0}
                        >
                          <div className="composer-item-title">
                            <span>{entry.alias}</span>
                            <div className="composer-row composer-item-actions">
                              {walletByAddress.has(entry.address) && <span className="composer-count">wallet</span>}
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
                              'application/nockster-node',
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
                            owner: {selectedEntry.alias} · {shortHash(selectedEntry.address, 5)}
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
            </>
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

                if (node.type === 'preview') {
                  const data = node.data as PreviewNodeData;
                  return (
                    <div className="composer-list">
                      <div>
                        <strong>{data.label}</strong>
                      </div>
                      <div>{data.title}</div>
                      {data.meta?.map((item) => (
                        <div key={item} className="inspector-help">
                          {item}
                        </div>
                      ))}
                      {data.mono && <div className="node-mono">{data.mono}</div>}
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

export function ComposerView({
  wasmReady,
  walletAddresses,
  deviceAddressBook,
  onSignDraft,
  canSignDraft,
  signingDraft,
  signDraftDisabledReason,
}: {
  wasmReady: boolean;
  walletAddresses?: WalletAddress[];
  deviceAddressBook?: DeviceAddressBookEntry[];
  onSignDraft?: (draft: ComposedDraft) => void | Promise<void>;
  canSignDraft?: boolean;
  signingDraft?: boolean;
  signDraftDisabledReason?: string;
}) {
  return (
    <ReactFlowProvider>
      <Composer
        wasmReady={wasmReady}
        walletAddresses={walletAddresses}
        deviceAddressBook={deviceAddressBook}
        onSignDraft={onSignDraft}
        canSignDraft={canSignDraft}
        signingDraft={signingDraft}
        signDraftDisabledReason={signDraftDisabledReason}
      />
    </ReactFlowProvider>
  );
}
