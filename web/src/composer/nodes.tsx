// Composer React Flow node components, extracted from Composer.tsx.
// Verbatim move — pure presentational components driven by node data.
import { Handle, Position, type NodeProps } from '@xyflow/react';

import {
  describeRecipient,
  downloadBytes,
  formatAmountWithUnit,
  isHighFeeSummary,
  parseAmountTextToNicks,
  parseComposeSummary,
  shortAddr,
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

export function AddressNode({ data, unitMode }: NodeProps<AddressFlowNode> & { unitMode: UnitMode }) {
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
      </div>
      <Handle type="target" id="in" position={Position.Left} />
      <Handle type="source" id="out" position={Position.Right} />
    </div>
  );
}

export function NoteNode({ data, unitMode }: NodeProps<NoteFlowNode> & { unitMode: UnitMode }) {
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

export function TxNode({ data, unitMode }: NodeProps<TxFlowNode> & { unitMode: UnitMode }) {
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
      <div className="node-header tx">
        <span>Tx</span>
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
                {summary.outputs.map((output, i) => {
                  const { badge, address } = describeRecipient(output.recipient);
                  const isMultisig = badge !== 'p2pkh';
                  return (
                    <div className="composer-output-row" key={i}>
                      <span className={`composer-lock-badge ${isMultisig ? 'multisig' : ''}`}>
                        {badge}
                      </span>
                      <span className="composer-output-addr" title={address}>
                        {output.alias || shortAddr(address)}
                      </span>
                      <strong>{formatAmountWithUnit(Number(output.amount) || 0, unitMode)}</strong>
                    </div>
                  );
                })}
                <div className="composer-output-note">what the device will show</div>
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

export function PreviewNode({ data, unitMode }: NodeProps<PreviewFlowNode> & { unitMode: UnitMode }) {
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
      <div className="node-header preview">
        <span>{data.label}</span>
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
