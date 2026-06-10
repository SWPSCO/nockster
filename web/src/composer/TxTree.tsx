import { useState } from 'react';
import type { TxTreeNode } from './model';

// Collapsible, human-meaningful transaction tree from wasm `inspect_tx`:
// labeled nodes with b58 hashes, readable amounts, decoded lock primitives.
// Branches default to expanded so the structure is visible at a glance; leaves
// (hashes / amounts) are click-to-copy.

function copy(text: string) {
  if (text && navigator.clipboard) void navigator.clipboard.writeText(text);
}

function TreeRow({ node, depth }: { node: TxTreeNode; depth: number }) {
  const hasChildren = node.children.length > 0;
  // Open fully exploded by default; collapse individual branches as needed.
  const [open, setOpen] = useState(true);

  return (
    <div className="txtree-node" style={{ marginLeft: depth === 0 ? 0 : 12 }}>
      <div className="txtree-row">
        {hasChildren ? (
          <button
            type="button"
            className="txtree-toggle nodrag"
            onClick={() => setOpen((v) => !v)}
            aria-label={open ? 'collapse' : 'expand'}
          >
            {open ? '▾' : '▸'}
          </button>
        ) : (
          <span className="txtree-toggle txtree-leaf-dot">·</span>
        )}
        <span className="txtree-label">{node.label}</span>
        {node.value && (
          <span
            className="txtree-value"
            title={`${node.value} (click to copy)`}
            onClick={() => copy(node.value)}
          >
            {node.value}
          </span>
        )}
      </div>
      {hasChildren && open && (
        <div className="txtree-children">
          {node.children.map((child, i) => (
            <TreeRow key={i} node={child} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}

export function TxTree({ node }: { node: TxTreeNode | null }) {
  if (!node) return null;
  return (
    <div className="txtree">
      <TreeRow node={node} depth={0} />
    </div>
  );
}
