#!/usr/bin/env node
/**
 * Local Nockchain RPC server (MOCK / GATEWAY passthrough / BACKEND passthrough)
 *
 * MODE:
 *   - mock     (default)  -> serve fake responses
 *   - gateway  -> forward to https://fletch.dev.nockblocks.com/wallet-gw/rpc
 *                 and PASS THROUGH caller's headers (Authorization, X-Client-Proof)
 *   - backend  -> forward to https://dev.nockblocks.com/rpc and INJECT service key
 */

const http = require('http');
const https = require('https');
const { URL } = require('url');

const PORT = Number(process.env.RPC_PORT || 8545);
const MODE = (process.env.MODE || '').toLowerCase() || 'mock';

// targets
const GATEWAY_URL = process.env.GATEWAY_URL || 'https://fletch.dev.nockblocks.com/wallet-gw/rpc';
const BACKEND_URL = process.env.BACKEND_URL || 'https://dev.nockblocks.com/wallet-gw/rpc';

// keys (backend mode)
const SERVICE_KEY = process.env.NOCKBLOCKS_API_KEY || '';
// legacy static token path (avoid this; kept for completeness)
const LEGACY_TOKEN = process.env.VITE_RPC_TOKEN || '';

/* ---------------- MOCK STATE ---------------- */
const mockState = {
  blockHeight: 123456,
  addresses: new Map(),
  transactions: [],
};
mockState.addresses.set('nc1q0000000000000000000000000000000000000000', {
  balance: 1000000,
  notes: [
    { noteId: 'note1', address: 'nc1q0000000000000000000000000000000000000000', amount: 500000, spent: false, blockHeight: 123450, txId: 'tx1' },
    { noteId: 'note2', address: 'nc1q0000000000000000000000000000000000000000', amount: 500000, spent: false, blockHeight: 123451, txId: 'tx2' },
  ],
});

const rpcMethods = {
  getNotes: (params) => {
    const { address } = params?.[0] || {};
    const a = mockState.addresses.get(address);
    return a ? { nicks: a.balance, notes: a.notes } : { nicks: 0, notes: [] };
  },
  getBlockchainMetrics: () => ({
    blockHeight: mockState.blockHeight,
    totalTransactions: mockState.transactions.length,
    totalAddresses: mockState.addresses.size,
    networkHashrate: 1234567890,
    difficulty: 999999,
    lastBlockTime: Date.now() - 30000,
  }),
  getTransactionsByAddress: (params) => {
    const { address } = params?.[0] || {};
    return { transactions: mockState.transactions.filter(tx => tx.from === address || tx.to === address) };
  },
  submitTransaction: (params) => {
    const txId = `tx${Date.now()}`;
    const tx = { txId, timestamp: Date.now(), status: 'pending', ...params?.[0] };
    mockState.transactions.push(tx);
    setTimeout(() => {
      tx.status = 'confirmed';
      tx.blockHeight = ++mockState.blockHeight;
      tx.confirmations = 1;
    }, 2000);
    return { txId };
  },
  getTransaction: (params) => {
    const { txId } = params?.[0] || {};
    const tx = mockState.transactions.find(t => t.txId === txId);
    if (!tx) throw new Error(`Transaction ${txId} not found`);
    return tx;
  },
  getBlockByHeight: (params) => {
    const { height } = params?.[0] || {};
    return {
      height,
      hash: `block_${height}`,
      timestamp: Date.now() - ((mockState.blockHeight - height) * 60000),
      transactions: mockState.transactions.filter(tx => tx.blockHeight === height),
    };
  },
  ping: () => ({ status: 'ok', timestamp: Date.now() }),
};

/* ---------------- UTILS ---------------- */
function sendJSON(res, code, obj, extraHeaders = {}) {
  res.writeHead(code, {
    'Content-Type': 'application/json',
    ...extraHeaders,
  });
  res.end(typeof obj === 'string' ? obj : JSON.stringify(obj));
}

function corsHeaders(req) {
  const origin = req.headers.origin || '*';
  // allow the headers your real client sends
  return {
    'Access-Control-Allow-Origin': origin,
    'Vary': 'Origin',
    'Access-Control-Allow-Methods': 'POST, GET, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type, Accept, Authorization, X-Client-Proof',
    'Access-Control-Max-Age': '86400',
  };
}

