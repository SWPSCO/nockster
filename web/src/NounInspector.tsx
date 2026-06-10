import { useState } from 'react';

// Mirrors the wasm `NounView` (serde tag = "kind").
type NounView =
  | {
      kind: 'atom';
      num: string | null;
      hex: string;
      text: string | null;
      tag: string | null;
      bytes: number;
    }
  | { kind: 'list'; items: NounView[]; truncated: boolean }
  | { kind: 'cell'; head: NounView; tail: NounView }
  | { kind: 'elided' };

interface InspectFn {
  inspect_noun(bytes: Uint8Array): unknown;
}

function AtomView({ node }: { node: Extract<NounView, { kind: 'atom' }> }) {
  // Prefer the most informative single rendering, but keep hex available.
  const primary =
    node.tag != null
      ? `%${node.tag}`
      : node.text != null
        ? `"${node.text}"`
        : node.num != null
          ? node.num
          : node.hex;
  const showHex = primary !== node.hex && node.bytes > 0;
  return (
    <span className="noun-atom">
      <span className="noun-atom-primary">{primary}</span>
      {showHex && <span className="noun-atom-hex"> {node.hex}</span>}
    </span>
  );
}

function NodeView({ node, depth }: { node: NounView; depth: number }) {
  const [open, setOpen] = useState(depth < 3);
  if (node.kind === 'atom') return <AtomView node={node} />;
  if (node.kind === 'elided') return <span className="noun-elided">…(elided)</span>;

  const children: NounView[] =
    node.kind === 'list' ? node.items : [node.head, node.tail];
  const label =
    node.kind === 'list'
      ? `[ ${node.items.length}${node.truncated ? '+' : ''} ]`
      : '[head . tail]';

  return (
    <div className="noun-node">
      <button type="button" className="noun-toggle" onClick={() => setOpen((v) => !v)}>
        {open ? '▾' : '▸'} {label}
      </button>
      {open && (
        <div className="noun-children">
          {children.map((child, i) => (
            <div className="noun-child" key={i}>
              {node.kind === 'cell' && (
                <span className="noun-child-label">{i === 0 ? 'head' : 'tail'}: </span>
              )}
              <NodeView node={child} depth={depth + 1} />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Upload any jammed noun (.tx / .psnt / keys.export / .sig / .jam) and view it
 * as a typed tree. Read-only, local; nothing is sent anywhere.
 */
export function NounInspector({ wasm }: { wasm: InspectFn | null }) {
  const [view, setView] = useState<NounView | null>(null);
  const [error, setError] = useState('');
  const [name, setName] = useState('');

  const onFile = async (file: File) => {
    setError('');
    setView(null);
    setName(file.name);
    try {
      if (!wasm) throw new Error('WASM not ready yet');
      const bytes = new Uint8Array(await file.arrayBuffer());
      setView(wasm.inspect_noun(bytes) as NounView);
    } catch (e: any) {
      setError(e?.message ?? String(e));
    }
  };

  return (
    <div className="section device-panel noun-inspector">
      <div className="device-panel-header">
        <h2>Noun inspector</h2>
        <label className="btn btn-small btn-secondary">
          <input
            type="file"
            style={{ display: 'none' }}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void onFile(f);
              e.target.value = '';
            }}
          />
          open jam…
        </label>
      </div>
      <p className="seed-subtitle">
        Decode any jammed noun (.tx, .psnt, keys.export, .sig) into a typed tree.
        Local and read-only.
      </p>
      {name && !error && <div className="device-inline-status">{name}</div>}
      {error && <div className="device-inline-status">{error}</div>}
      {view && (
        <div className="noun-tree">
          <NodeView node={view} depth={0} />
        </div>
      )}
    </div>
  );
}
