// Composer React Flow node components, extracted from Composer.tsx.
// Verbatim move — pure presentational components driven by node data.
import { Handle, NodeResizer, Position, type NodeProps } from '@xyflow/react';


import {
  describeRecipient,
  downloadBytes,
  formatAmountWithUnit,
  isHighFeeSummary,
  parseAmountTextToNicks,
  parseComposeSummary,
  shortHash,
  stopNodeInputEvent,
  summaryInputTotal,
  summaryOutputTotal,
} from './model';
import type {
  AddressFlowNode,
  NoteFlowNode,
  TxFlowNode,
  PreviewFlowNode,
  UnitMode,
} from './model';

// A magnifying-glass button in a node header that opens the inspector.
function InspectButton({ onInspect }: { onInspect?: () => void }) {
  if (!onInspect) return null;
  return (
    <button
      type="button"
      className="node-inspect-btn nodrag"
      title="inspect"
      aria-label="inspect"
      onPointerDown={stopNodeInputEvent}
      onClick={(e) => {
        e.stopPropagation();
        onInspect();
      }}
    >
      <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8">
        <circle cx="6.8" cy="6.8" r="4.6" />
        <line x1="10.2" y1="10.2" x2="14" y2="14" strokeLinecap="round" />
      </svg>
    </button>
  );
}

type WithExtras = { unitMode: UnitMode; onInspect?: () => void };