function proxyJSON(target, { method = 'POST', headers = {}, body = '' }, mutateHeadersFn) {
  return new Promise((resolve, reject) => {
    const u = new URL(target);
    const opts = {
      protocol: u.protocol,
      hostname: u.hostname,
      port: u.port || (u.protocol === 'https:' ? 443 : 80),
      path: u.pathname + (u.search || ''),
      method,
      headers: { ...headers },
    };

    if (mutateHeadersFn) mutateHeadersFn(opts.headers);

    const hmod = u.protocol === 'https:' ? https : http;
    const req = hmod.request(opts, (res) => {
      let data = '';
      res.setEncoding('utf8');
      res.on('data', (chunk) => (data += chunk));
      res.on('end', () => resolve({ status: res.statusCode || 502, headers: res.headers, body: data }));
    });
    req.on('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

/* ---------------- SERVER ---------------- */
const server = http.createServer(async (req, res) => {
  // CORS/preflight
  if (req.method === 'OPTIONS') {
    res.writeHead(204, corsHeaders(req));
    res.end();
    return;
  }
  if (req.method !== 'POST') {
    return sendJSON(res, 405, { error: 'Method not allowed' }, corsHeaders(req));
  }

  // read body
  let body = '';
  req.on('data', (c) => (body += c));
  req.on('end', async () => {
    const cors = corsHeaders(req);

    if (MODE === 'gateway') {
      // forward to the gateway, pass through caller's auth headers (so the extension can add them)
      try {
        const passthroughHeaders = {
          'content-type': 'application/json',
        };
        // pass along Authorization and X-Client-Proof if present
        if (req.headers.authorization) passthroughHeaders['authorization'] = req.headers.authorization;
        if (req.headers['x-client-proof']) passthroughHeaders['x-client-proof'] = req.headers['x-client-proof'];

        const out = await proxyJSON(GATEWAY_URL, {
          headers: passthroughHeaders,
          body,
        });

        sendJSON(res, out.status, out.body, cors);
      } catch (err) {
        console.error('[GATEWAY] error:', err.message || err);
        sendJSON(res, 502, { jsonrpc: '2.0', error: { code: -32603, message: `Gateway error: ${err.message || err}` }, id: null }, cors);
      }
      return;
    }

    if (MODE === 'backend') {
      // forward directly to backend with service key (skips device auth; DEV ONLY)
      if (!SERVICE_KEY && !LEGACY_TOKEN) {
        return sendJSON(res, 500, { jsonrpc: '2.0', error: { code: -32603, message: 'NOCKBLOCKS_API_KEY not set' }, id: null }, cors);
        }
      try {
        const out = await proxyJSON(BACKEND_URL, {
          headers: {
            'content-type': 'application/json',
            'authorization': `Bearer ${SERVICE_KEY || LEGACY_TOKEN}`,
            'x-client-tier': 'wallet',
            'x-device': 'dev-proxy',
          },
          body,
        });
        sendJSON(res, out.status, out.body, cors);
      } catch (err) {
        console.error('[BACKEND] error:', err.message || err);
        sendJSON(res, 502, { jsonrpc: '2.0', error: { code: -32603, message: `Backend error: ${err.message || err}` }, id: null }, cors);
      }
      return;
    }

    // MOCK
    try {
      const reqJson = JSON.parse(body || '{}');
      const { method, params, id } = reqJson;

      if (!rpcMethods[method]) {
        return sendJSON(res, 200, { jsonrpc: '2.0', error: { code: -32601, message: `Method '${method}' not found` }, id }, cors);
      }
      try {
        const result = rpcMethods[method](params);
        return sendJSON(res, 200, { jsonrpc: '2.0', result, id }, cors);
      } catch (e) {
        return sendJSON(res, 200, { jsonrpc: '2.0', error: { code: -32603, message: e.message }, id }, cors);
      }
    } catch {
      return sendJSON(res, 400, { jsonrpc: '2.0', error: { code: -32700, message: 'Parse error' }, id: null }, cors);
    }
  });
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`🚀 RPC dev server on http://127.0.0.1:${PORT}`);
  console.log(`Mode: ${MODE.toUpperCase()}  (${MODE === 'gateway' ? GATEWAY_URL : MODE === 'backend' ? BACKEND_URL : 'mock'})`);
  if (MODE === 'mock') {
    console.log('Methods:'); Object.keys(rpcMethods).forEach(m => console.log('  -', m));
    console.log('Test address: nc1q0000000000000000000000000000000000000000');
  }
});

process.on('SIGINT', () => { console.log('\n👋 shutting down'); server.close(() => process.exit(0)); });