export function AddressNode({ data, unitMode, onInspect }: NodeProps<AddressFlowNode> & WithExtras) {
  const io = data.io ?? { isInput: false, isOutput: false };
  const showNotes = io.isInput || (!io.isOutput && data.noteCount > 0);
  const showAmount = io.isOutput || data.amount.trim().length > 0;

  const parsed = data.amount.trim() ? parseAmountTextToNicks(data.amount, unitMode) : null;

  return (
    <div className="node-card">
      <NodeResizer minWidth={200} minHeight={80} />
      <div className="node-header address">
        <span>{data.kind === 'multisig' ? 'Multisig' : 'Address'}</span>
        <InspectButton onInspect={onInspect} />
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
            className="node-input node-input-compact nodrag"
            placeholder={`amount (${unitMode})`}
            value={data.amount}
            onClick={stopNodeInputEvent}
            onDoubleClick={stopNodeInputEvent}
            onKeyDown={stopNodeInputEvent}
            onPointerDown={stopNodeInputEvent}
            onChange={(e) => data.onChangeAmount(e.target.value)}
          />
        )}
        {parsed && 'error' in parsed && <div className="validation-text">{parsed.error}</div>}
        {io.isOutput && data.onChangeLock && (
          <div className="composer-lock-controls">
            <select
              className="node-input node-input-compact nodrag"
              value={data.lock?.kind ?? 'plain'}
              onPointerDown={stopNodeInputEvent}
              onChange={(e) =>
                data.onChangeLock?.({
                  ...(data.lock ?? { kind: 'plain' }),
                  kind: e.target.value as 'plain' | 'timelock' | 'burn',
                })
              }
            >
              <option value="plain">plain</option>
              <option value="timelock">timelock</option>
              <option value="hashlock">hashlock</option>
              <option value="htlc">HTLC (claim-or-refund)</option>
              <option value="burn">burn</option>
            </select>
            {data.lock?.kind === 'timelock' && (
              <input
                className="node-input node-input-compact nodrag"
                placeholder="spendable at height"
                value={data.lock?.absMin ?? ''}
                onClick={stopNodeInputEvent}
                onDoubleClick={stopNodeInputEvent}
                onKeyDown={stopNodeInputEvent}
                onPointerDown={stopNodeInputEvent}
                onChange={(e) =>
                  data.onChangeLock?.({ kind: 'timelock', absMin: e.target.value })
                }
              />
            )}
            {data.lock?.kind === 'hashlock' && (
              <input
                className="node-input node-input-compact nodrag"
                placeholder="commitment hash(es)"
                value={data.lock?.commitments ?? ''}
                onClick={stopNodeInputEvent}
                onDoubleClick={stopNodeInputEvent}
                onKeyDown={stopNodeInputEvent}
                onPointerDown={stopNodeInputEvent}
                onChange={(e) =>
                  data.onChangeLock?.({ kind: 'hashlock', commitments: e.target.value })
                }
              />
            )}
            {data.lock?.kind === 'htlc' && (
              <>
                <div className="inspector-help">
                  claim: recipient + preimage · refund: address after height
                </div>
                <input
                  className="node-input node-input-compact nodrag"
                  placeholder="claim commitment hash"
                  value={data.lock?.commitments ?? ''}
                  onClick={stopNodeInputEvent}
                  onPointerDown={stopNodeInputEvent}
                  onKeyDown={stopNodeInputEvent}
                  onChange={(e) =>
                    data.onChangeLock?.({ ...(data.lock ?? { kind: 'htlc' }), kind: 'htlc', commitments: e.target.value })
                  }
                />
                <input
                  className="node-input node-input-compact nodrag"
                  placeholder="refund address (pkh)"
                  value={data.lock?.refundAddress ?? ''}
                  onClick={stopNodeInputEvent}
                  onPointerDown={stopNodeInputEvent}
                  onKeyDown={stopNodeInputEvent}
                  onChange={(e) =>
                    data.onChangeLock?.({ ...(data.lock ?? { kind: 'htlc' }), kind: 'htlc', refundAddress: e.target.value })
                  }
                />
                <input
                  className="node-input node-input-compact nodrag"
                  placeholder="refund unlock height"
                  value={data.lock?.refundHeight ?? ''}
                  onClick={stopNodeInputEvent}
                  onPointerDown={stopNodeInputEvent}
                  onKeyDown={stopNodeInputEvent}
                  onChange={(e) =>
                    data.onChangeLock?.({ ...(data.lock ?? { kind: 'htlc' }), kind: 'htlc', refundHeight: e.target.value })
                  }
                />
              </>
            )}
            {data.lock?.kind === 'burn' && (
              <div className="inspector-help">⚠ unspendable</div>
            )}
          </div>
        )}
        {io.isOutput && data.onChangeLockRootOnly && (
          <label className="composer-radio" title="Commit only the lock-root; the lock isn't revealed on-chain until the recipient spends">
            <input
              type="checkbox"
              checked={data.lockRootOnly ?? false}
              onPointerDown={stopNodeInputEvent}
              onChange={(e) => data.onChangeLockRootOnly?.(e.target.checked)}
            />
            lock-root only (private)
          </label>
        )}
      </div>
      <Handle type="target" id="in" position={Position.Left} />
      <Handle type="source" id="out" position={Position.Right} />
    </div>
  );
}

export function NoteNode({ data, unitMode, onInspect }: NodeProps<NoteFlowNode> & WithExtras) {
  return (
    <div className="node-card">
      <NodeResizer minWidth={200} minHeight={80} />
      <div className="node-header note">
        <span>Note</span>
        <InspectButton onInspect={onInspect} />
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

export function TxNode({ data, unitMode, onInspect }: NodeProps<TxFlowNode> & WithExtras) {
  const composing = data.composing ?? false;
  const onCompose = data.onCompose;
  const summary = parseComposeSummary(data.result?.summaryJson);
  const fee = Number(summary?.total_fees) || 0;
  const minimumFee = Number(summary?.minimum_fee) || 0;
  const external = summaryOutputTotal(summary);
  const inputTotal = summaryInputTotal(summary);
  const change = Math.max(0, inputTotal - external - fee);
  const highFee = isHighFeeSummary(summary);

  return (
    <div className="node-card">
      <NodeResizer minWidth={200} minHeight={80} />
      <div className="node-header tx">
        <span>Tx</span>
        <InspectButton onInspect={onInspect} />
      </div>
      <div className="node-body">
        <div className="node-actions">
          <button
            className="btn btn-success btn-small nodrag"
            onPointerDown={stopNodeInputEvent}
            onClick={(event) => {
              event.stopPropagation();
              onCompose?.();
            }}
            disabled={!onCompose || composing}
          >
            {composing ? 'composing...' : 'compose'}
          </button>
        </div>
        {data.result && (
          <div className="composer-result">
            {summary && (
              <div className="composer-result-grid">
                <span>send</span>
                <strong>{formatAmountWithUnit(external, unitMode)}</strong>
                <span>fee</span>
                <strong className={highFee ? 'composer-warn-text' : ''}>
                  {formatAmountWithUnit(fee, unitMode)}
                </strong>
                <span>min</span>
                <strong>{formatAmountWithUnit(minimumFee, unitMode)}</strong>
                <span>change</span>
                <strong>{formatAmountWithUnit(change, unitMode)}</strong>
              </div>
            )}
            {summary?.outputs && summary.outputs.length > 0 && (
              <div className="composer-output-list">
                <div className="composer-section-title">Outputs &amp; locks (device review)</div>
                {summary.outputs.map((output, i) => {
                  const { badge, address } = describeRecipient(output.recipient);
                  const isMultisig = badge !== 'p2pkh';
                  return (
                    <div className="composer-output-row" key={i}>
                      <span className={`composer-lock-badge ${isMultisig ? 'multisig' : ''}`}>
                        {badge}
                      </span>
                      <span className="composer-output-addr" title={address}>
                        {output.alias || address}
                      </span>
                      <strong>{formatAmountWithUnit(Number(output.amount) || 0, unitMode)}</strong>
                    </div>
                  );
                })}
              </div>
            )}
            {highFee && <div className="validation-text">Fee exceeds calculated minimum.</div>}
            <div className="composer-row composer-action-row">
              <button
                className="btn btn-primary btn-small nodrag"
                title={!data.canSignDraft ? data.signDraftDisabledReason : undefined}
                onPointerDown={stopNodeInputEvent}
                onClick={(event) => {
                  event.stopPropagation();
                  if (data.result) void data.onSignDraft?.(data.result);
                }}
                disabled={!data.result || !data.onSignDraft || !data.canSignDraft || data.signingDraft}
              >
                {data.signingDraft ? 'signing...' : 'sign on device'}
              </button>
              <button
                className="btn btn-secondary btn-small nodrag"
                onPointerDown={stopNodeInputEvent}
                onClick={(event) => {
                  event.stopPropagation();
                  downloadBytes(data.result!.filename, data.result!.psnt);
                }}
              >
                download
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

export function PreviewNode({ data, unitMode, onInspect }: NodeProps<PreviewFlowNode> & WithExtras) {
  const amountMeta = [
    data.feeNicks === undefined ? '' : `fee ${formatAmountWithUnit(data.feeNicks, unitMode)}`,
    data.giftNicks === undefined ? '' : formatAmountWithUnit(data.giftNicks, unitMode),
  ].filter(Boolean);
  const copyPreviewValue = () => {
    if (!data.copyValue) return;
    void navigator.clipboard.writeText(data.copyValue);
  };

  return (
    <div className="node-card preview-node-card">
      <NodeResizer minWidth={200} minHeight={80} />
      <div className="node-header preview">
        <span>{data.label}</span>
        <InspectButton onInspect={onInspect} />
      </div>
      <div className="node-body">
        <div>{data.title}</div>
        {[...amountMeta, ...(data.meta ?? [])].map((item) => (
          <div key={item} className="inspector-help">
            {item}
          </div>
        ))}
        {data.mono && <div className="node-mono preview-node-mono">{data.mono}</div>}
        {data.copyValue && (
          <button
            type="button"
            className="btn btn-small btn-secondary preview-copy-btn nodrag"
            onPointerDown={stopNodeInputEvent}
            onClick={(event) => {
              event.stopPropagation();
              copyPreviewValue();
            }}
          >
            copy {data.copyLabel ?? 'value'}
          </button>
        )}
      </div>
      <Handle type="target" id="in" position={Position.Left} isConnectable={false} />
      <Handle type="source" id="out" position={Position.Right} isConnectable={false} />
    </div>
  );
}
